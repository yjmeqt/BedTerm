//! BedTerm Rust UIKit design system.
//!
//! Centralises colour tokens (replacing the `Tokens.xcassets` Shadcn*
//! asset-catalog round-trip), typography helpers, spacing constants, and
//! reusable component factories. The colour layer is iOS-only at the
//! `Retained<UIColor>` accessor surface, but the `Rgba` / `TOKEN_TABLE`
//! data lives outside the cfg gate so the parity unit tests run on the
//! macOS host.
//!
//! See `colors::TOKEN_TABLE` for the source-of-truth light/dark RGBA
//! pairs ported from `BedTermKit/Sources/BedTermKit/Resources/Tokens.xcassets`.

pub mod colors;
pub mod form_section_metrics;
pub mod list_row_metrics;
pub mod segmented_control_metrics;
pub mod spacing;
pub mod text_field_metrics;

#[cfg(target_os = "ios")]
pub mod components;
#[cfg(target_os = "ios")]
pub mod typography;
