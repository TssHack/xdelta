pub mod config;
pub mod process;

pub use config::{generate_xray_config, write_config_to_temp_file};
pub use process::CoreProcessManager;
