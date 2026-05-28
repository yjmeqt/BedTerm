//! Saved-hosts persistence — Rust-owned, backed by `NSUserDefaults` (order
//! index) + iOS Keychain (per-UUID `SavedHost` JSON blob).
//!
//! Replaces the Swift `HostsStore` storage layer. Swift retains the
//! `SavedHost` / `HostCredential` Codable types and encodes/decodes the
//! blobs; this module is opaque to the blob shape except when building
//! the display-snapshot JSON, where it extracts a handful of fields by
//! treating the blob as `serde_json::Value`.
//!
//! Threading: callable from any thread. The Keychain (`SecItem*`) and
//! `NSUserDefaults` are both thread-safe; production callers run on the
//! main actor so no internal locking is needed beyond what the OS gives
//! us.

#![cfg(target_os = "ios")]

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{TCFType, ToVoid};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::CFTypeRef;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSString, NSUserDefaults};
use security_framework_sys::access_control::kSecAttrAccessibleWhenUnlockedThisDeviceOnly;
use security_framework_sys::base::{errSecItemNotFound, errSecSuccess};
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrService, kSecClass, kSecClassGenericPassword, kSecMatchLimit,
    kSecMatchLimitAll, kSecReturnAttributes, kSecReturnData, kSecValueData,
};
use security_framework_sys::keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete};

// `kSecAttrAccessible` isn't re-exported by security-framework-sys; declare
// the symbol ourselves. Linked from `Security.framework` (already pulled
// in by the crate). On iOS the constant resolves at runtime to the same
// CFStringRef the system uses for the attribute key.
#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecAttrAccessible: CFStringRef;
}
use std::collections::HashMap;
use std::ptr;
use std::sync::{Mutex, RwLock};

const SERVICE: &str = "com.applovin.yi.bedterm.savedHosts";
const ORDER_KEY: &str = "hosts.order";

// Test-mode state. When `TestMode::Memory(...)` is active every Keychain
// + UserDefaults call short-circuits to an in-process `HashMap` + `Vec`.
// SPM xctest bundles have no host-app entitlement, so production
// `SecItem*` calls return errSecMissingEntitlement (-34018) — matches
// what the Swift `InMemoryKeychainBackend` worked around pre-port.
//
// Production code path: `TestMode::Production` — real Keychain, real
// `NSUserDefaults.standard`, real `hosts.order` key.
#[derive(Default)]
struct MemoryBackend {
    blobs: HashMap<String, Vec<u8>>,
    order: Vec<String>,
}

enum TestMode {
    Production,
    Memory(RwLock<MemoryBackend>),
}

static MODE: RwLock<TestMode> = RwLock::new(TestMode::Production);

fn with_memory<R>(f: impl FnOnce(&mut MemoryBackend) -> R) -> Option<R> {
    let guard = MODE.read().ok()?;
    if let TestMode::Memory(lock) = &*guard {
        let mut mem = lock.write().ok()?;
        return Some(f(&mut mem));
    }
    None
}

/// Install or clear the test-mode in-memory backend. Pass `Some((_, _))`
/// to install (the strings exist only to namespace different test cases
/// in logs / debug output — storage isolation is automatic because each
/// install resets the map). Pass `(None, None)` to restore the real
/// Keychain + UserDefaults backend.
///
/// Production code never calls this; the Swift test bundle reaches it
/// via `bt_ios_hosts_set_test_service`.
pub fn set_test_override(service: Option<&str>, order_key: Option<&str>) {
    let mut guard = MODE.write().expect("hosts_store MODE poisoned");
    match (service, order_key) {
        (Some(_), Some(_)) => *guard = TestMode::Memory(RwLock::new(MemoryBackend::default())),
        _ => *guard = TestMode::Production,
    }
}

// ── CFType helpers ────────────────────────────────────────────────────────

