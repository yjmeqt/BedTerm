#![allow(dead_code)]
//! Pure state machine for the saved-hosts list (formerly Swift
//! `HostsViewModel`). Owns the entry array, in-flight/current-session
//! bookkeeping, and the swap/mismatch/delete confirmation pieces.
//!
//! The Swift layer keeps the @Observable surface + the async SSH connect
//! path (`ConnectAttempt`, `TerminalSession`). Mutators here return an
//! [`Action`] enum telling Swift what side effect to perform next:
//! actually start an SSH attempt, disconnect a live session, etc. The
//! Swift VM mirrors Rust state into its @Observable properties on every
//! mutation so `withObservationTracking` in `HostsConnectController`
//! keeps firing on the same key paths it did pre-port.
//!
//! No UIKit imports — this module is host-testable. The FFI singleton +
//! C exports live in [`crate::ffi::hosts`] (iOS-gated). When building for
//! macOS the FFI consumer is not compiled, so the public API appears dead.

use serde::{Deserialize, Serialize};
use std::ffi::{c_char, c_void};
use std::sync::Mutex;

/// Display-shape mirror of `SavedHost`. Rust never sees the full
/// `HostCredential` (the auth method etc.) — Swift owns that schema —
/// but the VM needs enough to drive `displayName(for:)` and the row
/// snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntryMeta {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(rename = "authIsKey", default)]
    pub auth_is_key: bool,
}

