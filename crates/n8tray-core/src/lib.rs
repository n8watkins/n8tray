//! N8 Tray's host logic, with no window and no Tauri: a plain library that
//! the tray app drives and that is tested with data and with the conformance
//! kit's fake module.
//!
//! The [`host::Host`] owns every app the modules folder describes. It reads
//! manifests ([`discovery`]), keeps the user's intent ([`settings`]), starts
//! and stops apps only through what their manifest declares
//! ([`supervisor`]), reads their status ([`status_io`]), verifies which
//! processes they own ([`verify`]), writes the host lease ([`lease`]) and
//! publishes one [`model::HostSnapshot`] for the tray and the window to show.

pub mod alerts;
pub mod demand;
pub mod discovery;
pub mod folders;
pub mod history;
pub mod host;
pub mod lease;
pub mod logging;
pub mod model;
pub mod pressure;
pub mod resolve;
pub mod settings;
pub mod status_io;
pub mod supervisor;
pub mod tooltip;
pub mod update;
pub mod util;
pub mod verify;

/// This host's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
