mod handler;
mod cmake;
mod tools;
mod lsp;
mod resources;

use handler::CppServerHandler;
use rust_mcp_sdk::schema::{
    Implementation, InitializeResult, ServerCapabilities, ServerCapabilitiesTools,
    ServerCapabilitiesResources, LATEST_PROTOCOL_VERSION,
};

use rust_mcp_sdk::{
    error::SdkResult,
    mcp_server::{server_runtime, ServerRuntime},
    McpServer, StdioTransport, TransportOptions,
};
use tracing::info;

#[tokio::main]
async fn main() -> SdkResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting C++ MCP Server");

    // Define server details and capabilities
    let server_details = InitializeResult {
        server_info: Implementation {
            name: "C++ MCP Server".to_string(),
            version: "0.1.0".to_string(),
            title: Some("C++ Project Analysis MCP Server".to_string()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            resources: Some(ServerCapabilitiesResources { 
                subscribe: None, 
                list_changed: None 
            }),
            ..Default::default()
        },
        meta: None,
        instructions: Some("C++ project analysis and LSP bridge server".to_string()),
        protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
    };

    // Create stdio transport
    let transport = StdioTransport::new(TransportOptions::default())?;

    // Create custom handler
    let handler = CppServerHandler {};

    // Create MCP server
    let server: ServerRuntime = server_runtime::create_server(server_details, transport, handler);

    info!("C++ MCP Server ready, starting...");

    // Start the server
    if let Err(start_error) = server.start().await {
        eprintln!(
            "{}",
            start_error
                .rpc_error_message()
                .unwrap_or(&start_error.to_string())
        );
    }

    Ok(())
}