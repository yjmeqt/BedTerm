//! Hardware-key handling for `BtIosMetalInputView`. Implements
//! `pressesBegan:withEvent:` and the byte-encoding tables that translate
//! UIKey scancodes / Ctrl-modifier combos into xterm/VT byte sequences.

use super::BtIosMetalInputView;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::MainThreadMarker;
use objc2_ui_kit::{UIKey, UIKeyModifierFlags, UIPress, UIPressesEvent};

impl BtIosMetalInputView {
    pub(super) fn do_presses_began(&self, presses: &NSObject, event: Option<&UIPressesEvent>) {
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        // `presses` is an NSSet<UIPress *>. Iterate via NSFastEnumeration
        // through msg_send! to avoid pulling in NSSet bindings just for
        // this.
        let mut handled = false;
        let enumerator: Retained<NSObject> = unsafe { msg_send![presses, objectEnumerator] };
        loop {
            let next: *const UIPress = unsafe { msg_send![&*enumerator, nextObject] };
            if next.is_null() {
                break;
            }
            let press: &UIPress = unsafe { &*next };
            let Some(key) = press.key(mtm) else { continue };
            if let Some(bytes) = encode_key(&key) {
                self.dispatch_send(&bytes);
                handled = true;
            }
        }
        if !handled {
            unsafe {
                let _: () = msg_send![super(self), pressesBegan: presses, withEvent: event];
            }
        }
    }
}

/// xterm/VT byte sequence for `key`. Mirrors
/// `TerminalMetalUIView+HardwareKeys.swift`. Returns `None` for keys we
/// don't translate (printable insertText keys, Cmd-combos we want the
/// system to consume, etc.).
pub(super) fn encode_key(key: &UIKey) -> Option<Vec<u8>> {
    use objc2_ui_kit::UIKeyboardHIDUsage as K;
    if let Some(ctrl) = encode_control(key) {
        return Some(vec![ctrl]);
    }
    let esc: u8 = 0x1B;
    let kc = key.keyCode();
    let bytes: &[u8] = match kc {
        v if v == K::KeyboardUpArrow => &[esc, b'[', b'A'],
        v if v == K::KeyboardDownArrow => &[esc, b'[', b'B'],
        v if v == K::KeyboardRightArrow => &[esc, b'[', b'C'],
        v if v == K::KeyboardLeftArrow => &[esc, b'[', b'D'],
        v if v == K::KeyboardHome => &[esc, b'[', b'H'],
        v if v == K::KeyboardEnd => &[esc, b'[', b'F'],
        v if v == K::KeyboardPageUp => &[esc, b'[', b'5', b'~'],
        v if v == K::KeyboardPageDown => &[esc, b'[', b'6', b'~'],
        v if v == K::KeyboardEscape => &[esc],
        v if v == K::KeyboardTab => &[0x09],
        v if v == K::KeyboardReturnOrEnter => &[0x0D],
        v if v == K::KeyboardDeleteOrBackspace => &[0x7F],
        _ => return None,
    };
    Some(bytes.to_vec())
}

pub(super) fn encode_control(key: &UIKey) -> Option<u8> {
    let mods = key.modifierFlags();
    if !mods.contains(UIKeyModifierFlags::Control) {
        return None;
    }
    let chars = key.characters().to_string();
    if chars.chars().count() != 1 {
        return None;
    }
    let up = chars.to_uppercase();
    let c = up.chars().next()?;
    let ascii = c as u32;
    if (0x40..=0x5F).contains(&ascii) {
        Some((ascii - 0x40) as u8)
    } else {
        None
    }
}
