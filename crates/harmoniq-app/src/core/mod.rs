//! Core subsystems that back the Harmoniq Studio user interface.
//!
//! The plugin manager UI is split into a lightweight scanning service and a
//! persistence layer so that the egui front-end never performs blocking
//! operations. These modules are kept under `core` so that other parts of the
//! application can reuse the logic without depending on UI concerns.

pub mod plugin_registry;
pub mod plugin_scanner;
