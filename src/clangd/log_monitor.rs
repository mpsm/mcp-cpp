//! Clangd stderr log monitor for progress tracking
//!
//! Monitors clangd's stderr output for indexing progress messages and emits
//! structured progress events. This complements the LSP progress notifications
//! with more detailed file-level progress information.

use crate::clangd::index::{ProgressEvent, ProgressHandler};
use regex::Regex;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, trace};

/// Log parser trait for testing and extensibility
pub trait LogParser: Send + Sync {
    /// Parse a log line and return a progress event if applicable
    fn parse_line(&self, line: &str) -> Option<ProgressEvent>;
}

/// Default clangd log parser using regex patterns
pub struct ClangdLogParser {
    indexing_start_regex: Regex,
    indexing_complete_regex: Regex,
    stdlib_start_regex: Regex,
    stdlib_complete_regex: Regex,
}

impl ClangdLogParser {
    /// Create a new clangd log parser with compiled regex patterns
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            // V[14:23:45.123] Indexing /path/to/file.cpp (digest:=0x1234ABCD)
            indexing_start_regex: Regex::new(
                r"V\[\d{2}:\d{2}:\d{2}\.\d{3}\] Indexing (.+?) \(digest:=(.+?)\)",
            )?,

            // I[14:23:46.456] Indexed /path/to/file.cpp (42 symbols, 10 refs, 3 files)
            indexing_complete_regex: Regex::new(
                r"I\[\d{2}:\d{2}:\d{2}\.\d{3}\] Indexed (.+?) \((\d+) symbols?, (\d+) refs?, \d+ files?\)",
            )?,

            // I[14:23:47.789] Indexing c++20 standard library in the context of /path/to/file.cpp
            stdlib_start_regex: Regex::new(
                r"I\[\d{2}:\d{2}:\d{2}\.\d{3}\] Indexing (.+?) standard library in the context of (.+)",
            )?,

            // I[14:23:48.000] Indexed c++20 standard library: 1234 symbols, 567 filtered
            stdlib_complete_regex: Regex::new(
                r"I\[\d{2}:\d{2}:\d{2}\.\d{3}\] Indexed (.+?) standard library: (\d+) symbols?, (\d+) filtered",
            )?,
        })
    }
}

impl Default for ClangdLogParser {
    fn default() -> Self {
        Self::new().expect("Failed to compile regex patterns")
    }
}

impl LogParser for ClangdLogParser {
    fn parse_line(&self, line: &str) -> Option<ProgressEvent> {
        // Try indexing start pattern
        if let Some(captures) = self.indexing_start_regex.captures(line) {
            let path = captures.get(1)?.as_str();
            let digest = captures.get(2)?.as_str();

            return Some(ProgressEvent::FileIndexingStarted {
                path: PathBuf::from(path),
                digest: digest.to_string(),
            });
        }

        // Try indexing complete pattern
        if let Some(captures) = self.indexing_complete_regex.captures(line) {
            let path = captures.get(1)?.as_str();
            let symbols: u32 = captures.get(2)?.as_str().parse().ok()?;
            let refs: u32 = captures.get(3)?.as_str().parse().ok()?;

            return Some(ProgressEvent::FileIndexingCompleted {
                path: PathBuf::from(path),
                symbols,
                refs,
            });
        }

        // Try stdlib start pattern
        if let Some(captures) = self.stdlib_start_regex.captures(line) {
            let stdlib_version = captures.get(1)?.as_str();
            let context_file = captures.get(2)?.as_str();

            return Some(ProgressEvent::StandardLibraryStarted {
                context_file: PathBuf::from(context_file),
                stdlib_version: stdlib_version.to_string(),
            });
        }

        // Try stdlib complete pattern
        if let Some(captures) = self.stdlib_complete_regex.captures(line) {
            let symbols: u32 = captures.get(2)?.as_str().parse().ok()?;
            let filtered: u32 = captures.get(3)?.as_str().parse().ok()?;

            return Some(ProgressEvent::StandardLibraryCompleted { symbols, filtered });
        }

        None
    }
}

/// Log monitor that processes clangd stderr output
pub struct LogMonitor {
    parser: Box<dyn LogParser>,
    handler: Arc<std::sync::Mutex<Option<Arc<dyn ProgressHandler>>>>,
}

