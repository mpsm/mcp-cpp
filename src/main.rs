mod clangd;
mod io;
mod logging;
mod lsp;
mod mcp_server;
mod project;
mod symbol;

#[cfg(test)]
mod test_utils;

use clap::Parser;
use logging::{LogConfig, init_logging};
use mcp_server::CppServerHandler;
use project::{ProjectScanner, ProjectWorkspace};

use rmcp::service::serve_server;
use std::path::PathBuf;
use tokio::io::{stdin, stdout};
use tracing::info;

/// CLI arguments for the MCP C++ server
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Project root directory to scan for build configurations (defaults to current directory)
    #[arg(long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// Path to clangd executable (overrides CLANGD_PATH env var)
    #[arg(long, value_name = "PATH")]
    clangd_path: Option<String>,

    /// Log level (overrides RUST_LOG env var)
    #[arg(long, value_name = "LEVEL")]
    log_level: Option<String>,

    /// Log file path (overrides MCP_LOG_FILE env var)
    #[arg(long, value_name = "FILE")]
    log_file: Option<PathBuf>,
}

/// Resolve clangd path from CLI args and environment
fn resolve_clangd_path(clangd_path_arg: Option<String>) -> String {
    // Priority: CLI arg > CLANGD_PATH env var > "clangd" default
    clangd_path_arg
        .or_else(|| std::env::var("CLANGD_PATH").ok())
        .unwrap_or_else(|| "clangd".to_string())
}

/// Create ProjectWorkspace with all project setup logic centralized
fn create_project_workspace(project_root: PathBuf) -> ProjectWorkspace {
    info!(
        "Scanning project root for build configurations: {} (depth: 3)",
        project_root.display()
    );

    // Create project scanner with default providers
    let scanner = ProjectScanner::with_default_providers();

    // Scan the project root with depth 3
    match scanner.scan_project(&project_root, 3, None) {
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
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Extract values before moving
    let log_level = args.log_level.clone();
    let log_file = args.log_file.clone();
    let root_arg = args.root.clone();

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

    // Create ProjectWorkspace with all project setup
    let project_workspace = create_project_workspace(project_root);

    info!(
        "Starting C++ MCP Server with project root: {}",
        project_workspace.project_root_path.display()
    );

    // Resolve clangd path
    let clangd_path = resolve_clangd_path(args.clangd_path);
    info!("Using clangd: {}", clangd_path);

    // Create handler with ProjectWorkspace and clangd path
    let handler = match CppServerHandler::new(project_workspace, clangd_path) {
        Ok(handler) => handler,
        Err(e) => {
            eprintln!("Failed to create server handler: {}", e);
            std::process::exit(1);
        }
    };

    // Create stdio transport
    let transport = (stdin(), stdout());

    // Start the server using serve_server
    info!("C++ MCP Server ready and listening for requests");

    let server = serve_server(handler, transport).await?;

    // Wait for server to finish
    let quit_reason = server.waiting().await?;
    info!("Server shutdown: {:?}", quit_reason);

    Ok(())
}