fn cf_class() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecClass as _) }
}
fn cf_class_generic() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecClassGenericPassword as _) }
}
fn cf_attr_service() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrService as _) }
}
fn cf_attr_account() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrAccount as _) }
}
fn cf_attr_accessible() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrAccessible as _) }
}
fn cf_accessible_value() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecAttrAccessibleWhenUnlockedThisDeviceOnly as _) }
}
fn cf_value_data() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecValueData as _) }
}
fn cf_return_data() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecReturnData as _) }
}
fn cf_return_attrs() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecReturnAttributes as _) }
}
fn cf_match_limit() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecMatchLimit as _) }
}
fn cf_limit_all() -> CFString {
    unsafe { CFString::wrap_under_get_rule(kSecMatchLimitAll as _) }
}

// ── Keychain primitives ───────────────────────────────────────────────────

fn save_blob(uuid: &str, bytes: &[u8]) -> bool {
    if let Some(()) = with_memory(|mem| {
        mem.blobs.insert(uuid.to_string(), bytes.to_vec());
    }) {
        return true;
    }
    let svc_cf = CFString::new(SERVICE);
    let acct = CFString::new(uuid);

    // Delete any prior item under the same (service, account) so the
    // following Add is idempotent. Matches the Swift impl.
    let del_pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct.as_CFType()),
    ];
    let del = CFDictionary::from_CFType_pairs(&del_pairs);
    unsafe {
        SecItemDelete(del.as_concrete_TypeRef());
    }

    let data = CFData::from_buffer(bytes);
    let add_pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct.as_CFType()),
        (cf_value_data().as_CFType(), data.as_CFType()),
        (
            cf_attr_accessible().as_CFType(),
            cf_accessible_value().as_CFType(),
        ),
    ];
    let add = CFDictionary::from_CFType_pairs(&add_pairs);
    let status = unsafe { SecItemAdd(add.as_concrete_TypeRef(), ptr::null_mut()) };
    status == errSecSuccess
}

fn load_blob_raw(uuid: &str) -> Option<Vec<u8>> {
    if let Some(found) = with_memory(|mem| mem.blobs.get(uuid).cloned()) {
        return found;
    }
    let svc_cf = CFString::new(SERVICE);
    let acct = CFString::new(uuid);

    let pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct.as_CFType()),
        (
            cf_return_data().as_CFType(),
            CFBoolean::true_value().as_CFType(),
        ),
    ];
    let q = CFDictionary::from_CFType_pairs(&pairs);
    let mut result: CFTypeRef = ptr::null();
    let status = unsafe { SecItemCopyMatching(q.as_concrete_TypeRef(), &mut result) };
    if status == errSecItemNotFound {
        return None;
    }
    if status != errSecSuccess || result.is_null() {
        return None;
    }
    let data: CFData = unsafe { CFData::wrap_under_create_rule(result as _) };
    Some(data.bytes().to_vec())
}

fn delete_blob_raw(uuid: &str) {
    if let Some(()) = with_memory(|mem| {
        mem.blobs.remove(uuid);
    }) {
        return;
    }
    let svc_cf = CFString::new(SERVICE);
    let acct = CFString::new(uuid);
    let pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct.as_CFType()),
    ];
    let q = CFDictionary::from_CFType_pairs(&pairs);
    unsafe {
        SecItemDelete(q.as_concrete_TypeRef());
    }
}

