//! Per-host SSH host-key fingerprint persistence — Rust-owned, backed by
//! the iOS Keychain.
//!
//! Replaces the Swift `HostKeyStore` storage layer. The account key is
//! `"{host}:{port}"`; the stored value is the UTF-8 fingerprint string
//! (`SHA256:<base64-no-padding>` in production). Same accessibility
//! attribute as `hosts_store` (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`).
//!
//! Test seam: `set_test_service` swaps the backend over to an in-process
//! `HashMap` keyed by `(host, port)`. SPM xctest bundles run without a
//! host-app entitlement and therefore can't talk to the real Keychain —
//! same workaround `hosts_store` uses.

#![cfg(target_os = "ios")]

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::CFTypeRef;
use security_framework_sys::access_control::kSecAttrAccessibleWhenUnlockedThisDeviceOnly;
use security_framework_sys::base::{errSecItemNotFound, errSecSuccess};
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrService, kSecClass, kSecClassGenericPassword, kSecReturnData,
    kSecValueData,
};
use security_framework_sys::keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete};

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecAttrAccessible: CFStringRef;
}

use std::collections::HashMap;
use std::ptr;
use std::sync::RwLock;

const SERVICE: &str = "com.applovin.yi.bedterm.hostkeys";

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Verdict {
    Match,
    Mismatch { stored: String },
    Unknown,
}

#[derive(Default)]
struct MemoryBackend {
    items: HashMap<String, String>,
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

/// Install or clear the test-mode in-memory backend. Pass `Some(_)` to
/// install; the namespace string only appears in debug logs because each
/// install resets the map. Pass `None` to restore the real Keychain.
pub fn set_test_override(service: Option<&str>) {
    let mut guard = MODE.write().expect("host_key_store MODE poisoned");
    match service {
        Some(_) => *guard = TestMode::Memory(RwLock::new(MemoryBackend::default())),
        None => *guard = TestMode::Production,
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
fn account(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

// ── Keychain primitives ───────────────────────────────────────────────────

pub fn load(host: &str, port: u16) -> Option<String> {
    let acct = account(host, port);
    if let Some(found) = with_memory(|mem| mem.items.get(&acct).cloned()) {
        return found;
    }
    let svc_cf = CFString::new(SERVICE);
    let acct_cf = CFString::new(&acct);
    let pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct_cf.as_CFType()),
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
    String::from_utf8(data.bytes().to_vec()).ok()
}

pub fn save(host: &str, port: u16, fingerprint: &str) -> bool {
    let acct = account(host, port);
    if let Some(()) = with_memory(|mem| {
        mem.items.insert(acct.clone(), fingerprint.to_string());
    }) {
        return true;
    }
    let svc_cf = CFString::new(SERVICE);
    let acct_cf = CFString::new(&acct);

    // Delete prior entry so the Add below is idempotent.
    let del_pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct_cf.as_CFType()),
    ];
    let del = CFDictionary::from_CFType_pairs(&del_pairs);
    unsafe {
        SecItemDelete(del.as_concrete_TypeRef());
    }

    let data = CFData::from_buffer(fingerprint.as_bytes());
    let add_pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct_cf.as_CFType()),
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

pub fn delete(host: &str, port: u16) {
    let acct = account(host, port);
    if let Some(()) = with_memory(|mem| {
        mem.items.remove(&acct);
    }) {
        return;
    }
    let svc_cf = CFString::new(SERVICE);
    let acct_cf = CFString::new(&acct);
    let pairs = [
        (cf_class().as_CFType(), cf_class_generic().as_CFType()),
        (cf_attr_service().as_CFType(), svc_cf.as_CFType()),
        (cf_attr_account().as_CFType(), acct_cf.as_CFType()),
    ];
    let q = CFDictionary::from_CFType_pairs(&pairs);
    unsafe {
        SecItemDelete(q.as_concrete_TypeRef());
    }
}

pub fn verify(host: &str, port: u16, remote: &str) -> Verdict {
    match load(host, port) {
        None => Verdict::Unknown,
        Some(stored) if stored == remote => Verdict::Match,
        Some(stored) => Verdict::Mismatch { stored },
    }
}
