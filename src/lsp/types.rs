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