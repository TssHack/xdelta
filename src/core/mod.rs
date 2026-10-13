pub mod config;
pub mod persist;
pub mod process;

pub use config::{generate_xray_config, write_config_to_temp_file};
pub use persist::AppearanceSettings;
pub use process::CoreProcessManager;
