use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "processId")]
    pub process_id: Option<u32>,
    #[serde(rename = "rootUri")]
    pub root_uri: Option<String>,
    pub capabilities: ClientCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(rename = "textDocument")]
    pub text_document: Option<TextDocumentClientCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentClientCapabilities {
    pub completion: Option<Value>,
    pub hover: Option<Value>,
    pub definition: Option<Value>,
    pub references: Option<Value>,
    #[serde(rename = "documentSymbol")]
    pub document_symbol: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(rename = "completionProvider")]
    pub completion_provider: Option<bool>,
    #[serde(rename = "hoverProvider")]
    pub hover_provider: Option<bool>,
    #[serde(rename = "definitionProvider")]
    pub definition_provider: Option<bool>,
    #[serde(rename = "referencesProvider")]
    pub references_provider: Option<bool>,
    #[serde(rename = "documentSymbolProvider")]
    pub document_symbol_provider: Option<bool>,
}

impl JsonRpcRequest {
    pub fn new(method: String, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            method,
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IndexingStatus {
    NotStarted,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingState {
    pub status: IndexingStatus,
    pub files_processed: u32,
    pub total_files: Option<u32>,
    pub percentage: Option<u8>,
    pub message: Option<String>,
    #[serde(skip)]
    pub start_time: Option<std::time::Instant>,
    pub estimated_completion_seconds: Option<u32>,
}

impl Default for IndexingState {
    fn default() -> Self {
        Self {
            status: IndexingStatus::NotStarted,
            files_processed: 0,
            total_files: None,
            percentage: None,
            message: None,
            start_time: None,
            estimated_completion_seconds: None,
        }
    }
}

impl IndexingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_indexing(&mut self, title: Option<String>) {
        self.status = IndexingStatus::InProgress;
        self.files_processed = 0;
        self.total_files = None;
        self.percentage = None;
        self.message = title;
        self.start_time = Some(std::time::Instant::now());
        self.estimated_completion_seconds = None;
    }

    pub fn update_progress(&mut self, message: Option<String>, percentage: Option<u8>) {
        if self.status != IndexingStatus::InProgress {
            return;
        }

        self.message = message;
        self.percentage = percentage;

        // Calculate time estimate based on progress
        if let (Some(start_time), Some(pct)) = (self.start_time, percentage) {
            if pct > 0 {
                let elapsed = start_time.elapsed();
                let estimated_total = elapsed.as_secs() * 100 / pct as u64;
                let remaining = estimated_total.saturating_sub(elapsed.as_secs());
                self.estimated_completion_seconds = Some(remaining as u32);
            }
        }

        // If no data for estimate and only one file, use 1 second default
        if self.estimated_completion_seconds.is_none() && self.total_files.is_none_or(|t| t <= 1) {
            self.estimated_completion_seconds = Some(1);
        }
    }

    pub fn complete_indexing(&mut self) {
        self.status = IndexingStatus::Completed;
        self.percentage = Some(100);
        self.estimated_completion_seconds = Some(0);
        self.message = None; // Clear message when indexing is completed
    }

    pub fn is_indexing(&self) -> bool {
        self.status == IndexingStatus::InProgress
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_indexing_state_default() {
        let state = IndexingState::default();
        assert_eq!(state.status, IndexingStatus::NotStarted);
        assert_eq!(state.files_processed, 0);
        assert_eq!(state.total_files, None);
        assert_eq!(state.percentage, None);
        assert_eq!(state.message, None);
        assert_eq!(state.start_time, None);
        assert_eq!(state.estimated_completion_seconds, None);
        assert!(!state.is_indexing());
    }

    #[test]
    fn test_indexing_state_start_indexing() {
        let mut state = IndexingState::new();
        let title = Some("Building index".to_string());
        
        state.start_indexing(title.clone());
        
        assert_eq!(state.status, IndexingStatus::InProgress);
        assert_eq!(state.files_processed, 0);
        assert_eq!(state.total_files, None);
        assert_eq!(state.percentage, None);
        assert_eq!(state.message, title);
        assert!(state.start_time.is_some());
        assert_eq!(state.estimated_completion_seconds, None);
        assert!(state.is_indexing());
    }

    #[test]
    fn test_indexing_state_update_progress() {
        let mut state = IndexingState::new();
        state.start_indexing(Some("Building index".to_string()));
        
        // Sleep for a short time to ensure progress calculation works
        std::thread::sleep(Duration::from_millis(10));
        
        let message = Some("Processing files".to_string());
        let percentage = Some(50);
        
        state.update_progress(message.clone(), percentage);
        
        assert_eq!(state.status, IndexingStatus::InProgress);
        assert_eq!(state.message, message);
        assert_eq!(state.percentage, percentage);
        assert!(state.estimated_completion_seconds.is_some());
        assert!(state.is_indexing());
    }

    #[test]
    fn test_indexing_state_update_progress_single_file() {
        let mut state = IndexingState::new();
        state.start_indexing(Some("Building index".to_string()));
        state.total_files = Some(1);
        
        let message = Some("Processing file".to_string());
        let percentage = None;
        
        state.update_progress(message.clone(), percentage);
        
        assert_eq!(state.status, IndexingStatus::InProgress);
        assert_eq!(state.message, message);
        assert_eq!(state.percentage, percentage);
        assert_eq!(state.estimated_completion_seconds, Some(1)); // Default for single file
        assert!(state.is_indexing());
    }

    #[test]
    fn test_indexing_state_complete_indexing() {
        let mut state = IndexingState::new();
        state.start_indexing(Some("Building index".to_string()));
        
        // Add a progress update to ensure message is present
        state.update_progress(Some("Processing files".to_string()), Some(50));
        assert_eq!(state.message, Some("Processing files".to_string()));
        
        state.complete_indexing();
        
        assert_eq!(state.status, IndexingStatus::Completed);
        assert_eq!(state.percentage, Some(100));
        assert_eq!(state.estimated_completion_seconds, Some(0));
        assert_eq!(state.message, None); // Message should be cleared
        assert!(!state.is_indexing());
    }

    #[test]
    fn test_indexing_state_update_progress_when_not_in_progress() {
        let mut state = IndexingState::new();
        assert_eq!(state.status, IndexingStatus::NotStarted);
        
        let message = Some("Should not update".to_string());
        let percentage = Some(25);
        
        state.update_progress(message.clone(), percentage);
        
        // Should not update when not in progress
        assert_eq!(state.status, IndexingStatus::NotStarted);
        assert_eq!(state.message, None);
        assert_eq!(state.percentage, None);
        assert!(!state.is_indexing());
    }

    #[test]
    fn test_indexing_status_serialization() {
        let state = IndexingState {
            status: IndexingStatus::InProgress,
            files_processed: 5,
            total_files: Some(10),
            percentage: Some(50),
            message: Some("Processing files".to_string()),
            start_time: Some(std::time::Instant::now()),
            estimated_completion_seconds: Some(30),
        };
        
        // Test serialization (start_time should be skipped)
        let serialized = serde_json::to_value(&state).unwrap();
        assert!(serialized.get("start_time").is_none());
        assert_eq!(serialized.get("status").unwrap(), "InProgress");
        assert_eq!(serialized.get("files_processed").unwrap(), 5);
        assert_eq!(serialized.get("total_files").unwrap(), 10);
        assert_eq!(serialized.get("percentage").unwrap(), 50);
        assert_eq!(serialized.get("message").unwrap(), "Processing files");
        assert_eq!(serialized.get("estimated_completion_seconds").unwrap(), 30);
    }
}