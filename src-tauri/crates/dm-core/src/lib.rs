//! DeskMedic engine. Everything here runs without Tauri so it can be unit
//! tested; the app crate only wires these functions to commands.

pub mod cleanup;
pub mod disk;
pub mod drives;
pub mod fixes;
pub mod log;
pub mod paths;
pub mod procs;
pub mod profiles;
pub mod ps;
pub mod run;
pub mod scan;
pub mod settings;
pub mod sys;

pub use settings::Settings;
