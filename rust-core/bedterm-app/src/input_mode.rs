//! 3-state input mode enum (pure). The`ModeState` struct with UIKit
//! integration lives in `bedterm-ios`.

/// Partition the terminal's bottom chrome into three exclusive states.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    State1 = 1,
    State2 = 2,
    State3 = 3,
}
