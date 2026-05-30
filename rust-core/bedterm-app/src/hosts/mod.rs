//! Saved-hosts model types (pure, no UIKit).
//!
//! Defines [`SavedHost`], [`HostListEntry`] and JSON marshalling helpers
//! used by the hosts VM and the persistence layer.

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub mod model;
