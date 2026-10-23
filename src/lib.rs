//! Linux hardware discovery and validated control operations, independent of any UI.
pub mod authorization;
pub mod battery;
pub mod control;
pub mod controllers;
pub mod desktop;
pub mod devices;
pub mod firmware;
pub mod firmware_update;
pub mod inventory;
pub mod power;
pub mod probe;
pub mod process;
pub mod settings;

pub use control::{execute, plan, Change, ControlRequest, Plan};
pub use probe::{scan, Snapshot};
