mod cmake;
mod handler;
mod logging;
mod lsp;
mod tools;

use clap::Parser;
use handler::CppServerHandler;
use logging::{LogConfig, init_logging};
use rust_mcp_sdk::schema::{
    Implementation, InitializeResult, LATEST_PROTOCOL_VERSION, ServerCapabilities,
    ServerCapabilitiesTools,
};

use rust_mcp_sdk::{
    McpServer, StdioTransport, TransportOptions,
    error::SdkResult,
    mcp_server::{ServerRuntime, server_runtime},
};
use std::path::PathBuf;
use tracing::info;

/// CLI arguments for the MCP C++ server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Log level (overrides RUST_LOG env var)
    #[arg(long, value_name = "LEVEL")]
    log_level: Option<String>,

    /// Log file path (overrides MCP_LOG_FILE env var)
    #[arg(long, value_name = "FILE")]
    log_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> SdkResult<()> {
    let args = Args::parse();

    // Initialize logging with configuration from env vars and CLI args
    let log_config = LogConfig::from_env().with_overrides(args.log_level, args.log_file);

    if let Err(e) = init_logging(log_config) {
        eprintln!("Failed to initialize logging: {}", e);
        std::process::exit(1);
    }

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
            ..Default::default()
        },
        meta: None,
        instructions: Some("C++ project analysis and LSP bridge server".to_string()),
        protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
    };

    // Create stdio transport
    let transport = StdioTransport::new(TransportOptions::default())?;

    // Create custom handler
    let handler = CppServerHandler::new();

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
