mod app;
pub mod model;
pub mod ui;

use app::App;

fn main() -> glib::ExitCode {
    env_logger::init();
    libadwaita::init().expect("Failed to initialize Libadwaita");

    let app = App::new();
    app.run()
}
