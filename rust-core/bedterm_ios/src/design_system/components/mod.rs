//! Reusable UIKit component factories for the Rust UI layer.
//!
//! Components compose tokens (colours, typography, spacing) and return
//! `Retained<UI...>` instances ready to be added to a view hierarchy.
//! Designed so the upcoming Settings + Onboarding VCs can compose them
//! without re-deriving padding / typography / colour choices.

#![cfg(target_os = "ios")]
// Components are introduced for the upcoming Settings (W23b) +
// Onboarding (W23c) Rust VCs; no caller exists yet in the terminal VC,
// so dead-code warnings are allowed at module scope until then.
#![allow(dead_code)]

mod footer_label;
mod form_row;
mod form_section;
mod header_label;
mod list_row;
mod primary_button;
mod secure_text_field;
mod segmented_control;
mod text_field;
mod toggle_row;

// Re-exports are wired up so that the upcoming Settings (W23b) +
// Onboarding (W23c) Rust VCs can compose these components directly. They
// are not consumed yet by the existing terminal-host VC, so silence the
// `unused_imports` warning until those landing waves wire them in.
#[allow(unused_imports)]
pub use footer_label::footer_label;
#[allow(unused_imports)]
pub use form_row::form_row;
#[allow(unused_imports)]
pub use form_section::{form_card, form_card_external_header, form_section};
#[allow(unused_imports)]
pub use header_label::{card_description_label, card_title_label, field_label, header_label};
#[allow(unused_imports)]
pub use list_row::{make_list_row, make_list_row_with_accessory, ListRowAccessory, ListRowHandle};
#[allow(unused_imports)]
pub use primary_button::primary_button;
#[allow(unused_imports)]
pub use secure_text_field::make_secure_text_field;
#[allow(unused_imports)]
pub use segmented_control::make_segmented_control;
#[allow(unused_imports)]
pub use text_field::{make_text_field, TextFieldConfig, TextFieldHandle};
#[allow(unused_imports)]
pub use toggle_row::toggle_row;
