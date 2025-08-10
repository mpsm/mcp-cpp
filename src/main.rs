mod clangd;
mod cmake;
mod io;
mod legacy_lsp;
mod logging;
mod lsp_v2;
mod project;
mod server;
mod tools {
    pub mod analyze_symbols;
    pub mod project_tools;
    pub mod search_symbols;
    pub mod symbol_filtering;
    pub mod utils;
}

#[cfg(test)]
mod test_utils;

use clap::Parser;
use logging::{LogConfig, init_logging};
use project::{ProjectScanner, ProjectWorkspace};
use rust_mcp_sdk::schema::{
    Implementation, InitializeResult, LATEST_PROTOCOL_VERSION, ServerCapabilities,
    ServerCapabilitiesTools,
};
use server::CppServerHandler;

use rust_mcp_sdk::{
    McpServer, StdioTransport, TransportOptions,
    error::SdkResult,
    mcp_server::{ServerRuntime, server_runtime},
};
use std::path::{Path, PathBuf};
use tracing::info;

/// CLI arguments for the MCP C++ server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Project root directory to scan for build configurations (defaults to current directory)
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Global compilation database path (overrides per-component compilation databases)
    #[arg(long, short = 'c', value_name = "FILE")]
    compilation_database: Option<PathBuf>,

    /// Log level (overrides RUST_LOG env var)
    #[arg(long, value_name = "LEVEL")]
    log_level: Option<String>,

    /// Log file path (overrides MCP_LOG_FILE env var)
    #[arg(long, value_name = "FILE")]
    log_file: Option<PathBuf>,
}

/// Detect global compilation database path from CLI args and auto-detection
fn detect_global_compilation_database_from_args(
    project_root: &Path,
    compilation_database_arg: &Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(explicit_path) = compilation_database_arg {
        // User provided explicit override - validate it exists
        if explicit_path.exists() && explicit_path.is_file() {
            Some(explicit_path.clone())
        } else {
            eprintln!(
                "Error: Compilation database not found: {}",
                explicit_path.display()
            );
            std::process::exit(1);
        }
    } else {
        // Auto-detect: check for compile_commands.json in project root
        let auto_detect_path = project_root.join("compile_commands.json");
        if auto_detect_path.exists() && auto_detect_path.is_file() {
            Some(auto_detect_path)
        } else {
            None
        }
    }
}

/// Create ProjectWorkspace with all project setup logic centralized
fn create_project_workspace(
    project_root: PathBuf,
    global_compilation_db: Option<PathBuf>,
) -> ProjectWorkspace {
    info!(
        "Scanning project root for build configurations: {} (depth: 3)",
        project_root.display()
    );

    // Create project scanner with default providers
    let scanner = ProjectScanner::with_default_providers();

    // Scan the project root with depth 3
    let mut project_workspace = match scanner.scan_project(&project_root, 3, None) {
        Ok(project_workspace) => {
            info!(
                "Successfully discovered {} components across {} providers: {:?}",
                project_workspace.component_count(),
                project_workspace.get_provider_types().len(),
                project_workspace.get_provider_types()
            );
            project_workspace
        }
        Err(e) => {
            eprintln!(
                "Failed to scan project at {}: {}",
                project_root.display(),
                e
            );
            // Create empty ProjectWorkspace as fallback
            ProjectWorkspace::new(project_root, Vec::new(), 3)
        }
    };

    // Apply global compilation database if provided
    if let Some(global_path) = global_compilation_db {
        info!(
            "Using global compilation database: {}",
            global_path.display()
        );
        project_workspace.global_compilation_database = Some(
            crate::project::CompilationDatabase::new(global_path)
                .expect("Failed to load global compilation database"),
        );
    }

    project_workspace
}

#[tokio::main]
async fn main() -> SdkResult<()> {
    let args = Args::parse();

    // Extract values before moving
    let log_level = args.log_level.clone();
    let log_file = args.log_file.clone();
    let root_arg = args.root.clone();
    let compilation_database_arg = args.compilation_database.clone();

    // Initialize logging with configuration from env vars and CLI args
    let log_config = LogConfig::from_env().with_overrides(log_level, log_file);

    if let Err(e) = init_logging(log_config) {
        eprintln!("Failed to initialize logging: {e}");
        std::process::exit(1);
    }

    // Resolve project root directory
    let project_root = root_arg.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|e| {
            eprintln!("Failed to get current directory: {e}");
            std::process::exit(1);
        })
    });

    // Detect global compilation database
    let global_compilation_db =
        detect_global_compilation_database_from_args(&project_root, &compilation_database_arg);

    // Create ProjectWorkspace with all project setup
    let project_workspace = create_project_workspace(project_root, global_compilation_db);

    info!(
        "Starting C++ MCP Server with project root: {}",
        project_workspace.project_root_path.display()
    );

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

    // Create custom handler with ProjectWorkspace
    let handler = CppServerHandler::new(project_workspace);

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