fn all_accounts() -> Vec<String> {
    if let Some(out) = with_memory(|mem| mem.blobs.keys().cloned().collect::<Vec<_>>()) {
        return out;
    }
    let svc_cf = CFString::new(SERVICE);

    let pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (
            cf_return_attrs().as_CFType(),
            CFBoolean::true_value().as_CFType(),
        ),
        (cf_match_limit().as_CFType(), cf_limit_all().as_CFType()),
    ];
    let q = CFDictionary::from_CFType_pairs(&pairs);

    let mut result: CFTypeRef = ptr::null();
    let status = unsafe { SecItemCopyMatching(q.as_concrete_TypeRef(), &mut result) };
    if status != errSecSuccess || result.is_null() {
        return Vec::new();
    }
    let arr: CFArray<CFDictionary> =
        unsafe { CFArray::wrap_under_create_rule(result as CFArrayRef) };
    let acct_key = cf_attr_account();
    let mut out = Vec::with_capacity(arr.len() as usize);
    for dict in arr.iter() {
        // `find` returns Option<&V> where V is CFType — but our generic
        // is CFDictionary so we step down to the raw level.
        let dict_ref = dict.as_concrete_TypeRef();
        if let Some(s) = extract_string(dict_ref, &acct_key) {
            out.push(s);
        }
    }
    out
}

fn extract_string(dict: CFDictionaryRef, key: &CFString) -> Option<String> {
    use core_foundation_sys::dictionary::CFDictionaryGetValueIfPresent;
    let mut value: *const std::ffi::c_void = ptr::null();
    let present = unsafe { CFDictionaryGetValueIfPresent(dict, key.to_void(), &mut value) };
    if present == 0 || value.is_null() {
        return None;
    }
    let s: CFString = unsafe { CFString::wrap_under_get_rule(value as _) };
    Some(s.to_string())
}

// ── UserDefaults order index ──────────────────────────────────────────────

fn defaults() -> Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}

fn read_order() -> Vec<String> {
    if let Some(out) = with_memory(|mem| mem.order.clone()) {
        return out;
    }
    let key = NSString::from_str(ORDER_KEY);
    let d = defaults();
    let arr: Option<Retained<NSArray>> = unsafe { msg_send![&*d, arrayForKey: &*key] };
    let Some(arr) = arr else {
        return Vec::new();
    };
    let count: usize = unsafe { msg_send![&*arr, count] };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let obj: *mut AnyObject = unsafe { msg_send![&*arr, objectAtIndex: i] };
        if obj.is_null() {
            continue;
        }
        // NSUserDefaults round-trips strings as NSString; cast and read.
        let ns: &NSString = unsafe { &*(obj as *const NSString) };
        out.push(ns.to_string());
    }
    out
}

fn write_order(order: &[String]) {
    if let Some(()) = with_memory(|mem| {
        mem.order = order.to_vec();
    }) {
        return;
    }
    let key = NSString::from_str(ORDER_KEY);
    let d = defaults();
    let ns_strings: Vec<Retained<NSString>> = order.iter().map(|s| NSString::from_str(s)).collect();
    let refs: Vec<&NSString> = ns_strings.iter().map(|r| r.as_ref()).collect();
    let arr = NSArray::from_slice(&refs);
    let _: () = unsafe { msg_send![&*d, setObject: &*arr, forKey: &*key] };
}

// ── Reconciliation + snapshot ────────────────────────────────────────────

fn reconcile_order() -> Vec<String> {
    let stored = read_order();
    let keychain = all_accounts();
    let keychain_set: std::collections::HashSet<&String> = keychain.iter().collect();

    let valid_ordered: Vec<String> = stored
        .iter()
        .filter(|id| keychain_set.contains(*id))
        .cloned()
        .collect();
    let valid_set: std::collections::HashSet<&String> = valid_ordered.iter().collect();

    let mut orphans: Vec<String> = keychain
        .iter()
        .filter(|id| !valid_set.contains(*id))
        .cloned()
        .collect();
    orphans.sort();

    let resolved: Vec<String> = valid_ordered.into_iter().chain(orphans).collect();

    if resolved != stored {
        write_order(&resolved);
    }
    resolved
}

