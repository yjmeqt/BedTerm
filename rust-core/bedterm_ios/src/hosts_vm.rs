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
//! C exports live in [`crate::ffi::hosts`] (iOS-gated).

use serde::{Deserialize, Serialize};
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
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteConfirmation {
    #[serde(rename = "targetID")]
    pub target_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "isLive")]
    pub is_live: bool,
    #[serde(rename = "isInFlight")]
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

#[derive(Debug, Default, Clone)]
pub struct HostsVM {
    pub entries: Vec<EntryMeta>,
    pub in_flight_id: Option<String>,
    pub current_session_id: Option<String>,
    pub pending_mismatch: Option<PendingMismatch>,
    pub swap_confirmation: Option<SwapConfirmation>,
    pub delete_confirmation: Option<DeleteConfirmation>,
    pub load_failed: bool,
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
}
