mod app;
pub mod core;
pub mod model;
pub mod net;
pub mod parser;
pub mod ui;
pub mod updater;

use app::App;
use std::io::Write;

/// Starts logging to both the terminal and a file.
///
/// The file half matters for a desktop application: launched from a menu or
/// icon there is no terminal, so diagnostics would otherwise be invisible and a
/// failure looks like nothing happening at all. The file lives next to the
/// settings, at `~/.config/xdelta/xdelta.log`.
///
/// A failure to open the file is not fatal: logging continues on stderr.
fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));

    builder.format_timestamp_millis();

    if let Some(path) = core::persist::log_path() {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(file) => {
                // One sink writing to both targets keeps the two views in step;
                // two separate sinks would interleave badly.
                builder.target(env_logger::Target::Pipe(Box::new(DualWriter { file })));
            }
            Err(e) => {
                eprintln!("xdelta: could not open log file {}: {}", path.display(), e);
            }
        }
    }

    builder.init();

    if let Some(path) = core::persist::log_path() {
        log::info!(
            "xdelta {} starting; log file {}",
            env!("CARGO_PKG_VERSION"),
            path.display()
        );
    }
}

/// Writes each log record to a file and to stderr.
struct DualWriter {
    file: std::fs::File,
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // The terminal write is best-effort: a closed or redirected stderr must
        // not stop the file from being written.
        let _ = std::io::stderr().write_all(buf);
        self.file.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        self.file.flush()
    }
}

#[tokio::main]
async fn main() -> glib::ExitCode {
    init_logging();

    libadwaita::init().expect("Failed to initialize Libadwaita");

    let app = App::new();
    app.run()
}
