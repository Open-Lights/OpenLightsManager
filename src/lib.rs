#![warn(clippy::all, rust_2018_idioms)]

pub mod gui;
pub mod io;
pub mod notifications;
pub mod settings;
pub use gui::OpenLightsManager;