impl LogMonitor {
    /// Create a new log monitor with the default parser
    pub fn new() -> Self {
        Self {
            parser: Box::new(ClangdLogParser::default()),
            handler: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Create a log monitor with a custom parser
    pub fn with_parser(parser: Box<dyn LogParser>) -> Self {
        Self {
            parser,
            handler: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Set the progress handler
    pub fn set_handler(&mut self, handler: Arc<dyn ProgressHandler>) {
        if let Ok(mut handler_guard) = self.handler.lock() {
            *handler_guard = Some(handler);
        }
    }

    /// Get a mutable reference to set the handler
    pub fn handler_mut(&mut self) -> Arc<std::sync::Mutex<Option<Arc<dyn ProgressHandler>>>> {
        Arc::clone(&self.handler)
    }

    /// Process a single log line
    pub fn process_line(&self, line: &str) {
        trace!("LogMonitor: Processing stderr line: {}", line);

        if let Some(event) = self.parser.parse_line(line) {
            trace!("LogMonitor: Parsed event from stderr: {:?}", event);

            if let Ok(handler_guard) = self.handler.lock()
                && let Some(ref handler) = *handler_guard
            {
                handler.handle_event(event);
            }
        } else {
            trace!("LogMonitor: No event parsed from line: {}", line);
        }
    }

    /// Create a stderr line processor that can be used as a callback
    pub fn create_stderr_processor(&self) -> impl Fn(String) + Send + Sync + 'static {
        let parser = ClangdLogParser::default();
        let handler_ref = Arc::clone(&self.handler);

        move |line: String| {
            trace!("LogMonitor: Processing stderr line: {}", line);

            if let Some(event) = parser.parse_line(&line) {
                trace!("LogMonitor: Parsed event from stderr: {:?}", event);

                if let Ok(handler_guard) = handler_ref.lock()
                    && let Some(ref handler) = *handler_guard
                {
                    handler.handle_event(event);
                }
            } else {
                trace!("LogMonitor: No event parsed from line: {}", line);
            }
        }
    }

    /// Process stderr stream asynchronously
    pub async fn monitor_stream<R>(&self, reader: R) -> Result<(), std::io::Error>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut lines = BufReader::new(reader).lines();

        debug!("LogMonitor: Starting stderr monitoring");

        while let Some(line) = lines.next_line().await? {
            self.process_line(&line);
        }

        debug!("LogMonitor: Stderr monitoring ended");
        Ok(())
    }
}

impl Default for LogMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct TestHandler {
        events: Arc<Mutex<Vec<ProgressEvent>>>,
    }

    impl ProgressHandler for TestHandler {
        fn handle_event(&self, event: ProgressEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[test]
    fn test_clangd_log_parser_creation() {
        let parser = ClangdLogParser::new();
        assert!(parser.is_ok());
    }

    #[test]
    fn test_parse_indexing_start_log() {
        let parser = ClangdLogParser::default();
        let line = "V[14:23:45.123] Indexing /path/to/file.cpp (digest:=0x1234ABCD)";

        let event = parser.parse_line(line);

        assert!(event.is_some());
        match event.unwrap() {
            ProgressEvent::FileIndexingStarted { path, digest } => {
                assert_eq!(path, PathBuf::from("/path/to/file.cpp"));
                assert_eq!(digest, "0x1234ABCD");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_parse_indexing_complete_log() {
        let parser = ClangdLogParser::default();
        let line = "I[14:23:46.456] Indexed /path/to/file.cpp (42 symbols, 10 refs, 3 files)";

        let event = parser.parse_line(line);

        assert!(event.is_some());
        match event.unwrap() {
            ProgressEvent::FileIndexingCompleted {
                path,
                symbols,
                refs,
            } => {
                assert_eq!(path, PathBuf::from("/path/to/file.cpp"));
                assert_eq!(symbols, 42);
                assert_eq!(refs, 10);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_parse_stdlib_indexing_start() {
        let parser = ClangdLogParser::default();
        let line =
            "I[14:23:47.789] Indexing c++20 standard library in the context of /path/to/file.cpp";

        let event = parser.parse_line(line);

        assert!(event.is_some());
        match event.unwrap() {
            ProgressEvent::StandardLibraryStarted {
                context_file,
                stdlib_version,
            } => {
                assert_eq!(context_file, PathBuf::from("/path/to/file.cpp"));
                assert_eq!(stdlib_version, "c++20");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_parse_stdlib_indexing_complete() {
        let parser = ClangdLogParser::default();
        let line = "I[14:23:48.000] Indexed c++20 standard library: 1234 symbols, 567 filtered";

        let event = parser.parse_line(line);

        assert!(event.is_some());
        match event.unwrap() {
            ProgressEvent::StandardLibraryCompleted { symbols, filtered } => {
                assert_eq!(symbols, 1234);
                assert_eq!(filtered, 567);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_parse_ignore_unrelated_logs() {
        let parser = ClangdLogParser::default();
        let line = "I[14:23:48.000] Some other log message";

        let event = parser.parse_line(line);
        assert!(event.is_none());
    }

    #[test]
    fn test_log_monitor_creation() {
        let monitor = LogMonitor::new();
        assert!(monitor.handler.lock().unwrap().is_none());
    }

    #[test]
    fn test_log_monitor_with_handler() {
        let mut monitor = LogMonitor::new();
        let handler = Arc::new(TestHandler::default());

        monitor.set_handler(handler.clone());
        assert!(monitor.handler.lock().unwrap().is_some());

        // Test processing a line
        let line = "V[14:23:45.123] Indexing /test.cpp (digest:=0xABC)";
        monitor.process_line(line);

        let events = handler.events.lock().unwrap();
        assert_eq!(events.len(), 1);

        match &events[0] {
            ProgressEvent::FileIndexingStarted { path, digest } => {
                assert_eq!(*path, PathBuf::from("/test.cpp"));
                assert_eq!(digest, "0xABC");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_log_monitor_no_handler() {
        let monitor = LogMonitor::new();

        // Should not panic when no handler is set
        let line = "V[14:23:45.123] Indexing /test.cpp (digest:=0xABC)";
        monitor.process_line(line);
    }

    #[tokio::test]
    async fn test_monitor_stream() {
        let mut monitor = LogMonitor::new();
        let handler = Arc::new(TestHandler::default());
        monitor.set_handler(handler.clone());

        let log_data = "V[14:23:45.123] Indexing /test1.cpp (digest:=0xABC)\n\
                       I[14:23:46.456] Indexed /test1.cpp (42 symbols, 10 refs, 3 files)\n\
                       I[14:23:47.000] Some unrelated log\n\
                       V[14:23:48.123] Indexing /test2.cpp (digest:=0xDEF)\n";

        let cursor = std::io::Cursor::new(log_data.as_bytes());

        // Monitor the stream
        monitor.monitor_stream(cursor).await.unwrap();

        // Check captured events
        let events = handler.events.lock().unwrap();
        assert_eq!(events.len(), 3);

        match &events[0] {
            ProgressEvent::FileIndexingStarted { path, .. } => {
                assert_eq!(*path, PathBuf::from("/test1.cpp"));
            }
            _ => panic!("Wrong event type at index 0"),
        }

        match &events[1] {
            ProgressEvent::FileIndexingCompleted {
                path,
                symbols,
                refs,
            } => {
                assert_eq!(*path, PathBuf::from("/test1.cpp"));
                assert_eq!(*symbols, 42);
                assert_eq!(*refs, 10);
            }
            _ => panic!("Wrong event type at index 1"),
        }

        match &events[2] {
            ProgressEvent::FileIndexingStarted { path, .. } => {
                assert_eq!(*path, PathBuf::from("/test2.cpp"));
            }
            _ => panic!("Wrong event type at index 2"),
        }
    }

    #[test]
    fn test_regex_edge_cases() {
        let parser = ClangdLogParser::default();

        // Test with different timestamp formats
        let line1 = "V[01:02:03.999] Indexing /some/path.cpp (digest:=ABC123)";
        assert!(parser.parse_line(line1).is_some());

        // Test with different digest formats
        let line2 = "V[14:23:45.123] Indexing /path.cpp (digest:=0x1234ABCDEF)";
        assert!(parser.parse_line(line2).is_some());

        // Test with paths containing spaces (should not match due to regex)
        let line3 = "V[14:23:45.123] Indexing /path with spaces/file.cpp (digest:=ABC)";
        // This will match because our regex uses .+? which is non-greedy
        assert!(parser.parse_line(line3).is_some());

        // Test malformed lines
        let line4 = "V[14:23:45.123] Indexing incomplete line";
        assert!(parser.parse_line(line4).is_none());
    }
}
