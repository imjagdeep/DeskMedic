//! DeskMedic engine. Everything here runs without Tauri so it can be unit
//! tested; the app crate only wires these functions to commands.

pub mod drives;
pub mod log;
pub mod paths;
pub mod settings;
pub mod sys;

pub use settings::Settings;