impl EntryMeta {
    fn display_name(&self) -> String {
        if !self.label.is_empty() {
            return self.label.clone();
        }
        format!("{}@{}", self.username, self.host)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingMismatch {
    pub stored: String,
    pub remote: String,
    pub host: String,
    pub port: u16,
    #[serde(rename = "sourceID")]
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwapConfirmation {
    #[serde(rename = "targetID")]
    pub target_id: String,
    /// Only used Rust-side for alert formatting (`swap_alert()`).
    /// Swift no longer decodes this field — see HostsViewModelState trim.
    #[serde(default, skip_serializing)]
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntryRef {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteConfirmation {
    #[serde(rename = "targetID")]
    pub target_id: String,
    /// Only used Rust-side for alert formatting (`delete_alert()`).
    /// Swift no longer decodes this field — see HostsViewModelState trim.
    #[serde(default, skip_serializing)]
    pub display_name: String,
    /// Only used Rust-side for state cleanup (`confirm_delete()`).
    /// Swift no longer decodes this field — see HostsViewModelState trim.
    #[serde(default, skip_serializing)]
    pub is_live: bool,
    /// Only used Rust-side for state cleanup (`confirm_delete()`).
    /// Swift no longer decodes this field — see HostsViewModelState trim.
    #[serde(default, skip_serializing)]
    pub is_in_flight: bool,
}

/// Outcome of a mutator that needs Swift to perform an asynchronous side
/// effect. `None` means "state was updated, nothing else to do."
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    /// Start an SSH connect attempt for this entry id. Swift's
    /// `runConnect(id:)` handles the async dispatch.
    Connect(String),
    /// Disconnect the live `TerminalSession`. Swift owns the session
    /// object; the VM merely tracks that we *had* one.
    Disconnect,
}

pub type ConnectFn = Option<unsafe extern "C" fn(*mut c_void, *const c_char)>;
pub type DisconnectFn = Option<unsafe extern "C" fn(*mut c_void)>;
pub type AlertFn = Option<
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *const c_char, *const c_char),
>;

/// Wraps an opaque `*mut c_void` context pointer so it is `Send` + `Sync`.
/// Swift sets this once at initialisation with a heap-allocated box; the
/// pointer is only read back on the main thread when a callback fires.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct CallbackCtx(pub *mut c_void);

// SAFETY: CallbackCtx is set once on the main thread and only accessed
// from the main thread when callbacks fire. The opaque pointer is never
// dereferenced by Rust — it is an opaque cookie for the Swift side.
unsafe impl Send for CallbackCtx {}
unsafe impl Sync for CallbackCtx {}

impl CallbackCtx {
    pub const fn null() -> Self {
        Self(std::ptr::null_mut())
    }
}

#[derive(Debug, Clone)]
pub struct HostsVM {
    pub entries: Vec<EntryMeta>,
    pub in_flight_id: Option<String>,
    pub current_session_id: Option<String>,
    pub pending_mismatch: Option<PendingMismatch>,
    pub swap_confirmation: Option<SwapConfirmation>,
    pub delete_confirmation: Option<DeleteConfirmation>,
    pub load_failed: bool,

    // Side-effect callbacks installed by Swift (Phase 2 migration).
    // Fire on the calling thread; the Swift host dispatches to MainActor.
    pub connect_cb: ConnectFn,
    pub connect_ctx: CallbackCtx,
    pub disconnect_cb: DisconnectFn,
    pub disconnect_ctx: CallbackCtx,
    pub alert_cb: AlertFn,
    pub alert_ctx: CallbackCtx,
}

impl Default for HostsVM {
    fn default() -> Self {
        Self::new()
    }
}

impl HostsVM {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            in_flight_id: None,
            current_session_id: None,
            pending_mismatch: None,
            swap_confirmation: None,
            delete_confirmation: None,
            load_failed: false,
            connect_cb: None,
            connect_ctx: CallbackCtx::null(),
            disconnect_cb: None,
            disconnect_ctx: CallbackCtx::null(),
            alert_cb: None,
            alert_ctx: CallbackCtx::null(),
        }
    }

    // MARK: - Side-effect callbacks (Phase 2)

    /// Install C callbacks that the VM fires instead of returning Action
    /// discriminants. Swift installs these once at initialisation; when set,
    /// each mutator that would return `Action::Connect` / `Disconnect` fires
    /// the callback directly in addition to the normal return value.
    pub fn set_callbacks(
        &mut self,
        connect_cb: ConnectFn,
        connect_ctx: CallbackCtx,
        disconnect_cb: DisconnectFn,
        disconnect_ctx: CallbackCtx,
        alert_cb: AlertFn,
        alert_ctx: CallbackCtx,
    ) {
        self.connect_cb = connect_cb;
        self.connect_ctx = connect_ctx;
        self.disconnect_cb = disconnect_cb;
        self.disconnect_ctx = disconnect_ctx;
        self.alert_cb = alert_cb;
        self.alert_ctx = alert_ctx;
    }

    /// Fire the connect callback if installed. Safe to call with any id;
    /// the Swift host validates the UUID and dispatches an SSH attempt.
    pub fn fire_connect(&self, id: &str) {
        if let Some(cb) = self.connect_cb {
            let c_id = std::ffi::CString::new(id).unwrap_or_default();
            unsafe { cb(self.connect_ctx.0, c_id.as_ptr()) };
        }
    }

    pub fn fire_disconnect(&self) {
        if let Some(cb) = self.disconnect_cb {
            unsafe { cb(self.disconnect_ctx.0) };
        }
    }

    /// Fire the alert callback. All four strings are NUL-terminated C
    /// strings borrowed for the duration of the call.
    pub fn fire_alert(&self, title: &str, message: &str, confirm: &str, cancel: &str) {
        if let Some(cb) = self.alert_cb {
            let t = std::ffi::CString::new(title).unwrap_or_default();
            let m = std::ffi::CString::new(message).unwrap_or_default();
            let y = std::ffi::CString::new(confirm).unwrap_or_default();
            let n = std::ffi::CString::new(cancel).unwrap_or_default();
            unsafe {
                cb(
                    self.alert_ctx.0,
                    t.as_ptr(),
                    m.as_ptr(),
                    y.as_ptr(),
                    n.as_ptr(),
                )
            };
        }
    }

    /// Replace the entry array with the parsed display snapshot. Clears
    /// `load_failed`. JSON parse failure leaves entries empty and sets
    /// `load_failed = true`.
    pub fn set_entries_from_snapshot(&mut self, json: &str) {
        match serde_json::from_str::<Vec<EntryMeta>>(json) {
            Ok(parsed) => {
                self.entries = parsed;
                self.load_failed = false;
            }
            Err(_) => {
                self.entries.clear();
                self.load_failed = true;
            }
        }
    }

    /// UI-test seam — append entries that aren't already present. The
    /// snapshot JSON comes from `HostsStoreInjection.current` on the
    /// Swift side and never touches the Keychain.
    pub fn merge_injected(&mut self, json: &str) {
        let Ok(extras) = serde_json::from_str::<Vec<EntryMeta>>(json) else {
            return;
        };
        for extra in extras {
            if !self.entries.iter().any(|e| e.id == extra.id) {
                self.entries.push(extra);
            }
        }
    }

    pub fn set_load_failed(&mut self, value: bool) {
        self.load_failed = value;
    }

    pub fn display_name_for(&self, id: &str) -> String {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map(EntryMeta::display_name)
            .unwrap_or_default()
    }

    // MARK: - Connect

    /// Row's Connect tap. Cancels the swap dance for the in-flight id,
    /// raises a swap-confirm when a *different* session is live, or
    /// asks Swift to start the SSH attempt directly.
    pub fn request_connect(&mut self, id: &str) -> Action {
        if self.in_flight_id.as_deref() == Some(id) {
            return Action::None;
        }
        if let Some(session_id) = self.current_session_id.as_deref() {
            if session_id != id {
                self.swap_confirmation = Some(SwapConfirmation {
                    target_id: id.to_string(),
                    display_name: self.display_name_for(id),
                });
                return Action::None;
            }
        }
        self.start_dispatch(id);
        Action::Connect(id.to_string())
    }

    /// User accepted the swap dialog. Returns `Disconnect` so Swift
    /// tears down `lastSession`; Swift then immediately calls
    /// `start_connect_after_swap(target)` to kick off the new attempt.
    pub fn confirm_swap(&mut self) -> Option<String> {
        let target = self.swap_confirmation.take()?.target_id;
        self.current_session_id = None;
        self.start_dispatch(&target);
        Some(target)
    }

    pub fn cancel_swap(&mut self) {
        self.swap_confirmation = None;
    }

    /// Mark `id` as the currently in-flight attempt. R2.rapid_switch_cancels
    /// is realised by Swift cancelling its prior `Task`; on the Rust side
    /// we just overwrite the in-flight id (the prior connect_completed_*
    /// callbacks from the cancelled Task no-op because they no longer
    /// match the new in-flight id).
    fn start_dispatch(&mut self, id: &str) {
        self.in_flight_id = Some(id.to_string());
    }

    pub fn connect_completed_session(&mut self, id: &str) {
        // Only honour the callback if it matches the most recent dispatch.
        // A late callback from a cancelled Task arrives with the old id
        // and must not bump current_session_id.
        if self.in_flight_id.as_deref() != Some(id) {
            return;
        }
        self.current_session_id = Some(id.to_string());
        self.in_flight_id = None;
    }

    pub fn connect_completed_mismatch(
        &mut self,
        id: &str,
        stored: &str,
        remote: &str,
        host: &str,
        port: u16,
    ) {
        if self.in_flight_id.as_deref() != Some(id) {
            return;
        }
        self.pending_mismatch = Some(PendingMismatch {
            stored: stored.to_string(),
            remote: remote.to_string(),
            host: host.to_string(),
            port,
            source_id: id.to_string(),
        });
        self.in_flight_id = None;
    }

    pub fn connect_completed_error(&mut self, id: &str) {
        if self.in_flight_id.as_deref() == Some(id) {
            self.in_flight_id = None;
        }
    }

    pub fn retry_after_mismatch(&mut self) -> Option<String> {
        let id = self.pending_mismatch.take()?.source_id;
        self.start_dispatch(&id);
        Some(id)
    }

    pub fn clear_mismatch(&mut self) {
        self.pending_mismatch = None;
    }

    /// Terminal screen tore down — drop the live-session bookkeeping so
    /// the next row tap starts fresh. Swift's `lastSession` ref is its
    /// own concern; we just clear our mirror.
    pub fn session_ended(&mut self) {
        self.current_session_id = None;
    }

    /// User tapped the toolbar's End-session button. Returns
    /// `Disconnect` when a live session exists (Swift then runs
    /// `lastSession?.disconnect()` and follows up with `session_ended`).
    /// Returns `None` when nothing is live.
    pub fn end_live_session(&mut self) -> Action {
        if self.current_session_id.is_none() {
            return Action::None;
        }
        self.current_session_id = None;
        Action::Disconnect
    }

    // MARK: - Delete

    pub fn request_delete(&mut self, id: &str) {
        let is_live = self.current_session_id.as_deref() == Some(id);
        let is_in_flight = self.in_flight_id.as_deref() == Some(id);
        self.delete_confirmation = Some(DeleteConfirmation {
            target_id: id.to_string(),
            display_name: self.display_name_for(id),
            is_live,
            is_in_flight,
        });
    }

    /// Commit the pending delete. Returns the target UUID so Swift can
    /// reach into `HostsStore.delete(id:)`. Drops the entry from the
    /// mirrored array + clears live/in-flight state when the targets
    /// match. Returns `None` when no delete is pending.
    pub fn confirm_delete(&mut self) -> Option<String> {
        let conf = self.delete_confirmation.take()?;
        if conf.is_in_flight && self.in_flight_id.as_deref() == Some(&conf.target_id) {
            self.in_flight_id = None;
        }
        if conf.is_live && self.current_session_id.as_deref() == Some(&conf.target_id) {
            self.current_session_id = None;
        }
        self.entries.retain(|e| e.id != conf.target_id);
        Some(conf.target_id)
    }

    pub fn cancel_delete(&mut self) {
        self.delete_confirmation = None;
    }

    // MARK: - Alert rendering
    //
    // Formatted, localized alert text for the three dialog flows. Each
    // method returns `None` when no confirmation/mismatch is pending.
    // Strings flow through `crate::l10n` so the active locale wins —
    // Swift just feeds them straight into `UIAlertController`.

    pub fn swap_alert(&self) -> Option<AlertText> {
        let s = self.swap_confirmation.as_ref()?;
        Some(AlertText {
            title: crate::l10n::format1(
                "End current session and connect to \"%@\"?",
                &s.display_name,
            ),
            message: crate::l10n::t("Your current SSH session will be disconnected."),
            confirm_label: crate::l10n::t("Connect"),
            cancel_label: crate::l10n::t("Cancel"),
        })
    }

    pub fn delete_alert(&self) -> Option<AlertText> {
        let d = self.delete_confirmation.as_ref()?;
        let title = if d.is_live {
            crate::l10n::format1("Disconnect and delete \"%@\"?", &d.display_name)
        } else {
            crate::l10n::format1("Delete \"%@\"?", &d.display_name)
        };
        let message = if d.is_live {
            crate::l10n::t(
                "You are currently connected. The session will end and the saved password or key will be removed.",
            )
        } else {
            crate::l10n::t("This will remove the saved password or key.")
        };
        Some(AlertText {
            title,
            message,
            confirm_label: crate::l10n::t("Delete"),
            cancel_label: crate::l10n::t("Cancel"),
        })
    }

    /// Toast-style "alert" for the host-key mismatch flow. The actual
    /// review is a SwiftUI sheet; this powers the toast that prompts the
    /// user to open it. `confirm_label` is the toast action ("Review"),
    /// `cancel_label` is unused but kept so the FFI shape stays uniform.
    pub fn mismatch_alert(&self) -> Option<AlertText> {
        let m = self.pending_mismatch.as_ref()?;
        Some(AlertText {
            title: crate::l10n::format1("Host key changed · %@", &m.host),
            message: crate::l10n::t("Tap to review and accept or reject."),
            confirm_label: crate::l10n::t("Review"),
            cancel_label: String::new(),
        })
    }
}

/// Formatted, localized alert text bundle returned by the swap / delete /
/// mismatch alert formatters. Pure Rust — the FFI layer translates this
/// into a `#[repr(C)]` struct of C strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertText {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
}

