//! Clangd stderr log monitor for progress tracking
//!
//! Monitors clangd's stderr output for indexing progress messages and emits
//! structured progress events. This complements the LSP progress notifications
//! with more detailed file-level progress information.

use crate::clangd::index::ProgressEvent;
use regex::Regex;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

/// Log parser trait for testing and extensibility
pub trait LogParser: Send + Sync {
    /// Parse a log line and return a progress event if applicable
    fn parse_line(&self, line: &str) -> Option<ProgressEvent>;
}

/// Default clangd log parser using regex patterns
#[derive(Clone)]
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
    parser: ClangdLogParser,
    event_sender: Option<mpsc::Sender<ProgressEvent>>,
}

impl LogMonitor {
    /// Create a new log monitor with the default parser (no progress events)
    pub fn new() -> Self {
        Self {
            parser: ClangdLogParser::default(),
            event_sender: None,
        }
    }

    /// Create a log monitor with the default parser and progress event sender
    pub fn with_sender(sender: mpsc::Sender<ProgressEvent>) -> Self {
        Self {
            parser: ClangdLogParser::default(),
            event_sender: Some(sender),
        }
    }

    /// Create a log monitor with a custom parser and progress event sender
    pub fn with_parser_and_sender(
        parser: ClangdLogParser,
        sender: mpsc::Sender<ProgressEvent>,
    ) -> Self {
        Self {
            parser,
            event_sender: Some(sender),
        }
    }

    /// Process a single log line
    pub fn process_line(&self, line: &str) {
        trace!("LogMonitor: Processing stderr line: {}", line);

        if let Some(event) = self.parser.parse_line(line) {
            trace!("LogMonitor: Parsed event from stderr: {:?}", event);

            if let Some(ref sender) = self.event_sender {
                // Non-blocking send - drop event if channel is full
                if sender.try_send(event).is_err() {
                    warn!("LogMonitor: Progress event channel full, dropping event");
                }
            }
        } else {
            trace!("LogMonitor: No event parsed from line: {}", line);
        }
    }

    /// Create a stderr line processor that can be used as a callback
    pub fn create_stderr_processor(&self) -> impl Fn(String) + Send + Sync + 'static {
        // Clone the existing parser instead of creating a duplicate
        let parser = self.parser.clone();
        let sender = self.event_sender.clone();

        move |line: String| {
            trace!("LogMonitor: Processing stderr line: {}", line);

            if let Some(event) = parser.parse_line(&line) {
                trace!("LogMonitor: Parsed event from stderr: {:?}", event);

                if let Some(ref tx) = sender {
                    // Non-blocking send - drop event if channel is full
                    if tx.try_send(event).is_err() {
                        warn!("LogMonitor: Progress event channel full, dropping event");
                    }
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
    use tokio::sync::mpsc;

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
        assert!(monitor.event_sender.is_none());
    }

    #[tokio::test]
    async fn test_log_monitor_with_channel() {
        let (tx, mut rx) = mpsc::channel(10);
        let monitor = LogMonitor::with_sender(tx);

        // Test processing a line
        let line = "V[14:23:45.123] Indexing /test.cpp (digest:=0xABC)";
        monitor.process_line(line);

        // Receive the event
        let event = rx.recv().await.expect("Should receive event");
        match event {
            ProgressEvent::FileIndexingStarted { path, digest } => {
                assert_eq!(path, PathBuf::from("/test.cpp"));
                assert_eq!(digest, "0xABC");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_log_monitor_no_sender() {
        let monitor = LogMonitor::new();

        // Should not panic when no sender is set
        let line = "V[14:23:45.123] Indexing /test.cpp (digest:=0xABC)";
        monitor.process_line(line);
    }

    #[tokio::test]
    async fn test_monitor_stream() {
        let (tx, mut rx) = mpsc::channel(10);
        let monitor = LogMonitor::with_sender(tx);

        let log_data = "V[14:23:45.123] Indexing /test1.cpp (digest:=0xABC)\n\
                       I[14:23:46.456] Indexed /test1.cpp (42 symbols, 10 refs, 3 files)\n\
                       I[14:23:47.000] Some unrelated log\n\
                       V[14:23:48.123] Indexing /test2.cpp (digest:=0xDEF)\n";

        let cursor = std::io::Cursor::new(log_data.as_bytes());

        // Monitor the stream
        monitor.monitor_stream(cursor).await.unwrap();

        // Collect all events
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
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
