//! N8 Tray's OS layer: everything that differs between Windows, macOS and
//! Linux sits behind the plain functions and types here, so `n8tray-core`
//! stays free of OS calls and is tested with data.
//!
//! - [`process`]: one snapshot of every process per pass (pid, parent,
//!   creation time, CPU time, memory), the executable path and command line.
//! - [`memory`]: free and total physical memory.
//! - [`container`]: the Job Object every app the host starts runs in
//!   (Windows); elsewhere containment is not available yet.
//! - [`spawn`]: starting a program inside its container, with no window.
//! - [`signal`]: a polite close request and a force stop.
//! - [`gpu`]: per-process GPU memory and engine use (Windows counters).
//! - [`autostart`]: start at sign-in (the Run key, an XDG autostart entry or
//!   a LaunchAgent).
//! - [`legacy`]: whether the PowerShell N8 Tray runs.
//! - [`folders`]: a folder only the current user can read.

pub mod autostart;
pub mod container;
pub mod folders;
pub mod gpu;
pub mod legacy;
pub mod memory;
pub mod process;
pub mod signal;
pub mod spawn;

#[cfg(windows)]
mod win;
