//! the log, written to smash-3ds.log in the data folder, which the Device
//! Portal's file explorer can fetch from a PC.

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

struct FileLog {
    file: Mutex<std::fs::File>,
}

impl log::Log for FileLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            if let Ok(mut file) = self.file.lock() {
                let _ = writeln!(file, "[{}] {}: {}", record.level(), record.target(), record.args());
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// starts logging to the folder. a second call does nothing.
pub fn init(folder: &Path) {
    let Ok(file) = std::fs::File::create(folder.join("smash-3ds.log")) else { return };
    let logger = Box::leak(Box::new(FileLog { file: Mutex::new(file) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    // a panic is written down before the app goes
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        log::logger().flush();
        previous(info);
    }));
}
