//! Pure state machine for the R10 onboarding flow.
//!
//! Mirrors what `OnboardingViewModel.swift` used to encode. Lives outside
//! the iOS gate so the unit tests run on a macOS host (no simulator needed).
//!
//! Step-int conventions match the existing FFI:
//! - `HostKind`: `0 = macOS`, `1 = other`
//! - `Location`: `0 = sameWifi`, `1 = remote`
//!
//! The Swift coordinator no longer drives transitions — the Rust
//! coordinator (`coordinator.rs`) consults [`OnboardingState::next_step_after_location`]
//! when a step's callback fires.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostKind {
    MacOS,
    Other,
}

impl HostKind {
    pub fn from_choice(choice: i32) -> Option<Self> {
        match choice {
            0 => Some(HostKind::MacOS),
            1 => Some(HostKind::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Location {
    SameWifi,
    Remote,
}

impl Location {
    pub fn from_choice(choice: i32) -> Option<Self> {
        match choice {
            0 => Some(Location::SameWifi),
            1 => Some(Location::Remote),
            _ => None,
        }
    }
}

/// Navigation steps after the host-kind picker.
///
/// `Location` is the very next step after host-kind selection; the other
/// two are conditional terminators. `Location` is retained in the enum
/// for symmetry with the original Swift `Step` enum even though the
/// transition helpers never return it themselves (the coordinator
/// constructs the location VC directly from `host_kind_choice_cb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    #[allow(dead_code)]
    Location,
    MacTutorial,
    LocalPermission,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OnboardingState {
    pub host_kind: Option<HostKind>,
    pub location: Option<Location>,
}

impl OnboardingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn select_host_kind(&mut self, kind: HostKind) {
        self.host_kind = Some(kind);
    }

    pub fn select_location(&mut self, loc: Location) {
        self.location = Some(loc);
    }

    /// The step that should come after the location-picker step.
    /// Mirrors `OnboardingViewModel.nextStepAfterLocation`:
    /// - macOS host → MacTutorial (regardless of location).
    /// - Other host + sameWifi → LocalPermission.
    /// - Other host + remote → LocalPermission (terminator with "all set"
    ///   copy; the VC skips the prompt itself when `is_remote == true`).
    pub fn next_step_after_location(host: HostKind, _loc: Location) -> Step {
        match host {
            HostKind::MacOS => Step::MacTutorial,
            HostKind::Other => Step::LocalPermission,
        }
    }

    /// The step that should come after MacTutorial. Same Wi-Fi shows the
    /// permission prompt; remote is the terminator.
    pub fn next_step_after_mac_tutorial(loc: Location) -> Option<Step> {
        match loc {
            Location::SameWifi => Some(Step::LocalPermission),
            Location::Remote => None, // finishes onboarding
        }
    }

    /// Whether the local-permission terminator VC should show the
    /// "all set" copy (`true`) or the "request permission" copy (`false`).
    pub fn is_remote_terminator(loc: Location) -> bool {
        matches!(loc, Location::Remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_kind_from_choice_round_trip() {
        assert_eq!(HostKind::from_choice(0), Some(HostKind::MacOS));
        assert_eq!(HostKind::from_choice(1), Some(HostKind::Other));
        assert_eq!(HostKind::from_choice(2), None);
        assert_eq!(HostKind::from_choice(-1), None);
    }

    #[test]
    fn location_from_choice_round_trip() {
        assert_eq!(Location::from_choice(0), Some(Location::SameWifi));
        assert_eq!(Location::from_choice(1), Some(Location::Remote));
        assert_eq!(Location::from_choice(7), None);
    }

    #[test]
    fn select_host_kind_records_selection() {
        let mut s = OnboardingState::new();
        assert!(s.host_kind.is_none());
        s.select_host_kind(HostKind::MacOS);
        assert_eq!(s.host_kind, Some(HostKind::MacOS));
        s.select_host_kind(HostKind::Other);
        assert_eq!(s.host_kind, Some(HostKind::Other));
    }

    #[test]
    fn select_location_records_selection() {
        let mut s = OnboardingState::new();
        assert!(s.location.is_none());
        s.select_location(Location::SameWifi);
        assert_eq!(s.location, Some(Location::SameWifi));
        s.select_location(Location::Remote);
        assert_eq!(s.location, Some(Location::Remote));
    }

    #[test]
    fn next_step_after_location_macos_same_wifi_is_tutorial() {
        assert_eq!(
            OnboardingState::next_step_after_location(HostKind::MacOS, Location::SameWifi),
            Step::MacTutorial
        );
    }

    #[test]
    fn next_step_after_location_macos_remote_is_tutorial() {
        assert_eq!(
            OnboardingState::next_step_after_location(HostKind::MacOS, Location::Remote),
            Step::MacTutorial
        );
    }

    #[test]
    fn next_step_after_location_other_same_wifi_is_permission() {
        assert_eq!(
            OnboardingState::next_step_after_location(HostKind::Other, Location::SameWifi),
            Step::LocalPermission
        );
    }

    #[test]
    fn next_step_after_location_other_remote_is_permission_terminator() {
        assert_eq!(
            OnboardingState::next_step_after_location(HostKind::Other, Location::Remote),
            Step::LocalPermission
        );
    }

    #[test]
    fn next_step_after_mac_tutorial_routes_on_location() {
        assert_eq!(
            OnboardingState::next_step_after_mac_tutorial(Location::SameWifi),
            Some(Step::LocalPermission)
        );
        assert_eq!(
            OnboardingState::next_step_after_mac_tutorial(Location::Remote),
            None
        );
    }

    #[test]
    fn is_remote_terminator_matches_location() {
        assert!(!OnboardingState::is_remote_terminator(Location::SameWifi));
        assert!(OnboardingState::is_remote_terminator(Location::Remote));
    }
}
