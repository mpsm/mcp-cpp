use std::env;
use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Configuration for the logging system
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log level filter (e.g., "debug", "info", "warn", "error")
    pub level: String,
    /// Optional log file path. If None, logs only to stderr
    pub file_path: Option<PathBuf>,
    /// Whether to use structured JSON format for logs
    pub json_format: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file_path: None,
            json_format: false,
        }
    }
}

impl LogConfig {
    /// Create LogConfig from environment variables
    pub fn from_env() -> Self {
        let level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        let file_path = env::var("MCP_LOG_FILE").ok().map(|path| {
            let mut path_buf = PathBuf::from(path);

            // Add process ID if MCP_LOG_UNIQUE is set
            if env::var("MCP_LOG_UNIQUE").unwrap_or_default() == "true" {
                if let Some(filename) = path_buf.file_stem() {
                    let extension = path_buf
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("");

                    let pid = std::process::id();
                    let unique_filename = if extension.is_empty() {
                        format!("{}.{}", filename.to_string_lossy(), pid)
                    } else {
                        format!("{}.{}.{}", filename.to_string_lossy(), pid, extension)
                    };

                    path_buf.set_file_name(unique_filename);
                }
            }

            path_buf
        });

        let json_format = env::var("MCP_LOG_JSON").unwrap_or_default() == "true";

        Self {
            level,
            file_path,
            json_format,
        }
    }

    /// Override values from CLI arguments
    pub fn with_overrides(mut self, level: Option<String>, file_path: Option<PathBuf>) -> Self {
        if let Some(level) = level {
            self.level = level;
        }
        if let Some(file_path) = file_path {
            self.file_path = Some(file_path);
        }
        self
    }
}

/// Initialize the logging system based on configuration
pub fn init_logging(config: LogConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Create environment filter from log level
    let env_filter = EnvFilter::try_new(&config.level).or_else(|_| EnvFilter::try_new("info"))?;

    // Build the subscriber based on configuration
    let subscriber = tracing_subscriber::registry().with(env_filter);

    match (&config.file_path, config.json_format) {
        // File + JSON format
        (Some(file_path), true) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?;

            let file_layer = fmt::layer().json().with_writer(file).with_ansi(false);

            subscriber.with(file_layer).init();
        }
        // File + human readable format
        (Some(file_path), false) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?;

            let file_layer = fmt::layer()
                .with_writer(file)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true);

            subscriber.with(file_layer).init();
        }
        // Stderr only + JSON format
        (None, true) => {
            let stderr_layer = fmt::layer().json().with_writer(io::stderr).with_ansi(false);

            subscriber.with(stderr_layer).init();
        }
        // Stderr only + human readable format (default)
        (None, false) => {
            let stderr_layer = fmt::layer()
                .with_writer(io::stderr)
                .with_ansi(true)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true);

            subscriber.with(stderr_layer).init();
        }
    }

    Ok(())
}

/// Helper function to log structured MCP requests/responses in one line
#[macro_export]
macro_rules! log_mcp_message {
    ($level:expr, $direction:expr, $method:expr, $data:expr) => {
        tracing::event!(
            $level,
            direction = $direction,
            method = $method,
            data = ?$data,
            pid = std::process::id(),
            "MCP message"
        );
    };
}

/// Helper function to log structured LSP requests/responses in one line
#[macro_export]
macro_rules! log_lsp_message {
    ($level:expr, $direction:expr, $method:expr, $data:expr) => {
        tracing::event!(
            $level,
            direction = $direction,
            method = $method,
            data = ?$data,
            pid = std::process::id(),
            "LSP message"
        );
    };
}

/// Helper function to log performance timing
#[macro_export]
macro_rules! log_timing {
    ($level:expr, $operation:expr, $duration:expr) => {
        tracing::event!(
            $level,
            operation = $operation,
            duration_ms = $duration.as_millis(),
            pid = std::process::id(),
            "Performance timing"
        );
    };
}