/// Process-wide singleton. Main-thread-only in practice (`@MainActor`
/// Swift callers); the `Mutex` is uncontended.
pub static VM: Mutex<HostsVM> = Mutex::new(HostsVM::new());

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, label: &str) -> EntryMeta {
        EntryMeta {
            id: id.to_string(),
            label: label.to_string(),
            host: "example.com".to_string(),
            port: 22,
            username: "alice".to_string(),
            auth_is_key: false,
        }
    }

    fn snapshot(items: &[EntryMeta]) -> String {
        serde_json::to_string(items).unwrap()
    }

    #[test]
    fn set_entries_clears_load_failed() {
        let mut vm = HostsVM::new();
        vm.load_failed = true;
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        assert_eq!(vm.entries.len(), 1);
        assert!(!vm.load_failed);
    }

    #[test]
    fn set_entries_with_bad_json_marks_load_failed() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot("not json");
        assert!(vm.entries.is_empty());
        assert!(vm.load_failed);
    }

    #[test]
    fn merge_injected_skips_duplicates() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        vm.merge_injected(&snapshot(&[entry("a", "dup"), entry("b", "")]));
        assert_eq!(vm.entries.len(), 2);
        assert_eq!(vm.entries[0].id, "a");
        // The first one's label is kept (no override).
        assert_eq!(vm.entries[0].label, "");
        assert_eq!(vm.entries[1].id, "b");
    }

    #[test]
    fn display_name_falls_back_to_user_at_host() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        assert_eq!(vm.display_name_for("a"), "alice@example.com");
    }

    #[test]
    fn display_name_uses_label_when_present() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "prod-box")]));
        assert_eq!(vm.display_name_for("a"), "prod-box");
    }

    #[test]
    fn request_connect_with_no_live_session_dispatches() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        assert_eq!(vm.request_connect("a"), Action::Connect("a".to_string()));
        assert_eq!(vm.in_flight_id.as_deref(), Some("a"));
        assert!(vm.swap_confirmation.is_none());
    }

    #[test]
    fn request_connect_when_in_flight_for_same_id_is_noop() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        let _ = vm.request_connect("a");
        assert_eq!(vm.request_connect("a"), Action::None);
    }

    #[test]
    fn request_connect_raises_swap_when_different_session_live() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha"), entry("b", "beta")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        assert_eq!(vm.request_connect("b"), Action::None);
        assert_eq!(
            vm.swap_confirmation.as_ref().map(|s| s.target_id.as_str()),
            Some("b")
        );
        assert_eq!(
            vm.swap_confirmation
                .as_ref()
                .map(|s| s.display_name.as_str()),
            Some("beta")
        );
    }

    #[test]
    fn confirm_swap_clears_session_and_returns_target() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", ""), entry("b", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        let _ = vm.request_connect("b");
        assert_eq!(vm.confirm_swap(), Some("b".to_string()));
        assert!(vm.swap_confirmation.is_none());
        assert!(vm.current_session_id.is_none());
        assert_eq!(vm.in_flight_id.as_deref(), Some("b"));
    }

    #[test]
    fn cancel_swap_drops_confirmation() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", ""), entry("b", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        let _ = vm.request_connect("b");
        vm.cancel_swap();
        assert!(vm.swap_confirmation.is_none());
        // Original session survives the cancel.
        assert_eq!(vm.current_session_id.as_deref(), Some("a"));
    }

    #[test]
    fn rapid_switch_late_callback_for_old_id_is_dropped() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", ""), entry("b", "")]));
        let _ = vm.request_connect("a");
        // Simulate B being requested before A's task lands. Real Swift
        // cancels A's task; we just see in_flight flip to B.
        vm.in_flight_id = Some("b".to_string());
        // Late callback from the cancelled A task. Must not promote A
        // to current_session_id.
        vm.connect_completed_session("a");
        assert_eq!(vm.current_session_id, None);
        assert_eq!(vm.in_flight_id.as_deref(), Some("b"));
    }

    #[test]
    fn connect_completed_session_clears_in_flight() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        assert_eq!(vm.current_session_id.as_deref(), Some("a"));
        assert!(vm.in_flight_id.is_none());
    }

    #[test]
    fn connect_completed_mismatch_sets_pending() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_mismatch("a", "STORED", "REMOTE", "example.com", 22);
        let m = vm.pending_mismatch.as_ref().unwrap();
        assert_eq!(m.source_id, "a");
        assert_eq!(m.stored, "STORED");
        assert_eq!(m.remote, "REMOTE");
        assert!(vm.in_flight_id.is_none());
    }

    #[test]
    fn retry_after_mismatch_redispatches() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_mismatch("a", "S", "R", "h", 22);
        assert_eq!(vm.retry_after_mismatch(), Some("a".to_string()));
        assert!(vm.pending_mismatch.is_none());
        assert_eq!(vm.in_flight_id.as_deref(), Some("a"));
    }

    #[test]
    fn clear_mismatch_drops_pending() {
        let mut vm = HostsVM::new();
        vm.pending_mismatch = Some(PendingMismatch {
            stored: "S".into(),
            remote: "R".into(),
            host: "h".into(),
            port: 22,
            source_id: "a".into(),
        });
        vm.clear_mismatch();
        assert!(vm.pending_mismatch.is_none());
    }

    #[test]
    fn end_live_session_returns_disconnect_then_session_ended_clears() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        assert_eq!(vm.end_live_session(), Action::Disconnect);
        assert!(vm.current_session_id.is_none());
        vm.session_ended();
        assert!(vm.current_session_id.is_none());
    }

    #[test]
    fn end_live_session_with_no_live_is_noop() {
        let mut vm = HostsVM::new();
        assert_eq!(vm.end_live_session(), Action::None);
    }

    #[test]
    fn request_delete_marks_live_and_in_flight_flags() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", ""), entry("b", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        // "b" is not in flight yet; request_connect("b") raises a swap-
        // confirm instead of dispatching, so to exercise the in_flight
        // branch we confirm the swap first.
        let _ = vm.request_connect("b");
        let _ = vm.confirm_swap();

        // After confirm_swap, in_flight=b and current_session is cleared.
        vm.request_delete("a");
        let d = vm.delete_confirmation.as_ref().unwrap();
        assert_eq!(d.target_id, "a");
        // current_session was cleared by confirm_swap → "a" is no longer live.
        assert!(!d.is_live);
        assert!(!d.is_in_flight);

        vm.request_delete("b");
        let d = vm.delete_confirmation.as_ref().unwrap();
        assert!(!d.is_live);
        assert!(d.is_in_flight);
    }

    #[test]
    fn confirm_delete_drops_entry_and_clears_state() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        vm.request_delete("a");
        assert_eq!(vm.confirm_delete(), Some("a".to_string()));
        assert!(vm.entries.is_empty());
        assert!(vm.current_session_id.is_none());
        assert!(vm.in_flight_id.is_none());
        assert!(vm.delete_confirmation.is_none());
    }

    #[test]
    fn confirm_delete_with_no_pending_returns_none() {
        let mut vm = HostsVM::new();
        assert!(vm.confirm_delete().is_none());
    }

    #[test]
    fn cancel_delete_drops_pending() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "")]));
        vm.request_delete("a");
        vm.cancel_delete();
        assert!(vm.delete_confirmation.is_none());
    }

    // MARK: - Alert rendering

    #[test]
    fn swap_alert_none_when_no_pending() {
        let vm = HostsVM::new();
        assert!(vm.swap_alert().is_none());
    }

    #[test]
    fn swap_alert_formats_display_name_into_title() {
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha"), entry("b", "beta-box")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        let _ = vm.request_connect("b");
        let alert = vm.swap_alert().expect("swap pending");
        assert!(alert.title.contains("beta-box"), "title={}", alert.title);
        assert!(!alert.message.is_empty());
        assert_eq!(alert.confirm_label, "Connect");
        assert_eq!(alert.cancel_label, "Cancel");
    }

    #[test]
    fn swap_alert_translates_to_zh_hans() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha"), entry("b", "beta-box")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        let _ = vm.request_connect("b");

        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let en = vm.swap_alert().unwrap();
        unsafe { crate::l10n::bt_ios_set_locale(c"zh-Hans".as_ptr()) };
        let zh = vm.swap_alert().unwrap();
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };

        // Both should contain the unlocalised display name…
        assert!(zh.title.contains("beta-box"));
        // …but the surrounding template should differ between locales
        // (xcstrings ships zh-Hans for both the title and the buttons).
        assert_ne!(en.title, zh.title, "zh title should differ from en");
        assert_ne!(en.confirm_label, zh.confirm_label);
    }

    #[test]
    fn delete_alert_uses_live_copy_when_currently_connected() {
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_session("a");
        vm.request_delete("a");
        let alert = vm.delete_alert().expect("delete pending");
        assert!(alert.title.contains("alpha"));
        assert!(
            alert.title.to_lowercase().contains("disconnect"),
            "live-delete title should mention disconnect: {}",
            alert.title
        );
        assert!(alert.message.contains("currently connected"));
        assert_eq!(alert.confirm_label, "Delete");
        assert_eq!(alert.cancel_label, "Cancel");
    }

    #[test]
    fn delete_alert_uses_plain_copy_when_offline() {
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha")]));
        vm.request_delete("a");
        let alert = vm.delete_alert().expect("delete pending");
        assert!(alert.title.contains("alpha"));
        assert!(!alert.title.to_lowercase().contains("disconnect"));
        assert_eq!(alert.message, "This will remove the saved password or key.");
    }

    #[test]
    fn delete_alert_translates_to_zh_hans() {
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha")]));
        vm.request_delete("a");

        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let en = vm.delete_alert().unwrap();
        unsafe { crate::l10n::bt_ios_set_locale(c"zh-Hans".as_ptr()) };
        let zh = vm.delete_alert().unwrap();
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };

        assert!(zh.title.contains("alpha"));
        assert_ne!(en.confirm_label, zh.confirm_label);
        assert_ne!(en.title, zh.title);
    }

    #[test]
    fn mismatch_alert_none_when_no_pending() {
        let vm = HostsVM::new();
        assert!(vm.mismatch_alert().is_none());
    }

    #[test]
    fn mismatch_alert_formats_host_into_title() {
        unsafe { crate::l10n::bt_ios_set_locale(c"en".as_ptr()) };
        let mut vm = HostsVM::new();
        vm.set_entries_from_snapshot(&snapshot(&[entry("a", "alpha")]));
        let _ = vm.request_connect("a");
        vm.connect_completed_mismatch("a", "STORED", "REMOTE", "example.com", 22);
        let alert = vm.mismatch_alert().expect("mismatch pending");
        assert!(alert.title.contains("example.com"));
        assert!(!alert.message.is_empty());
        assert_eq!(alert.confirm_label, "Review");
    }

    // MARK: - State struct Serialize / Deserialize

    #[test]
    fn entry_ref_deserializes_id_from_entries_json() {
        // JSON shape produced by bt_ios_hosts_vm_entries_json()
        let json = r#"[
            {"id":"E621E1F8-C36C-495A-93FC-0C247A3E6E5F","label":"prod","host":"example.com","port":22,"username":"alice","authIsKey":true},
            {"id":"00000000-0000-0000-0000-000000000000","label":"","host":"test.local","port":2222,"username":"root","authIsKey":false}
        ]"#;
        let refs: Vec<EntryRef> = serde_json::from_str(json).expect("deserialize EntryRef");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].id, "E621E1F8-C36C-495A-93FC-0C247A3E6E5F");
        assert_eq!(refs[1].id, "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn entry_ref_deserializes_with_only_id_field_present() {
        // Minimal JSON — decoder only needs "id", extra fields are ignored.
        let json = r#"[{"id":"abc-123"}]"#;
        let refs: Vec<EntryRef> = serde_json::from_str(json).expect("deserialize EntryRef");
        assert_eq!(refs[0].id, "abc-123");
    }

    #[test]
    fn entry_ref_rejects_missing_id() {
        let result = serde_json::from_str::<Vec<EntryRef>>(r#"[{"label":"nope"}]"#);
        assert!(result.is_err(), "should fail without id field");
    }

    #[test]
    fn pending_mismatch_round_trip() {
        let original = PendingMismatch {
            stored: "SHA256:A1B2".into(),
            remote: "SHA256:C3D4".into(),
            host: "server.example.com".into(),
            port: 22,
            source_id: "uuid-from-swift".into(),
        };
        let json = serde_json::to_string(&original).expect("serialize PendingMismatch");
        // Verify field-name mapping: source_id → "sourceID" (camelCase)
        assert!(
            json.contains(r#""sourceID":"#),
            "JSON should use sourceID key: {json}"
        );
        assert!(json.contains(r#""stored":"#));
        assert!(json.contains(r#""remote":"#));

        let parsed: PendingMismatch =
            serde_json::from_str(&json).expect("deserialize PendingMismatch");
        assert_eq!(parsed, original);
    }

    #[test]
    fn swap_confirmation_round_trip() {
        let original = SwapConfirmation {
            target_id: "E621E1F8-C36C-495A-93FC-0C247A3E6E5F".into(),
            display_name: String::new(), // skip_serializing — ignored
        };
        let json = serde_json::to_string(&original).expect("serialize SwapConfirmation");
        // display_name is skip_serializing, so it should NOT appear in JSON
        assert!(
            !json.contains("display_name"),
            "display_name should be skipped"
        );
        assert!(
            json.contains(r#""targetID":"#),
            "JSON should use targetID key: {json}"
        );

        let parsed: SwapConfirmation =
            serde_json::from_str(&json).expect("deserialize SwapConfirmation");
        assert_eq!(parsed.target_id, original.target_id);
        assert!(parsed.display_name.is_empty());
    }

    #[test]
    fn swap_confirmation_deserializes_from_swift_json() {
        // Swift sends exactly this shape
        let json = r#"{"targetID":"550e8400-e29b-41d4-a716-446655440000"}"#;
        let parsed: SwapConfirmation =
            serde_json::from_str(json).expect("deserialize SwapConfirmation");
        assert_eq!(parsed.target_id, "550e8400-e29b-41d4-a716-446655440000");
        // display_name has skip_serializing but NOT skip_deserializing, so
        // serde still deserializes from its default if missing
        assert!(parsed.display_name.is_empty());
    }

    #[test]
    fn delete_confirmation_round_trip() {
        let original = DeleteConfirmation {
            target_id: "delete-me-uuid".into(),
            display_name: String::new(), // skip_serializing
            is_live: true,               // skip_serializing
            is_in_flight: false,         // skip_serializing
        };
        let json = serde_json::to_string(&original).expect("serialize DeleteConfirmation");
        // Only targetID should appear in JSON
        assert!(json.contains(r#""targetID":"#));
        assert!(
            !json.contains("display_name"),
            "display_name should be skipped"
        );
        assert!(!json.contains("is_live"), "is_live should be skipped");
        assert!(
            !json.contains("is_in_flight"),
            "is_in_flight should be skipped"
        );

        let parsed: DeleteConfirmation =
            serde_json::from_str(&json).expect("deserialize DeleteConfirmation");
        assert_eq!(parsed.target_id, original.target_id);
        // Fields with skip_serializing that also have skip_deserializing use
        // Default::default() when round-tripping — but we set them in original
        // and they get Default::default() on deserialize. The PartialEq impl
        // will thus fail if we compare with original since is_live differs.
        assert!(parsed.display_name.is_empty());
        assert!(
            !parsed.is_live,
            "is_live should default to false after round-trip"
        );
        assert!(
            !parsed.is_in_flight,
            "is_in_flight should default to false after round-trip"
        );
    }

    #[test]
    fn delete_confirmation_deserializes_from_swift_json() {
        // Swift sends only the targetID; Rust fills internal fields as defaults
        let json = r#"{"targetID":"abc-def-ghi"}"#;
        let parsed: DeleteConfirmation =
            serde_json::from_str(json).expect("deserialize DeleteConfirmation");
        assert_eq!(parsed.target_id, "abc-def-ghi");
        assert!(parsed.display_name.is_empty());
        assert!(!parsed.is_live);
        assert!(!parsed.is_in_flight);
    }

    #[test]
    fn pending_mismatch_deserializes_from_swift_json() {
        // Exact JSON shape Swift sends across the FFI
        let json = r#"{"stored":"SHA256:abc","remote":"SHA256:def","host":"example.com","port":22,"sourceID":"some-uuid"}"#;
        let parsed: PendingMismatch =
            serde_json::from_str(json).expect("deserialize PendingMismatch");
        assert_eq!(parsed.stored, "SHA256:abc");
        assert_eq!(parsed.remote, "SHA256:def");
        assert_eq!(parsed.host, "example.com");
        assert_eq!(parsed.port, 22);
        assert_eq!(parsed.source_id, "some-uuid");
    }

    #[test]
    fn entry_meta_deserializes_from_snapshot_json() {
        // JSON shape Swift's HostsStoreInjection produces
        let json = r#"{"id":"e1","label":"My Server","host":"myserver.com","port":2222,"username":"admin","authIsKey":true}"#;
        let parsed: EntryMeta = serde_json::from_str(json).expect("deserialize EntryMeta");
        assert_eq!(parsed.id, "e1");
        assert_eq!(parsed.label, "My Server");
        assert_eq!(parsed.host, "myserver.com");
        assert_eq!(parsed.port, 2222);
        assert_eq!(parsed.username, "admin");
        assert!(parsed.auth_is_key);
    }

    #[test]
    fn entry_meta_defaults_auth_is_key_to_false() {
        let json = r#"{"id":"e1","label":"","host":"h","port":22,"username":"u"}"#;
        let parsed: EntryMeta = serde_json::from_str(json).expect("deserialize EntryMeta");
        assert!(!parsed.auth_is_key, "authIsKey should default to false");
    }

    // ── EntryRef edge cases ──────────────────────────────────────────────

    #[test]
    fn entry_ref_accepts_empty_string_id() {
        let refs: Vec<EntryRef> =
            serde_json::from_str(r#"[{"id":""}]"#).expect("deserialize EntryRef");
        assert_eq!(refs[0].id, "");
    }

    #[test]
    fn entry_ref_rejects_null_id() {
        let result = serde_json::from_value::<Vec<EntryRef>>(serde_json::json!([{"id": null}]));
        assert!(result.is_err(), "null id should be rejected");
    }

    #[test]
    fn entry_ref_ignores_extra_fields() {
        let refs: Vec<EntryRef> =
            serde_json::from_str(r#"[{"id":"abc","unexpected":"value","extra":99}]"#)
                .expect("deserialize EntryRef with extra fields");
        assert_eq!(refs[0].id, "abc");
    }

    #[test]
    fn entry_ref_rejects_object_not_wrapped_in_array() {
        let result = serde_json::from_str::<Vec<EntryRef>>(r#"{"id":"abc"}"#);
        assert!(
            result.is_err(),
            "object without array wrapper should be rejected"
        );
    }

    // ── PendingMismatch edge cases ───────────────────────────────────────

    #[test]
    fn pending_mismatch_rejects_missing_stored() {
        let result = serde_json::from_str::<PendingMismatch>(
            r#"{"remote":"R","host":"h","port":22,"sourceID":"s"}"#,
        );
        assert!(result.is_err(), "missing stored should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_missing_remote() {
        let result = serde_json::from_str::<PendingMismatch>(
            r#"{"stored":"S","host":"h","port":22,"sourceID":"s"}"#,
        );
        assert!(result.is_err(), "missing remote should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_missing_host() {
        let result = serde_json::from_str::<PendingMismatch>(
            r#"{"stored":"S","remote":"R","port":22,"sourceID":"s"}"#,
        );
        assert!(result.is_err(), "missing host should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_missing_port() {
        let result = serde_json::from_str::<PendingMismatch>(
            r#"{"stored":"S","remote":"R","host":"h","sourceID":"s"}"#,
        );
        assert!(result.is_err(), "missing port should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_missing_source_id() {
        let result = serde_json::from_str::<PendingMismatch>(
            r#"{"stored":"S","remote":"R","host":"h","port":22}"#,
        );
        assert!(result.is_err(), "missing sourceID should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_null_stored() {
        let json = serde_json::json!(
            {"stored":null,"remote":"R","host":"h","port":22,"sourceID":"s"}
        );
        let result = serde_json::from_value::<PendingMismatch>(json);
        assert!(result.is_err(), "null stored should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_null_remote() {
        let json = serde_json::json!(
            {"stored":"S","remote":null,"host":"h","port":22,"sourceID":"s"}
        );
        let result = serde_json::from_value::<PendingMismatch>(json);
        assert!(result.is_err(), "null remote should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_null_host() {
        let json = serde_json::json!(
            {"stored":"S","remote":"R","host":null,"port":22,"sourceID":"s"}
        );
        let result = serde_json::from_value::<PendingMismatch>(json);
        assert!(result.is_err(), "null host should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_null_port() {
        let json = serde_json::json!(
            {"stored":"S","remote":"R","host":"h","port":null,"sourceID":"s"}
        );
        let result = serde_json::from_value::<PendingMismatch>(json);
        assert!(result.is_err(), "null port should be rejected");
    }

    #[test]
    fn pending_mismatch_rejects_null_source_id() {
        let json = serde_json::json!(
            {"stored":"S","remote":"R","host":"h","port":22,"sourceID":null}
        );
        let result = serde_json::from_value::<PendingMismatch>(json);
        assert!(result.is_err(), "null sourceID should be rejected");
    }

    #[test]
    fn pending_mismatch_accepts_empty_strings() {
        let parsed: PendingMismatch =
            serde_json::from_str(r#"{"stored":"","remote":"","host":"","port":0,"sourceID":""}"#)
                .expect("deserialize PendingMismatch with empty strings");
        assert_eq!(parsed.stored, "");
        assert_eq!(parsed.remote, "");
        assert_eq!(parsed.host, "");
        assert_eq!(parsed.port, 0, "port 0 is valid for u16");
        assert_eq!(parsed.source_id, "");
    }

    #[test]
    fn pending_mismatch_ignores_extra_fields() {
        let parsed: PendingMismatch = serde_json::from_str(
            r#"{"stored":"S","remote":"R","host":"h","port":22,"sourceID":"s","extraField":"x","another":42}"#,
        )
        .expect("deserialize PendingMismatch with extra fields");
        assert_eq!(parsed.stored, "S");
        assert_eq!(parsed.remote, "R");
        assert_eq!(parsed.host, "h");
        assert_eq!(parsed.port, 22);
        assert_eq!(parsed.source_id, "s");
    }

    #[test]
    fn pending_mismatch_accepts_boundary_port_values() {
        // u16 range: 0..=65535
        let parsed: PendingMismatch = serde_json::from_str(
            r#"{"stored":"S","remote":"R","host":"h","port":0,"sourceID":"s"}"#,
        )
        .expect("deserialize port 0");
        assert_eq!(parsed.port, 0);

        let parsed: PendingMismatch = serde_json::from_str(
            r#"{"stored":"S","remote":"R","host":"h","port":65535,"sourceID":"s"}"#,
        )
        .expect("deserialize port 65535");
        assert_eq!(parsed.port, 65535);
    }

    // ── SwapConfirmation edge cases ──────────────────────────────────────

    #[test]
    fn swap_confirmation_rejects_missing_target_id() {
        let result = serde_json::from_str::<SwapConfirmation>(r#"{"display_name":"nope"}"#);
        assert!(result.is_err(), "missing targetID should be rejected");
    }

    #[test]
    fn swap_confirmation_rejects_null_target_id() {
        let result =
            serde_json::from_value::<SwapConfirmation>(serde_json::json!({"targetID": null}));
        assert!(result.is_err(), "null targetID should be rejected");
    }

    #[test]
    fn swap_confirmation_accepts_empty_target_id() {
        let parsed: SwapConfirmation = serde_json::from_str(r#"{"targetID":""}"#)
            .expect("deserialize SwapConfirmation with empty targetID");
        assert_eq!(parsed.target_id, "");
        assert!(parsed.display_name.is_empty());
    }

    #[test]
    fn swap_confirmation_deserializes_display_name_when_present() {
        // display_name has #[serde(default, skip_serializing)] but NOT
        // skip_deserializing, so if Swift sends it the value is honoured.
        let json = r#"{"targetID":"abc","display_name":"My Server"}"#;
        let parsed: SwapConfirmation =
            serde_json::from_str(json).expect("deserialize SwapConfirmation");
        assert_eq!(parsed.target_id, "abc");
        assert_eq!(parsed.display_name, "My Server");
    }

    #[test]
    fn swap_confirmation_ignores_extra_fields() {
        let parsed: SwapConfirmation =
            serde_json::from_str(r#"{"targetID":"abc","extraKey":99,"unknown":true}"#)
                .expect("deserialize SwapConfirmation with extra fields");
        assert_eq!(parsed.target_id, "abc");
    }

    // ── DeleteConfirmation edge cases ────────────────────────────────────

    #[test]
    fn delete_confirmation_rejects_missing_target_id() {
        let result = serde_json::from_str::<DeleteConfirmation>(r#"{"display_name":"nope"}"#);
        assert!(result.is_err(), "missing targetID should be rejected");
    }

    #[test]
    fn delete_confirmation_rejects_null_target_id() {
        let result =
            serde_json::from_value::<DeleteConfirmation>(serde_json::json!({"targetID": null}));
        assert!(result.is_err(), "null targetID should be rejected");
    }

    #[test]
    fn delete_confirmation_accepts_empty_target_id() {
        let parsed: DeleteConfirmation = serde_json::from_str(r#"{"targetID":""}"#)
            .expect("deserialize DeleteConfirmation with empty targetID");
        assert_eq!(parsed.target_id, "");
        assert!(parsed.display_name.is_empty());
        assert!(!parsed.is_live);
        assert!(!parsed.is_in_flight);
    }

    #[test]
    fn delete_confirmation_deserializes_display_name_when_present() {
        // display_name has #[serde(default, skip_serializing)] but NOT
        // skip_deserializing, so an explicit value is honoured.
        let parsed: DeleteConfirmation =
            serde_json::from_str(r#"{"targetID":"abc","display_name":"My Server"}"#)
                .expect("deserialize DeleteConfirmation");
        assert_eq!(parsed.target_id, "abc");
        assert_eq!(parsed.display_name, "My Server");
    }

    #[test]
    fn delete_confirmation_ignores_extra_fields() {
        let parsed: DeleteConfirmation =
            serde_json::from_str(r#"{"targetID":"abc","extraKey":99,"unknown":"value"}"#)
                .expect("deserialize DeleteConfirmation with extra fields");
        assert_eq!(parsed.target_id, "abc");
    }

    // ── EntryMeta edge cases ─────────────────────────────────────────────

    #[test]
    fn entry_meta_rejects_missing_host() {
        let result =
            serde_json::from_str::<EntryMeta>(r#"{"id":"e1","label":"","port":22,"username":"u"}"#);
        assert!(result.is_err(), "missing host should be rejected");
    }

    #[test]
    fn entry_meta_rejects_missing_port() {
        let result = serde_json::from_str::<EntryMeta>(
            r#"{"id":"e1","label":"","host":"h","username":"u"}"#,
        );
        assert!(result.is_err(), "missing port should be rejected");
    }

    #[test]
    fn entry_meta_rejects_missing_username() {
        let result =
            serde_json::from_str::<EntryMeta>(r#"{"id":"e1","label":"","host":"h","port":22}"#);
        assert!(result.is_err(), "missing username should be rejected");
    }

    #[test]
    fn entry_meta_rejects_null_host() {
        let json = serde_json::json!(
            {"id":"e1","label":"","host":null,"port":22,"username":"u"}
        );
        let result = serde_json::from_value::<EntryMeta>(json);
        assert!(result.is_err(), "null host should be rejected");
    }

    #[test]
    fn entry_meta_rejects_null_port() {
        let json = serde_json::json!(
            {"id":"e1","label":"","host":"h","port":null,"username":"u"}
        );
        let result = serde_json::from_value::<EntryMeta>(json);
        assert!(result.is_err(), "null port should be rejected");
    }

    #[test]
    fn entry_meta_rejects_null_username() {
        let json = serde_json::json!(
            {"id":"e1","label":"","host":"h","port":22,"username":null}
        );
        let result = serde_json::from_value::<EntryMeta>(json);
        assert!(result.is_err(), "null username should be rejected");
    }

    #[test]
    fn entry_meta_defaults_label_to_empty_when_missing() {
        let json = r#"{"id":"e1","host":"h","port":22,"username":"u"}"#;
        let parsed: EntryMeta =
            serde_json::from_str(json).expect("deserialize EntryMeta without label");
        assert_eq!(parsed.label, "", "label should default to empty string");
    }

    #[test]
    fn entry_meta_defaults_auth_is_key_when_null() {
        let json = serde_json::json!(
            {"id":"e1","label":"","host":"h","port":22,"username":"u","authIsKey":null}
        );
        let result = serde_json::from_value::<EntryMeta>(json);
        // #[serde(default)] on auth_is_key means null is treated as default → false
        assert!(result.is_err(), "null authIsKey should be rejected");
    }

    #[test]
    fn entry_meta_accepts_empty_strings_for_required_fields() {
        // id, host, and username are required but may be empty strings
        let parsed: EntryMeta =
            serde_json::from_str(r#"{"id":"","label":"","host":"","port":22,"username":""}"#)
                .expect("deserialize EntryMeta with empty strings");
        assert_eq!(parsed.id, "");
        assert_eq!(parsed.label, "");
        assert_eq!(parsed.host, "");
        assert_eq!(parsed.port, 22);
        assert_eq!(parsed.username, "");
        assert!(!parsed.auth_is_key);
    }

    #[test]
    fn entry_meta_ignores_extra_fields() {
        let parsed: EntryMeta = serde_json::from_str(
            r#"{"id":"e1","label":"","host":"h","port":22,"username":"u","unknownKey":"x","num":3}"#,
        )
        .expect("deserialize EntryMeta with extra fields");
        assert_eq!(parsed.id, "e1");
        assert_eq!(parsed.host, "h");
        assert_eq!(parsed.port, 22);
        assert_eq!(parsed.username, "u");
    }

    #[test]
    fn entry_meta_rejects_negative_port() {
        let json = serde_json::json!(
            {"id":"e1","label":"","host":"h","port":-1,"username":"u"}
        );
        let result = serde_json::from_value::<EntryMeta>(json);
        assert!(result.is_err(), "negative port should be rejected by u16");
    }

    // ── EntryMeta JSON round-trip (serialize → deserialize) ──────────

    #[test]
    fn entry_meta_round_trip_all_fields() {
        let original = EntryMeta {
            id: "uuid-123".into(),
            label: "My Server".into(),
            host: "10.0.0.5".into(),
            port: 2222,
            username: "admin".into(),
            auth_is_key: false,
        };
        let json = serde_json::to_string(&original).expect("serialize EntryMeta");
        let deserialized: EntryMeta = serde_json::from_str(&json).expect("deserialize EntryMeta");
        assert_eq!(original, deserialized);
    }

    #[test]
    fn entry_meta_round_trip_minimal() {
        let original = EntryMeta {
            id: "uuid-456".into(),
            label: String::new(),
            host: "example.com".into(),
            port: 22,
            username: "user".into(),
            auth_is_key: false,
        };
        let json = serde_json::to_string(&original).expect("serialize EntryMeta");
        let deserialized: EntryMeta = serde_json::from_str(&json).expect("deserialize EntryMeta");
        assert_eq!(original, deserialized);
    }

    #[test]
    fn entry_meta_round_trip_auth_is_key_json_key() {
        let original = EntryMeta {
            id: "uuid-789".into(),
            label: "Server".into(),
            host: "host.local".into(),
            port: 22,
            username: "root".into(),
            auth_is_key: false,
        };
        let json = serde_json::to_string(&original).expect("serialize EntryMeta");
        assert!(
            json.contains(r#""authIsKey":true"#),
            "JSON should use camelCase authIsKey: {json}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["authIsKey"], true);
        // Verify the field is not snake_case
        assert!(
            !json.contains("auth_is_key"),
            "JSON must not use snake_case: {json}"
        );
    }

    #[test]
    fn entry_meta_round_trip_ipv6_host() {
        let original = EntryMeta {
            id: "uuid-ipv6".into(),
            label: String::new(),
            host: "::1".into(),
            port: 22,
            username: "alice".into(),
            auth_is_key: false,
        };
        let json = serde_json::to_string(&original).expect("serialize EntryMeta");
        let deserialized: EntryMeta = serde_json::from_str(&json).expect("deserialize EntryMeta");
        assert_eq!(original, deserialized);
    }

    #[test]
    fn entry_meta_round_trip_boundary_port_values() {
        let min_port = EntryMeta {
            id: "min".into(),
            label: String::new(),
            host: "h".into(),
            port: 0,
            username: "u".into(),
            auth_is_key: false,
        };
        let json = serde_json::to_string(&min_port).expect("serialize port 0");
        let parsed: EntryMeta = serde_json::from_str(&json).expect("deserialize port 0");
        assert_eq!(parsed.port, 0);

        let max_port = EntryMeta {
            id: "max".into(),
            label: String::new(),
            host: "h".into(),
            port: 65535,
            username: "u".into(),
            auth_is_key: false,
        };
        let json = serde_json::to_string(&max_port).expect("serialize port 65535");
        let parsed: EntryMeta = serde_json::from_str(&json).expect("deserialize port 65535");
        assert_eq!(parsed.port, 65535);
    }

    #[test]
    fn entry_meta_json_ignores_extra_fields_on_deserialize() {
        // JSON with extra fields should still deserialize correctly
        let json = r#"{"id":"e1","label":"L","host":"h","port":22,"username":"u","authIsKey":false,"extra":"value","count":99}"#;
        let parsed: EntryMeta = serde_json::from_str(json).expect("deserialize with extra fields");
        assert_eq!(parsed.id, "e1");
        assert_eq!(parsed.label, "L");
        assert_eq!(parsed.host, "h");
        assert_eq!(parsed.port, 22);
        assert_eq!(parsed.username, "u");
        assert!(!parsed.auth_is_key);
    }
}