/// Returns the saved-hosts display snapshot as JSON in stored order.
/// Each entry is `{id, label, host, port, username, authIsKey}`. Blobs
/// that fail to parse are logged + skipped.
pub fn list_snapshot_json() -> String {
    let order = reconcile_order();
    // Prepend UI-test injected entries before the Keychain-backed rows.
    let injected = INJECTED.lock().unwrap_or_else(|p| p.into_inner());
    let mut items: Vec<serde_json::Value> = injected.clone();
    items.reserve(order.len());
    for uuid in &order {
        let Some(bytes) = load_blob_raw(uuid) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            eprintln!("hosts_store: failed to parse blob for {uuid} as JSON; skipping");
            continue;
        };
        let Some(obj) = value.as_object() else {
            eprintln!("hosts_store: blob for {uuid} is not a JSON object; skipping");
            continue;
        };
        let id = obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(uuid)
            .to_string();
        let label = obj
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cred = match obj.get("credential").and_then(|v| v.as_object()) {
            Some(c) => c,
            None => {
                eprintln!("hosts_store: blob for {uuid} is missing credential; skipping");
                continue;
            }
        };
        let host = cred
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let port = cred.get("port").and_then(|v| v.as_i64()).unwrap_or(22);
        let username = cred
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // Swift's Codable AuthMethod encodes a single-key dictionary with
        // either `password` or `privateKey` under the `auth` field.
        let auth_is_key = cred
            .get("auth")
            .and_then(|v| v.as_object())
            .map(|o| o.contains_key("privateKey"))
            .unwrap_or(false);

        items.push(serde_json::json!({
            "id": id,
            "label": label,
            "host": host,
            "port": port,
            "username": username,
            "authIsKey": auth_is_key,
        }));
    }
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

// ── UI-test injected entries ─────────────────────────────────────────────

/// In-memory stub entries injected via `-uitest-injectStubHost`. These
/// live here (not in `hosts_vm`) so `list_snapshot_json` can surface them
/// to the Rust hosts VC, which reads from the store directly.
static INJECTED: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());

/// Append JSON-encoded display-snapshot entries to the in-memory injection
/// buffer. Callers must also call `hosts_vm::merge_injected` to keep the VM
/// in sync for the Swift-side `HostsViewModel.entries` mirror.
pub fn merge_injected(json: &str) {
    let Ok(extras) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return;
    };
    let mut injected = INJECTED.lock().unwrap_or_else(|p| p.into_inner());
    for extra in extras {
        let id = extra.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if !injected
            .iter()
            .any(|e| e.get("id").and_then(|v| v.as_str()) == Some(id))
        {
            injected.push(extra);
        }
    }
}

/// Test-only — clear the injected entries buffer.
pub fn test_clear_injected() {
    INJECTED.lock().unwrap_or_else(|p| p.into_inner()).clear();
}

/// Public API consumed by the FFI surface.
pub fn load_blob(uuid: &str) -> Option<Vec<u8>> {
    load_blob_raw(uuid)
}

/// Load the raw JSON blob for a given UUID as a string.
/// Returns `None` when no blob exists or the bytes aren't valid UTF-8.
pub fn load_json(uuid: &str) -> Option<String> {
    load_blob_raw(uuid).and_then(|bytes| String::from_utf8(bytes).ok())
}

pub fn save(uuid: &str, bytes: &[u8]) -> bool {
    if !save_blob(uuid, bytes) {
        return false;
    }
    let mut order = read_order();
    if !order.iter().any(|s| s == uuid) {
        order.push(uuid.to_string());
        write_order(&order);
    }
    true
}

/// Test-only — wipe the order index without touching blobs. Used by
/// `HostsStoreTests.reconcileOrphans` to assert that `list_snapshot_json`
/// rebuilds the order from surviving Keychain items.
pub fn test_clear_order() {
    with_memory(|mem| {
        mem.order.clear();
    });
}

pub fn delete(uuid: &str) {
    delete_blob_raw(uuid);
    let mut order = read_order();
    let before = order.len();
    order.retain(|s| s != uuid);
    if order.len() != before {
        write_order(&order);
    }
}
