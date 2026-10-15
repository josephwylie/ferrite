//! herdr-focus-notify, vendored as Ferrite's system-notification delivery.
//!
//! See README.md for the upstream commit and what was changed. The modules
//! are upstream's; `focus` is the Ferrite adapter that replaced its Herdr
//! adapter, and the crate's public face.
//!
//! Upstream runs on macOS only (alerter, `lsappinfo`, `open -b`); elsewhere
//! only the decision of whether to notify is built.
//!
//! Vendored with minimal changes so upstream can be merged: the modules
//! marked `allow(dead_code)` keep helpers only the Herdr paths used (event
//! JSON, foreground delivery, terminal learning, focus-origin markers).

#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod event;
#[cfg(target_os = "macos")]
mod executable;
mod focus;
#[cfg(target_os = "macos")]
mod git;
#[cfg(target_os = "macos")]
mod icons;
#[cfg(target_os = "macos")]
mod notification;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod notifier;
#[cfg(target_os = "macos")]
mod script;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod state;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod util;

pub use focus::*;
