use rust_mcp_sdk::error::SdkResult;
use rust_mcp_sdk::schema::{
    ListResourcesRequest, ListResourcesResult, ReadResourceRequest, ReadResourceResult, Resource,
    TextResourceContents,
};
use serde_json::json;

pub struct LspResources;

impl LspResources {
    pub fn list_resources(_request: ListResourcesRequest) -> SdkResult<ListResourcesResult> {
        let resources = vec![
            Resource {
                uri: "lsp://workflow".to_string(),
                name: "C++ LSP Analysis Workflow".to_string(),
                description: Some(
                    "Complete workflow guide for using LSP tools with clangd".to_string(),
                ),
                mime_type: Some("text/markdown".to_string()),
                title: None,
                annotations: None,
                meta: None,
                size: None,
            },
            Resource {
                uri: "lsp://methods".to_string(),
                name: "LSP Methods Reference".to_string(),
                description: Some("Available LSP methods and their usage".to_string()),
                mime_type: Some("application/json".to_string()),
                title: None,
                annotations: None,
                meta: None,
                size: None,
            },
            Resource {
                uri: "lsp://capabilities".to_string(),
                name: "Clangd LSP Capabilities".to_string(),
                description: Some("Clangd-specific LSP capabilities and features".to_string()),
                mime_type: Some("application/json".to_string()),
                title: None,
                annotations: None,
                meta: None,
                size: None,
            },
            Resource {
                uri: "lsp://examples".to_string(),
                name: "LSP Request Examples".to_string(),
                description: Some("Common LSP request examples with parameters".to_string()),
                mime_type: Some("application/json".to_string()),
                title: None,
                annotations: None,
                meta: None,
                size: None,
            },
        ];

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    pub fn read_resource(request: ReadResourceRequest) -> SdkResult<ReadResourceResult> {
        let content = match request.params.uri.as_str() {
            "lsp://workflow" => Self::workflow_content(),
            "lsp://methods" => Self::methods_content(),
            "lsp://capabilities" => Self::capabilities_content(),
            "lsp://examples" => Self::examples_content(),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Unknown resource URI",
                )
                .into());
            }
        };

        Ok(ReadResourceResult {
            contents: vec![
                rust_mcp_sdk::schema::ReadResourceResultContentsItem::TextResourceContents(content),
            ],
            meta: None,
        })
    }

    fn workflow_content() -> TextResourceContents {
        let markdown = r#"# C++ LSP Analysis Workflow

## Overview
This workflow enables AI agents to perform semantic analysis of C++ code using clangd LSP server.

## Required Steps

### 1. [Optional] Discover Build Directories
Use `cpp_project_status` tool to scan for CMake build directories:
```json
{
  "name": "cpp_project_status"
}
```

### 2. [Required] Setup Clangd
Use `setup_clangd` tool to initialize clangd for a specific build directory:
```json
{
  "name": "setup_clangd",
  "arguments": {
    "buildDirectory": "/path/to/build"
  }
}
```

**Requirements:**
- Build directory must contain `compile_commands.json`
- clangd binary must be available (use CLANGD_PATH env var if needed)

### 3. [Use] Send LSP Requests
Use `lsp_request` tool for semantic analysis:
```json
{
  "name": "lsp_request",
  "arguments": {
    "method": "textDocument/definition",
    "params": {
      "textDocument": {"uri": "file:///path/to/file.cpp"},
      "position": {"line": 10, "character": 5}
    }
  }
}
```

## Important Notes

- Steps must be executed in order
- Only one clangd instance runs at a time per build directory
- Switching build directories will terminate the previous clangd process
- LSP requests will fail with helpful error if clangd not setup first

## Common LSP Methods

- `textDocument/definition` - Go to definition
- `textDocument/hover` - Get symbol information
- `textDocument/completion` - Code completion
- `textDocument/references` - Find references
- `textDocument/documentSymbol` - List file symbols
- `workspace/symbol` - Search workspace symbols

See `lsp://methods` and `lsp://examples` resources for detailed usage.
"#;

        TextResourceContents {
            text: markdown.to_string(),
            uri: "lsp://workflow".to_string(),
            mime_type: Some("text/markdown".to_string()),
            meta: None,
        }
    }

    fn methods_content() -> TextResourceContents {
        let methods = json!({
            "textDocument": {
                "definition": {
                    "description": "Go to symbol definition",
                    "params": {
                        "textDocument": {"uri": "file URI"},
                        "position": {"line": "0-based line", "character": "0-based column"}
                    }
                },
                "hover": {
                    "description": "Get hover information for symbol",
                    "params": {
                        "textDocument": {"uri": "file URI"},
                        "position": {"line": "0-based line", "character": "0-based column"}
                    }
                },
                "completion": {
                    "description": "Get code completion suggestions",
                    "params": {
                        "textDocument": {"uri": "file URI"},
                        "position": {"line": "0-based line", "character": "0-based column"}
                    }
                },
                "references": {
                    "description": "Find all references to symbol",
                    "params": {
                        "textDocument": {"uri": "file URI"},
                        "position": {"line": "0-based line", "character": "0-based column"},
                        "context": {"includeDeclaration": true}
                    }
                },
                "documentSymbol": {
                    "description": "List all symbols in document",
                    "params": {
                        "textDocument": {"uri": "file URI"}
                    }
                }
            },
            "workspace": {
                "symbol": {
                    "description": "Search workspace symbols",
                    "params": {
                        "query": "symbol name or pattern"
                    }
                }
            }
        });

        TextResourceContents {
            text: serde_json::to_string_pretty(&methods).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize methods: {}\"}}", e)
            }),
            uri: "lsp://methods".to_string(),
            mime_type: Some("application/json".to_string()),
            meta: None,
        }
    }

    fn capabilities_content() -> TextResourceContents {
        let capabilities = json!({
            "clangd_specific": {
                "features": [
                    "C++ semantic analysis",
                    "Template instantiation",
                    "Include resolution",
                    "Macro expansion",
                    "Cross-references",
                    "Call hierarchy",
                    "Type hierarchy"
                ],
                "limitations": [
                    "Limited C++20 modules support",
                    "No background indexing (by design)",
                    "Requires compile_commands.json"
                ]
            },
            "supported_lsp_methods": [
                "initialize",
                "textDocument/definition",
                "textDocument/hover",
                "textDocument/completion",
                "textDocument/references",
                "textDocument/documentSymbol",
                "workspace/symbol",
                "textDocument/publishDiagnostics"
            ],
            "configuration": {
                "binary_path": "Set via CLANGD_PATH environment variable",
                "background_indexing": "Disabled for on-demand analysis",
                "working_directory": "Set to build directory with compile_commands.json"
            }
        });

        TextResourceContents {
            text: serde_json::to_string_pretty(&capabilities).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize capabilities: {}\"}}", e)
            }),
            uri: "lsp://capabilities".to_string(),
            mime_type: Some("application/json".to_string()),
            meta: None,
        }
    }

    fn examples_content() -> TextResourceContents {
        let examples = json!({
            "go_to_definition": {
                "method": "textDocument/definition",
                "params": {
                    "textDocument": {"uri": "file:///home/user/project/src/main.cpp"},
                    "position": {"line": 15, "character": 8}
                }
            },
            "hover_information": {
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": "file:///home/user/project/src/main.cpp"},
                    "position": {"line": 15, "character": 8}
                }
            },
            "code_completion": {
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": "file:///home/user/project/src/main.cpp"},
                    "position": {"line": 20, "character": 10}
                }
            },
            "find_references": {
                "method": "textDocument/references",
                "params": {
                    "textDocument": {"uri": "file:///home/user/project/src/main.cpp"},
                    "position": {"line": 15, "character": 8},
                    "context": {"includeDeclaration": true}
                }
            },
            "document_symbols": {
                "method": "textDocument/documentSymbol",
                "params": {
                    "textDocument": {"uri": "file:///home/user/project/src/main.cpp"}
                }
            },
            "workspace_symbol_search": {
                "method": "workspace/symbol",
                "params": {
                    "query": "MyClass"
                }
            }
        });

        TextResourceContents {
            text: serde_json::to_string_pretty(&examples).unwrap_or_else(|e| {
                format!("{{\"error\": \"Failed to serialize examples: {}\"}}", e)
            }),
            uri: "lsp://examples".to_string(),
            mime_type: Some("application/json".to_string()),
            meta: None,
        }
    }
}
