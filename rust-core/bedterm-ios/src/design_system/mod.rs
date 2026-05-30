//! BedTerm Rust UIKit design system.
//!
//! Pure tokens and metrics are re-exported from `bedterm_app`.
//! iOS-specific component factories and typography live here.

pub mod colors;

// Re-export pure metrics from bedterm-app
pub use bedterm_app::design_system::form_section_metrics;
pub use bedterm_app::design_system::list_row_metrics;
pub use bedterm_app::design_system::segmented_control_metrics;
pub use bedterm_app::design_system::spacing;
pub use bedterm_app::design_system::text_field_metrics;

pub mod components;
pub mod typography;
