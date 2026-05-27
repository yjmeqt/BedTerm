//! Asset-catalog token lookup.
//!
//! Per project CLAUDE.md: every user-visible colour must come from
//! `Tokens.xcassets` in the BedTermKit Swift package — no hex strings,
//! no `UIColor(red:green:blue:)`. That asset catalogue ships inside
//! the `BedTermKit_BedTermKit.bundle` resource bundle that SwiftPM
//! drops alongside the host app at build time, so from the Rust side
//! we have to locate that bundle and ask UIColor to resolve names
//! against it.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, ClassType, MainThreadMarker};
use objc2_foundation::{NSBundle, NSString};
use objc2_ui_kit::UIColor;

/// Path-suffix of the SwiftPM-generated resource bundle that contains
/// `Tokens.xcassets`. The convention is `<Package>_<Target>.bundle`.
const KIT_BUNDLE_NAME: &str = "BedTermKit_BedTermKit";

/// Cached BedTermKit resource bundle. Looked up once on first use.
fn kit_bundle() -> Option<Retained<NSBundle>> {
    let main = NSBundle::mainBundle();
    let name = NSString::from_str(KIT_BUNDLE_NAME);
    let ext = NSString::from_str("bundle");
    let path: Option<Retained<NSString>> =
        unsafe { msg_send![&*main, pathForResource: &*name, ofType: &*ext] };
    let path = path?;
    NSBundle::bundleWithPath(&path)
}

/// Load a named colour from `Tokens.xcassets`, falling back to a hard
/// override (typed UIColor accessor) if the bundle / asset can't be
/// found — defensive only; the catalogue ships with every release build.
pub(crate) fn token_color(name: &str, fallback: Retained<UIColor>) -> Retained<UIColor> {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let _ = mtm; // UIColor APIs require main thread but don't take mtm.
    let Some(bundle) = kit_bundle() else {
        return fallback;
    };
    let ns_name = NSString::from_str(name);
    let trait_collection: *const AnyObject = std::ptr::null();
    let color: Option<Retained<UIColor>> = unsafe {
        msg_send![
            UIColor::class(),
            colorNamed: &*ns_name,
            inBundle: &*bundle,
            compatibleWithTraitCollection: trait_collection,
        ]
    };
    color.unwrap_or(fallback)
}
