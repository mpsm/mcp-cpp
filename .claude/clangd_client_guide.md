# clangd Client Implementation Guide

## Overview

This document provides a comprehensive guide for implementing a clangd Language Server Protocol (LSP) client, with specific focus on AI agent requirements and indexing workflows. Based on practical implementation experience with the `generate-index.py` tool and analysis of VS Code's clangd integration.

## Table of Contents

1. [Core Architecture](#core-architecture)
2. [LSP Communication Protocol](#lsp-communication-protocol)
3. [Client Initialization Workflow](#client-initialization-workflow)
4. [Capability Negotiation](#capability-negotiation)
5. [Indexing Lifecycle Management](#indexing-lifecycle-management)
6. [Progress Monitoring](#progress-monitoring)
7. [AI Agent Considerations](#ai-agent-considerations)
8. [Cache Management](#cache-management)
9. [Troubleshooting](#troubleshooting)
10. [Future Work](#future-work)

---

## Core Architecture

### clangd Process Management

clangd operates as a subprocess communicating via stdin/stdout using the LSP protocol:

```python
# Recommended clangd startup arguments
args = [
    "clangd",
    "--background-index",        # Enable background indexing
    "--clang-tidy",             # Enable static analysis
    "--completion-style=detailed", # Rich completion information
    "--log=verbose"             # Detailed logging for debugging
]

process = subprocess.Popen(
    args,
    cwd=build_directory,        # Must be project root with compile_commands.json
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=0                   # Unbuffered for real-time communication
)
```

### Communication Threads

Implement separate threads for:

- **Message Reader**: Parse incoming LSP messages from stdout
- **Stderr Monitor**: Track clangd logs and indexing progress
- **Main Thread**: Send requests and manage lifecycle

---

## LSP Communication Protocol

### Message Format

clangd uses the Language Server Protocol with HTTP-style headers:

```
Content-Length: <byte_count>\r\n
\r\n
<json_payload>
```

### Implementation Pattern

```python
def _send_json_rpc(self, message: Dict[str, Any]):
    """Send LSP message with proper headers"""
    json_text = json.dumps(message)
    content = f"Content-Length: {len(json_text)}\r\n\r\n{json_text}"

    with self.stdin_lock:  # Thread-safe sending
        self.process.stdin.write(content)
        self.process.stdin.flush()

def _read_messages(self):
    """Parse incoming LSP messages"""
    while self.process and self.process.poll() is None:
        # Read Content-Length header
        line = self.process.stdout.readline().strip()
        if line.startswith('Content-Length:'):
            content_length = int(line.split(':')[1].strip())

            # Skip empty line after headers
            self.process.stdout.readline()

            # Read JSON payload
            json_data = self.process.stdout.read(content_length)
            message = json.loads(json_data)
            self._handle_message(message)
```

---

## Client Initialization Workflow

### 1. Process Startup

- Start clangd with appropriate arguments
- Verify `compile_commands.json` exists in working directory
- Set up communication threads

### 2. LSP Handshake

```python
# Step 1: Send initialize request
init_params = {
    "processId": os.getpid(),
    "rootPath": str(build_directory),
    "rootUri": f"file://{build_directory}",
    "capabilities": { /* see capability section */ },
    "initializationOptions": {
        "clangdFileStatus": True,
        "fallbackFlags": ["-std=c++20"]
    }
}
self._send_request("initialize", init_params)

# Step 2: Send initialized notification
self._send_notification("initialized")
```

### File Opening (Critical for Indexing)

**Key Finding**: clangd only starts background indexing when a file is opened:

```python
def trigger_indexing_by_opening_file(self):
    """Opening a file triggers background indexing"""
    # Read a source file from the project
    with open(cpp_file, 'r') as f:
        content = f.read()

    # Send textDocument/didOpen
    did_open_params = {
        "textDocument": {
            "uri": f"file://{cpp_file}",
            "languageId": "cpp",
            "version": 1,
            "text": content
        }
    }
    self._send_notification("textDocument/didOpen", did_open_params)

    # Send additional requests that work with clangd
    doc_uri = {"uri": f"file://{cpp_file}"}
    self._send_request("textDocument/documentSymbol", {"textDocument": doc_uri})
    # Note: Don't send foldingRange requests - clangd doesn't support them
```

---

## Capability Negotiation

### Essential Capabilities for Indexing

```python
"capabilities": {
    "workspace": {
        "workDoneProgress": True,           # CRITICAL for indexing progress
        "workspaceFolders": True,           # Multi-folder project support
        "didChangeWatchedFiles": {          # File system monitoring
            "dynamicRegistration": True
        }
    },
    "window": {
        "workDoneProgress": True            # Progress reporting
    }
}
```

### Comprehensive AI Agent Capabilities

Based on analysis, AI agents benefit from human-like capabilities:

```python
"textDocument": {
    # Code Navigation
    "definition": {"linkSupport": True},
    "declaration": {"linkSupport": True},
    "references": {"context": True},
    "implementation": {"linkSupport": True},

    # Code Understanding
    "hover": {"contentFormat": ["markdown", "plaintext"]},
    "documentSymbol": {
        "hierarchicalDocumentSymbolSupport": True
    },
    "semanticTokens": {
        "dynamicRegistration": True,
        "requests": {"range": True, "full": {"delta": True}}
    },

    # Code Quality
    "diagnostic": {"relatedDocumentSupport": True},
    "inlayHint": {"dynamicRegistration": True},
    "codeAction": {
        "codeActionLiteralSupport": {
            "codeActionKind": {
                "valueSet": ["quickfix", "refactor", "source"]
            }
        }
    },

    # Interactive Features
    "completion": {
        "completionItem": {
            "snippetSupport": True,
            "documentationFormat": ["markdown", "plaintext"]
        }
    },
    "signatureHelp": {"signatureInformation": {
        "documentationFormat": ["markdown", "plaintext"]
    }},
    "codeLens": {"dynamicRegistration": True},
    "callHierarchy": {"dynamicRegistration": True}
}
```

### Capabilities to Avoid

Only exclude truly UI-specific features:

- `colorProvider`: Color swatches in editors
- `onTypeFormatting`: Format-as-you-type functionality

---

## Indexing Lifecycle Management

### Progress Token Handling

clangd requests progress tokens for indexing:

```python
def _handle_message(self, message: Dict[str, Any]):
    method = message.get("method", "")

    if method == "window/workDoneProgress/create":
        # clangd requesting progress token
        token = message["params"]["token"]
        self._send_response(message["id"], None)  # Accept token

    elif method == "$/progress":
        # Progress updates
        params = message["params"]
        token = params["token"]
        value = params["value"]

        if token == "backgroundIndexProgress":
            kind = value["kind"]
            if kind == "begin":
                print(f"Indexing started: {value['title']}")
            elif kind == "report":
                print(f"Progress: {value['message']} ({value['percentage']}%)")
            elif kind == "end":
                print("Indexing completed!")

    elif method == "textDocument/clangd.fileStatus":
        # File status updates
        params = message["params"]
        uri = params["uri"]
        state = params["state"]
        filename = Path(uri.replace("file://", "")).name
        print(f"File status: {filename} - {state}")

    elif method == "textDocument/publishDiagnostics":
        # Standard LSP diagnostic messages (errors, warnings, etc.)
        params = message["params"]
        uri = params["uri"]
        diagnostics = params["diagnostics"]
        filename = Path(uri.replace("file://", "")).name
        if diagnostics:
            print(f"Diagnostics for {filename}: {len(diagnostics)} issues")

    elif "result" in message:
        # Response to our request
        request_id = message["id"]
        print(f"Response to request {request_id}")

    elif "error" in message:
        # Error response (e.g., unsupported methods)
        error = message["error"]
        if error["code"] == -32601:  # Method not found
            print(f"Method not supported by clangd")
        else:
            print(f"Error: {error['message']}")
```

### File Status Monitoring

Track individual file indexing:

```python
elif method == "textDocument/clangd.fileStatus":
    params = message["params"]
    uri = params["uri"]
    state = params["state"]  # "indexed", "parsing", etc.
    filename = Path(uri.replace("file://", "")).name
    print(f"File status: {filename} - {state}")
```

### Completion Detection

Monitor stderr for indexing completion:

```python
def _read_stderr(self):
    """Monitor clangd logs for indexing progress"""
    while self.process and self.process.poll() is None:
        line = self.process.stderr.readline().strip()

        # Look for indexing completion messages
        if "Indexed " in line and "symbols" in line:
            # Parse: "Indexed /path/file.cpp (1234 symbols)"
            file_part = line.split("Indexed ")[1].split(" (")[0]
            filename = Path(file_part).name

            if filename in self.expected_files:
                self.indexed_files.add(filename)

                if len(self.indexed_files) >= self.total_files:
                    print("All files indexed!")
                    self.shutdown()
```

---

## Progress Monitoring

### Multi-Channel Monitoring

Implement multiple monitoring approaches:

1. **LSP Progress Protocol**: `$/progress` messages with `backgroundIndexProgress` token
2. **File Status Updates**: `textDocument/clangd.fileStatus` notifications
3. **Stderr Log Parsing**: Direct monitoring of clangd verbose logs

### Expected File Tracking

Load compilation database to track expected files:

```python
def load_expected_files(self):
    """Load files from compile_commands.json"""
    with open("compile_commands.json", 'r') as f:
        commands = json.load(f)

    for cmd in commands:
        if 'file' in cmd:
            file_path = Path(cmd['file'])
            self.expected_files.add(file_path.name)

    self.total_files = len(self.expected_files)
```

### Timeout Handling

Implement reasonable timeouts for indexing operations:

```python
timeout = 300  # 5 minutes for large codebases
start_time = time.time()

while (len(self.indexed_files) < self.total_files and
       time.time() - start_time < timeout):
    time.sleep(1)
```

---

## AI Agent Considerations

### Why AI Agents Need Human-Like Capabilities

**Key Insight**: AI agents operate on codebases like humans, not like simple automation tools.

#### Essential for AI Agents:

1. **File Change Monitoring** (`didChangeWatchedFiles`): AI agents need to know when the codebase changes
2. **Inlay Hints** (`inlayHint`): Type information and parameter names help AI understand code
3. **Semantic Tokens** (`semanticTokens`): Syntax highlighting reveals code structure and roles
4. **Code Actions** (`codeAction`): AI agents can suggest and apply fixes like humans
5. **Symbol Navigation** (`definition`, `references`): Critical for code understanding
6. **Workspace Symbols** (`symbol`): AI agents search across entire codebases

#### Workflow Integration:

```python
# AI agents can use the full spectrum of LSP features
def analyze_code_with_ai(self, file_uri):
    # Get type information
    hover_info = self._send_request("textDocument/hover", {
        "textDocument": {"uri": file_uri},
        "position": {"line": 10, "character": 5}
    })

    # Find all references
    references = self._send_request("textDocument/references", {
        "textDocument": {"uri": file_uri},
        "position": {"line": 10, "character": 5},
        "context": {"includeDeclaration": True}
    })

    # Get available code actions
    actions = self._send_request("textDocument/codeAction", {
        "textDocument": {"uri": file_uri},
        "range": {"start": {"line": 10, "character": 0},
                 "end": {"line": 10, "character": 50}},
        "context": {"diagnostics": []}
    })
```

---

## Cache Management

### Cache Locations

clangd stores index data in multiple locations:

```python
cache_locations = [
    Path.home() / ".cache" / "clangd",           # User cache
    Path.home() / ".clangd",                     # Legacy location
    build_directory / ".clangd",                 # Project-local
    build_directory.parent / ".clangd",          # Source root
    build_directory.parent / ".cache" / "clangd", # Project cache
    Path(os.environ.get("XDG_CACHE_HOME",
                       Path.home() / ".cache")) / "clangd"  # XDG standard
]
```

### Clean Cache Strategy

Always clean cache before fresh indexing:

```python
def clean_cache(self):
    """Clean all clangd cache directories"""
    for cache_dir in cache_locations:
        if cache_dir.exists() and cache_dir.is_dir():
            try:
                shutil.rmtree(cache_dir)
                print(f"Removed: {cache_dir}")
            except Exception as e:
                print(f"Could not remove {cache_dir}: {e}")
```

### When to Clean Cache

- Before initial indexing of a new project
- After significant build system changes
- When `compile_commands.json` is regenerated
- When switching between different project configurations

---

## Troubleshooting

### Common Issues

#### 1. Indexing Doesn't Start

**Symptoms**: No progress messages, no file status updates
**Solutions**:

- Ensure `workDoneProgress` capability is enabled
- Verify `compile_commands.json` exists in working directory
- **Critical**: Open a source file with `textDocument/didOpen`

#### 2. Incomplete Indexing

**Symptoms**: Some files never show as indexed
**Solutions**:

- Check `compile_commands.json` includes all expected files
- Verify file paths are accessible from clangd working directory
- Increase timeout for large codebases

#### 3. Communication Errors

**Symptoms**: JSON decode errors, broken messages
**Solutions**:

- Ensure proper Content-Length header calculation
- Use unbuffered I/O (`bufsize=0`)
- Implement thread-safe message sending

#### 4. Performance Issues

**Symptoms**: Slow indexing, high memory usage
**Solutions**:

- Clean cache before indexing
- Use `--background-index` for concurrent indexing
- Monitor system resources during indexing

#### 5. clangd-Specific Issues

**Symptoms**: "Method not found" errors for certain LSP requests
**Solutions**:

- Avoid unsupported methods like `textDocument/foldingRange`
- Handle `publishDiagnostics` properly (standard LSP method)
- Check clangd documentation for supported LSP features
- Use error handling for unsupported method responses:

```python
elif "error" in message:
    error = message["error"]
    if error["code"] == -32601:  # Method not found
        print(f"Method not supported by clangd")
```

### Debug Logging

Enable verbose logging for troubleshooting:

```python
# clangd startup with verbose logging
args.append("--log=verbose")

# Monitor stderr for detailed information
def _read_stderr(self):
    while self.process.poll() is None:
        line = self.process.stderr.readline().strip()
        if any(keyword in line for keyword in [
            "Indexed", "compilation database", "ASTWorker",
            "backgroundIndexProgress", "Building preamble"
        ]):
            print(f"[CLANGD LOG] {line}")
```

---

## Future Work

### Enhanced Progress Reporting

1. **File-Level Progress**: Track individual file indexing status
2. **Symbol Count Tracking**: Monitor symbol database growth
3. **Memory Usage Monitoring**: Track clangd resource consumption
4. **Incremental Updates**: Handle partial re-indexing efficiently

### Advanced AI Integration

1. **Capability Profiling**: Dynamically adjust capabilities based on project type
2. **Context-Aware Requests**: Send relevant LSP requests based on AI task context
3. **Batch Operations**: Optimize multiple requests for AI analysis workflows
4. **Error Recovery**: Handle clangd crashes and automatic restart

### Performance Optimization

1. **Parallel File Opening**: Open multiple files simultaneously to accelerate indexing
2. **Smart Cache Management**: Preserve partial indexes across sessions
3. **Resource Monitoring**: Implement memory and CPU usage limits
4. **Index Validation**: Verify index completeness and consistency

### Cross-Platform Support

1. **Windows Path Handling**: Handle drive letters and backslashes
2. **macOS Integration**: Optimize for macOS-specific clangd behaviors
3. **Docker Support**: Handle containerized development environments
4. **Remote Development**: Support for remote clangd instances

---

## Implementation Checklist

### Basic Client Implementation

- [ ] LSP message parsing with Content-Length headers
- [ ] Thread-safe request/response handling
- [ ] Process lifecycle management
- [ ] Error handling and recovery

### Indexing Support

- [ ] `workDoneProgress` capability implementation
- [ ] Progress token creation handling
- [ ] File status monitoring
- [ ] Completion detection via stderr parsing

### AI Agent Features

- [ ] Comprehensive capability negotiation
- [ ] Code navigation request handling
- [ ] Diagnostic and code action support
- [ ] Semantic token processing

### Production Readiness

- [ ] Cache management implementation
- [ ] Timeout and error handling
- [ ] Debug logging and monitoring
- [ ] Performance optimization

---

## Conclusion

Working with clangd as an LSP client requires understanding both the protocol mechanics and the specific behaviors of clangd. The key insights from this implementation:

1. **File Opening Triggers Indexing**: Unlike some language servers, clangd requires active file opening to start background indexing
2. **AI Agents Need Human-Like Capabilities**: Don't artificially limit AI agents - they benefit from the full spectrum of LSP features
3. **Multi-Channel Monitoring**: Use LSP progress, file status, and stderr logs for comprehensive progress tracking
4. **Cache Management is Critical**: Always clean cache for consistent indexing behavior

This guide provides the foundation for building robust clangd clients that can effectively support both human developers and AI agents in understanding and working with C++ codebases.

---

_Generated from practical implementation experience with the `generate-index.py` tool and analysis of VS Code's clangd integration patterns._
