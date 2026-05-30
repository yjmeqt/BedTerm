//! BedTerm Rust UIKit design system.
//!
//! Pure tokens and metrics are re-exported from `bedterm_app`.
//! iOS-specific component factories and typography live here.

#![cfg_attr(not(target_os = "ios"), allow(unused_imports))]

pub mod colors;

// Re-export pure metrics from bedterm-app
pub use bedterm_app::design_system::form_section_metrics;
pub use bedterm_app::design_system::list_row_metrics;
pub use bedterm_app::design_system::segmented_control_metrics;
pub use bedterm_app::design_system::spacing;
pub use bedterm_app::design_system::text_field_metrics;

#[cfg(target_os = "ios")]
pub mod components;
#[cfg(target_os = "ios")]
pub mod typography;
