mod app;
pub mod core;
pub mod model;
pub mod net;
pub mod parser;
pub mod ui;

use app::App;

#[tokio::main]
async fn main() -> glib::ExitCode {
    env_logger::init();
    libadwaita::init().expect("Failed to initialize Libadwaita");

    let app = App::new();
    app.run()
}
