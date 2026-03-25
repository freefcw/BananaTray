use env_logger::fmt::Target;
use env_logger::Env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct LoggingInit {
    pub target_description: String,
}

pub fn init() -> LoggingInit {
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    builder.format(|buf, record| {
        writeln!(
            buf,
            "{} {:<5} {:<12} {}",
            buf.timestamp_millis(),
            record.level(),
            record.target(),
            record.args()
        )
    });

    let target_description = if should_log_to_file() {
        match open_log_file() {
            Ok(file) => {
                builder.target(Target::Pipe(Box::new(file)));
                "./banana.log".to_string()
            }
            Err(err) => {
                eprintln!("failed to open banana.log, falling back to stderr: {err}");
                builder.target(Target::Stderr);
                "stderr".to_string()
            }
        }
    } else {
        builder.target(Target::Stderr);
        "stderr".to_string()
    };

    builder.init();

    LoggingInit { target_description }
}

fn should_log_to_file() -> bool {
    std::env::var("BANANATRAY_LOG_FILE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn open_log_file() -> std::io::Result<SharedFile> {
    let path = PathBuf::from("banana.log");
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(SharedFile {
        file: Arc::new(Mutex::new(file)),
    })
}

struct SharedFile {
    file: Arc<Mutex<std::fs::File>>,
}

impl Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("failed to lock banana.log"))?;
        file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| std::io::Error::other("failed to lock banana.log"))?;
        file.flush()
    }
}
