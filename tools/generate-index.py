#!/usr/bin/env python3
"""
Script to generate clangd index by mimicking VS Code's initialization sequence.
Automatically exits when clang        # Use same arguments as VS Code (from the log analysis)
        args = [
            self.clangd_path,
            "--background-index",  # Fixed: not --background-index=true
            "--clang-tidy",
            "--completion-style=detailed",
            "--log=verbose",  # To see indexing messages
            "--query-driver=**",  # Allow querying all drivers
            # Pass build directory to clangd
            f"--compile-commands-dir={self.build_directory}"
        ]indexing.
"""

import json
import subprocess
import threading
import time
import os
import sys
import shutil
import argparse
from pathlib import Path
from typing import Dict, Any


class ClangdIndexGenerator:
    def __init__(self, build_directory: str, clangd_path: str = "clangd",
                 refresh_index: bool = False, log_file: str = None, verbose: bool = False):
        self.build_directory = Path(build_directory).resolve()
        self.clangd_path = clangd_path
        self.refresh_index = refresh_index
        self.log_file = log_file
        self.verbose = verbose
        self.log_file_handle = None
        self.process = None
        self.reader = None
        self.writer = None
        self.request_id = 0
        self.indexing_progress = {}
        self.indexed_files = set()  # All files indexed (including headers)
        self.processed_compile_files = set()  # Files from compile_commands.json that have been processed
        # Files from compile_commands.json for reporting (now stores full paths)
        self.compile_commands_files = set()  # Set of Path objects
        self.compile_commands_files_by_name = set()  # Set of filenames for backward compatibility
        self.failed_files = set()  # Files that failed to index
        self.files_with_errors = {}  # filename -> error count
        self.indexing_complete = False
        self.last_indexing_activity = time.time()
        self.diagnostic_errors = 0  # Total count of diagnostic errors
        self.diagnostic_warnings = 0  # Total count of diagnostic warnings
        self.lsp_errors = 0  # Count of LSP protocol errors
        self.current_processing_file = ""  # Current file being processed for progress display

        # Open log file if specified
        if self.log_file:
            try:
                self.log_file_handle = open(self.log_file, 'w',
                                            encoding='utf-8')
                print(f"📝 Logging clangd output to: {self.log_file}")
            except Exception as e:
                print(f"⚠️  Warning: Could not open log file "
                      f"{self.log_file}: {e}")
                self.log_file = None

    def _log_message(self, message: str, source: str = "STDOUT"):
        """Log a message to both console and file if log file is specified"""
        timestamped_msg = f"[{time.strftime('%H:%M:%S')}] [{source}] {message}"

        # Always print to console
        print(timestamped_msg)

        # Also write to log file if specified
        if self.log_file_handle:
            try:
                self.log_file_handle.write(timestamped_msg + "\n")
                self.log_file_handle.flush()
            except Exception as e:
                print(f"⚠️  Warning: Could not write to log file: {e}")

    def _log_clangd_stderr(self, line: str):
        """Log clangd stderr output with special handling"""
        if self.log_file_handle:
            try:
                timestamp = time.strftime('%H:%M:%S')
                self.log_file_handle.write(
                    f"[{timestamp}] [CLANGD_STDERR] {line}\n")
                self.log_file_handle.flush()
            except Exception as e:
                print(f"⚠️  Warning: Could not write stderr to log file: {e}")

    def _log_lsp_message(self, message: Dict[str, Any], direction: str):
        """Log LSP messages (JSON-RPC) to file if logging is enabled"""
        if self.log_file_handle:
            try:
                timestamp = time.strftime('%H:%M:%S')
                json_str = json.dumps(message, indent=2)
                self.log_file_handle.write(
                    f"[{timestamp}] [LSP_{direction}] {json_str}\n"
                )
                self.log_file_handle.flush()
            except Exception as e:
                print(f"⚠️  Warning: Could not write LSP message to log: {e}")

    def _print_verbose(self, message: str):
        """Print message only if in verbose mode"""
        if self.verbose:
            print(message)

    def _mark_file_as_processed(self, file_path_str: str, activity: str = ""):
        """Mark a file as processed if it's in compile_commands.json"""
        try:
            file_path = Path(file_path_str)
            filename = file_path.name
            
            # Check if this file is in our compile_commands.json
            if filename in self.compile_commands_files_by_name:
                # Find the matching full path from compile_commands.json
                matching_path = None
                for cc_path in self.compile_commands_files:
                    if cc_path.name == filename:
                        matching_path = cc_path
                        break
                
                if matching_path and matching_path not in self.processed_compile_files:
                    self.processed_compile_files.add(matching_path)
                    self.current_processing_file = filename
                    self.last_indexing_activity = time.time()
                    
                    if activity and self.verbose:
                        self._print_verbose(f"📝 Processing {filename}: {activity}")
                    
                    return True
        except Exception:
            pass
        return False

    def _print_progress(self, update_in_place: bool = False):
        """Display clean progress indicator"""
        if not self.compile_commands_files:
            return
        
        processed_count = len(self.processed_compile_files)
        total_count = len(self.compile_commands_files)
        percentage = (processed_count / total_count) * 100 if total_count > 0 else 0
        
        progress_msg = f"Processing: {processed_count}/{total_count} files ({percentage:.1f}%)"
        if self.current_processing_file:
            progress_msg += f" [{self.current_processing_file}]"
        
        if update_in_place and not self.verbose:
            print(f"\r{progress_msg}", end='', flush=True)
        else:
            print(progress_msg)

    def clean_cache(self):
        """Clean clangd cache directories to ensure fresh indexing (only if refresh_index is True)"""
        if not self.refresh_index:
            print("🔄 Skipping cache cleaning (use --refresh-index to clean cache)")
            return

        cache_locations = [
            # Common clangd cache locations
            Path.home() / ".cache" / "clangd",
            Path.home() / ".clangd",
            self.build_directory / ".clangd",
            self.build_directory.parent / ".clangd",
            # Project-local cache directories
            self.build_directory.parent / ".cache" / "clangd",
            self.build_directory / ".cache" / "clangd",
            # XDG cache directory
            Path(os.environ.get("XDG_CACHE_HOME",
                                Path.home() / ".cache")) / "clangd",
        ]

        print("🧹 Cleaning clangd cache directories...")
        cleaned_any = False

        for cache_dir in cache_locations:
            if cache_dir.exists() and cache_dir.is_dir():
                try:
                    print(f"   Removing: {cache_dir}")
                    shutil.rmtree(cache_dir)
                    cleaned_any = True
                except Exception as e:
                    print(f"   ⚠️  Could not remove {cache_dir}: {e}")

        if not cleaned_any:
            print("   No cache directories found to clean")
        else:
            print("✅ Cache cleaning completed")
        print()

    def start_clangd(self):
        """Start clangd with the same arguments VS Code uses"""
        compile_commands = self.build_directory / "compile_commands.json"
        if not compile_commands.exists():
            raise FileNotFoundError(
                f"No compile_commands.json found in {self.build_directory}")

        # Use same arguments as VS Code (from the log analysis)
        args = [
            self.clangd_path,
            "--background-index",  # Fixed: not --background-index=true
            "--clang-tidy",
            "--completion-style=detailed",
            "--log=verbose",  # To see indexing messages
            "--query-driver=**",  # Allow querying all drivers for cross-compilation
            # Pass build directory to clangd
            f"--compile-commands-dir={self.build_directory}"
        ]

        print(f"Starting clangd with args: {' '.join(args)}")
        print(f"Working directory: {os.getcwd()}")
        print(f"Build directory: {self.build_directory}")

        # Run clangd from current working directory, pass build dir as argument
        self.process = subprocess.Popen(
            args,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=0
        )

        # Set up JSON-RPC communication using simple JSON over stdio
        self.stdin_lock = threading.Lock()

        # Start message reader thread
        self.reader_thread = threading.Thread(
            target=self._read_messages, daemon=True)
        self.reader_thread.start()

        # Start stderr reader for clangd logs
        self.stderr_thread = threading.Thread(
            target=self._read_stderr, daemon=True)
        self.stderr_thread.start()

    def _read_stderr(self):
        """Read stderr for clangd log messages including indexing progress"""
        while self.process and self.process.poll() is None:
            try:
                line = self.process.stderr.readline()
                if not line:
                    break
                line = line.strip()
                if line:
                    # Log all stderr to file if logging is enabled
                    self._log_clangd_stderr(line)

                    # Track indexing-related messages with reduced verbosity
                    show_clangd_log = self.verbose and any(keyword in line for keyword in [
                        "Enqueueing", "commands for indexing", "Indexed",
                        "symbols", "backgroundIndexProgress",
                        "Building first preamble", "compilation database",
                        "Broadcasting", "ASTWorker", "Error", "Failed",
                        "error:", "warning:", "fatal error"
                    ])
                    
                    if show_clangd_log:
                        self._print_verbose(f"[CLANGD LOG] {line}")

                    # Track file processing from multiple log patterns
                    file_processed = False
                    
                    if "Indexed " in line and "symbols" in line:
                        # Extract filename from log line like:
                        # "Indexed /path/to/file.cpp (1234 symbols, ...)"
                        try:
                            file_part = line.split("Indexed ")[1].split(" (")[0]
                            filename = Path(file_part).name
                            self.indexed_files.add(filename)
                            
                            # Mark as processed if it's from compile_commands.json
                            if self._mark_file_as_processed(file_part, "indexed with symbols"):
                                file_processed = True
                                if self.verbose:
                                    symbols_part = line.split("(")[1].split(" symbols")[0] if "(" in line else "unknown"
                                    print(f"✅ Indexed {filename} ({symbols_part} symbols)")

                        except Exception:
                            pass  # Ignore parsing errors
                    
                    elif "Building first preamble for " in line:
                        # Extract filename from preamble build message
                        # Format: "Building first preamble for /path/to/file.cpp version N"
                        try:
                            after_for = line.split("Building first preamble for ")[1]
                            file_part = after_for.split(" version ")[0].strip()
                            if self._mark_file_as_processed(file_part, "building preamble"):
                                file_processed = True
                        except Exception:
                            pass
                    
                    elif "ASTWorker building file " in line:
                        # Extract filename from ASTWorker build messages
                        # Format: "ASTWorker building file /path/to/file.cpp version N with command ..."
                        try:
                            after_file = line.split("ASTWorker building file ")[1]
                            file_part = after_file.split(" version ")[0].strip()
                            if self._mark_file_as_processed(file_part, "ASTWorker building"):
                                file_processed = True
                        except Exception:
                            pass
                    
                    # Update progress display if a compile_commands.json file was processed
                    if file_processed and not self.verbose:
                        self._print_progress(update_in_place=True)

                    # Track indexing failures
                    elif any(error_indicator in line.lower() for error_indicator in [
                        "error:", "fatal error", "failed to", "could not", "cannot"
                    ]):
                        # Extract potential filename from error messages
                        try:
                            # Look for patterns like "file.cpp:line:col: error"
                            if ".cpp:" in line or ".cc:" in line or ".cxx:" in line:
                                for ext in [".cpp:", ".cc:", ".cxx:"]:
                                    if ext in line:
                                        file_part = line.split(ext)[0]
                                        filename_with_ext = file_part.split("/")[-1] + ext[:-1]
                                        if filename_with_ext in self.compile_commands_files_by_name:
                                            if filename_with_ext not in self.files_with_errors:
                                                self.files_with_errors[filename_with_ext] = 0
                                            self.files_with_errors[filename_with_ext] += 1
                                            error_msg = line.split('error:')[-1].strip()
                                            self._print_verbose(f"❌ Error in {filename_with_ext}: {error_msg}")
                                        break
                            else:
                                self._print_verbose(f"❌ General indexing error: {line}")
                        except Exception:
                            self._print_verbose(f"❌ Parse error in log: {line}")

                    # Also track from symbol slab messages
                    elif "symbol slab:" in line and "symbols" in line:
                        # These indicate files being processed
                        symbols_count = line.split("symbol slab:")[1].split("symbols")[0].strip()
                        if symbols_count.isdigit() and int(symbols_count) > 0:
                            self._print_verbose(f"📊 Processing symbols: {symbols_count} symbols indexed")
                            self.last_indexing_activity = time.time()

                    # Check for indexing completion signals
                    elif "backgroundIndexProgress" in line and "end" in line:
                        self._print_verbose("🎯 Background indexing progress ended")
                        self.indexing_complete = True

                    elif "ASTWorker" in line and ("idle" in line.lower() or "finished" in line.lower()):
                        self._print_verbose("🔄 ASTWorker activity completed")

            except Exception as e:
                print(f"Error reading stderr: {e}")
                break

    def _read_messages(self):
        """Read JSON-RPC messages from clangd using LSP protocol"""
        while self.process and self.process.poll() is None:
            try:
                # Read the Content-Length header
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        return
                    line = line.strip()
                    if line.startswith('Content-Length:'):
                        content_length = int(line.split(':')[1].strip())
                        break
                    elif line == '':
                        # Empty line after headers
                        break

                # Read any remaining header lines until empty line
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        return
                    line = line.strip()
                    if line == '':
                        break

                # Read the JSON content
                if 'content_length' in locals():
                    json_data = self.process.stdout.read(content_length)
                    if json_data:
                        try:
                            message = json.loads(json_data)
                            self._handle_message(message)
                        except json.JSONDecodeError as e:
                            print(f"JSON decode error: {e}")
                            print(f"Raw data: {json_data}")

            except Exception as e:
                print(f"Error reading message: {e}")
                break

    def _handle_message(self, message: Dict[str, Any]):
        """Handle incoming messages from clangd"""
        # Log incoming message if logging is enabled
        self._log_lsp_message(message, "INCOMING")

        method = message.get("method", "")

        # Debug: print all methods we receive in verbose mode
        if method:
            self._print_verbose(f"🔍 Received method: {method}")

        if method == "window/workDoneProgress/create":
            # clangd is requesting to create a progress token
            token = message.get("params", {}).get("token", "")
            self._print_verbose(f"🔄 Progress token created: {token}")
            # Send success response
            self._send_response(message.get("id"), None)

        elif method == "$/progress":
            # Progress update from clangd
            params = message.get("params", {})
            token = params.get("token", "")
            value = params.get("value", {})

            if token == "backgroundIndexProgress":
                kind = value.get("kind", "")
                if kind == "begin":
                    title = value.get("title", "")
                    percentage = value.get("percentage", 0)
                    print(f"🚀 Indexing started: {title}")
                    if self.verbose:
                        print(f"   Initial progress: {percentage}%")
                elif kind == "report":
                    message_text = value.get("message", "")
                    percentage = value.get("percentage", 0)
                    self._print_verbose(f"📊 Indexing progress: {message_text} ({percentage}%)")
                    self.last_indexing_activity = time.time()
                elif kind == "end":
                    if not self.verbose:
                        print()  # New line to finish the progress indicator
                    print("✅ Background indexing completed!")
                    self.indexing_complete = True

        elif method == "textDocument/clangd.fileStatus":
            # File status updates
            params = message.get("params", {})
            uri = params.get("uri", "")
            state = params.get("state", "")
            filename = Path(uri.replace("file://", "")).name
            self._print_verbose(f"📄 File status: {filename} - {state}")

        elif method == "textDocument/publishDiagnostics":
            # Diagnostic messages (errors, warnings, etc.)
            params = message.get("params", {})
            uri = params.get("uri", "")
            diagnostics = params.get("diagnostics", [])
            filename = Path(uri.replace("file://", "")).name

            if diagnostics:
                # Track errors and warnings separately
                errors = [d for d in diagnostics if d.get('severity') == 1]
                warnings = [d for d in diagnostics if d.get('severity') == 2]

                # Track diagnostics for failure reporting
                if filename in self.compile_commands_files_by_name:
                    if errors:
                        self.diagnostic_errors += len(errors)
                        if filename not in self.files_with_errors:
                            self.files_with_errors[filename] = 0
                        self.files_with_errors[filename] += len(errors)
                        self._print_verbose(f"❌ {filename}: {len(errors)} error(s)")
                    if warnings:
                        self.diagnostic_warnings += len(warnings)
                        self._print_verbose(f"⚠️  {filename}: {len(warnings)} warning(s)")
                else:
                    self._print_verbose(f"🔍 Diagnostics for {filename}: {len(diagnostics)} issues")
            # Don't print "Unknown method" for this standard LSP method

        elif "result" in message:
            # Response to our request
            request_id = message.get("id")
            if request_id:
                self._print_verbose(f"✅ Response to request {request_id}")

        elif "error" in message:
            # Error response to our request
            request_id = message.get("id")
            error = message.get("error", {})
            error_code = error.get("code", "")
            error_message = error.get("message", "")
            if error_code == -32601:  # Method not found
                self._print_verbose(f"⚠️  Method not supported by clangd (request {request_id})")
            else:
                print(f"❌ Error response to request {request_id}: {error_message}")
                # Track LSP errors as potential indexing failures
                self.lsp_errors += 1
        else:
            # Debug: print unknown messages
            if method:
                self._print_verbose(f"❓ Unknown method: {method}")
            elif "result" not in message and "error" not in message:
                self._print_verbose(f"❓ Unknown message: {message}")

    def _send_json_rpc(self, message: Dict[str, Any]):
        """Send a JSON-RPC message to clangd using LSP format"""
        # Log outgoing message if logging is enabled
        self._log_lsp_message(message, "OUTGOING")

        json_text = json.dumps(message)
        content = f"Content-Length: {len(json_text)}\r\n\r\n{json_text}"

        with self.stdin_lock:
            try:
                self.process.stdin.write(content)
                self.process.stdin.flush()
            except Exception as e:
                print(f"Error writing to clangd: {e}")

    def _send_response(self, request_id: Any, result: Any):
        """Send a response to clangd"""
        response = {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result
        }
        self._send_json_rpc(response)

    def _send_request(self, method: str, params: Any = None) -> int:
        """Send a request to clangd"""
        self.request_id += 1
        request = {
            "jsonrpc": "2.0",
            "id": self.request_id,
            "method": method,
            "params": params or {}
        }
        self._print_verbose(f"📤 Sending {method} request (id: {self.request_id})")
        self._send_json_rpc(request)
        return self.request_id

    def _send_notification(self, method: str, params: Any = None):
        """Send a notification to clangd"""
        notification = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {}
        }
        self._print_verbose(f"📤 Sending {method} notification")
        self._send_json_rpc(notification)

    def initialize_lsp(self):
        """Initialize clangd with comprehensive capabilities for AI agent use"""
        # Comprehensive capabilities for AI agents that work like humans
        init_params = {
            "processId": os.getpid(),
            "capabilities": {
                # INDEXING-CRITICAL CAPABILITIES
                "workspace": {
                    "workspaceFolders": True,  # Essential for multi-folder projects
                    "workDoneProgress": True,  # Essential for indexing progress
                    "configuration": True,     # May affect indexing behavior
                    "didChangeConfiguration": {
                        "dynamicRegistration": True
                    },
                    "didChangeWatchedFiles": {  # AI needs to know when files change
                        "dynamicRegistration": True
                    },
                    "symbol": {  # Workspace-wide symbol search - very useful for AI
                        "dynamicRegistration": True
                    },
                    "executeCommand": {  # AI may want to trigger clangd commands
                        "dynamicRegistration": True
                    }
                },
                "window": {
                    "workDoneProgress": True,   # Essential for indexing progress
                    "showMessage": {  # AI should see clangd messages/warnings
                        "messageActionItem": {
                            "additionalPropertiesSupport": True
                        }
                    }
                },

                # COMPREHENSIVE AI CAPABILITIES (Human-like interaction)
                "textDocument": {
                    # Code navigation - essential for AI
                    "definition": {
                        "linkSupport": True
                    },
                    "declaration": {
                        "linkSupport": True
                    },
                    "references": {
                        "context": True
                    },
                    "implementation": {
                        "linkSupport": True
                    },

                    # Code understanding - essential for AI
                    "hover": {
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "documentSymbol": {
                        "dynamicRegistration": True,
                        "hierarchicalDocumentSymbolSupport": True
                    },
                    "completion": {
                        "completionItem": {
                            "documentationFormat": ["markdown", "plaintext"],
                            "snippetSupport": True,  # AI can understand snippets
                            "commitCharactersSupport": True,
                            "resolveSupport": {
                                "properties": ["documentation", "detail"]
                            }
                        }
                    },

                    # Code quality - AI needs to see what humans see
                    "diagnostic": {
                        "dynamicRegistration": True,
                        "relatedDocumentSupport": True
                    },
                    "inlayHint": {  # Type hints, parameter names - useful for AI
                        "dynamicRegistration": True,
                        "resolveSupport": {
                            "properties": ["tooltip", "textEdits"]
                        }
                    },

                    # Code editing - AI edits code like humans
                    "codeAction": {
                        "codeActionLiteralSupport": {
                            "codeActionKind": {
                                "valueSet": ["quickfix", "refactor", "source"]
                            }
                        },
                        "resolveSupport": {
                            "properties": ["edit"]
                        }
                    },
                    "rename": {
                        "prepareSupport": True
                    },
                    "formatting": {
                        "dynamicRegistration": True
                    },
                    "rangeFormatting": {
                        "dynamicRegistration": True
                    },

                    # Code structure understanding
                    "foldingRange": {  # AI can understand code structure
                        "dynamicRegistration": True,
                        "rangeLimit": 5000
                    },
                    "selectionRange": {  # Smart selection for AI
                        "dynamicRegistration": True
                    },

                    # Enhanced code understanding
                    "semanticTokens": {  # AI can understand syntax roles
                        "dynamicRegistration": True,
                        "requests": {
                            "range": True,
                            "full": {
                                "delta": True
                            }
                        }
                    },
                    "documentHighlight": {  # AI can see related symbols
                        "dynamicRegistration": True
                    },
                    "documentLink": {  # AI can follow links in comments/docs
                        "dynamicRegistration": True,
                        "tooltipSupport": True
                    },

                    # Interactive features AI can use
                    "signatureHelp": {  # Function parameter hints
                        "signatureInformation": {
                            "documentationFormat": ["markdown", "plaintext"],
                            "parameterInformation": {
                                "labelOffsetSupport": True
                            }
                        }
                    },
                    "codeLens": {  # Inline references/implementations count
                        "dynamicRegistration": True
                    },
                    "callHierarchy": {  # Call relationships
                        "dynamicRegistration": True
                    }

                    # ONLY REMOVED: Capabilities that are truly not useful for AI
                    # - colorProvider (color swatches in UI)
                    # - onTypeFormatting (format-as-you-type)
                }
            },
            "initializationOptions": {
                "clangdFileStatus": True,  # Important for tracking file states
                "fallbackFlags": ["-std=c++20"]
            },
            "trace": "off"
        }

        print("🔧 Initializing LSP with comprehensive AI capabilities...")
        self._send_request("initialize", init_params)
        time.sleep(1)  # Wait for initialize response

        print("✅ Sending initialized notification...")
        self._send_notification("initialized")
        time.sleep(0.5)

    def trigger_indexing_by_opening_file(self):
        """
        The key insight: VS Code triggers indexing by opening a file!
        This is what actually starts the background indexing process.
        """
        # Get the first file from compile_commands.json - no need for folder introspection
        compile_commands = self.build_directory / "compile_commands.json"

        try:
            with open(compile_commands, 'r') as f:
                commands = json.load(f)

            if not commands:
                print("⚠️  No files in compile_commands.json. Cannot trigger indexing.")
                return

            # Use the first file from compile_commands.json
            cpp_file = Path(commands[0]['file'])
            print(f"📂 Opening file to trigger indexing: {cpp_file}")

            with open(cpp_file, 'r', encoding='utf-8') as f:
                content = f.read()
        except Exception as e:
            print(f"❌ Could not read file from compile_commands.json: {e}")
            return

        # Send textDocument/didOpen - this is the trigger!
        did_open_params = {
            "textDocument": {
                "uri": f"file://{cpp_file}",
                "languageId": "cpp",
                "version": 1,
                "text": content
            }
        }

        print("🚀 Sending textDocument/didOpen - this should trigger indexing!")
        self._send_notification("textDocument/didOpen", did_open_params)

        # Also send some requests that VS Code typically sends
        time.sleep(0.1)

        doc_uri = {"uri": f"file://{cpp_file}"}

        # Request document symbols (this works with clangd)
        self._send_request("textDocument/documentSymbol",
                           {"textDocument": doc_uri})

        # Note: Don't request foldingRange as clangd doesn't support it

    def open_file_for_indexing(self, file_path: Path):
        """Open a specific file to trigger its indexing"""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()
        except Exception as e:
            self._print_verbose(f"❌ Could not read file {file_path}: {e}")
            return False

        # Send textDocument/didOpen
        did_open_params = {
            "textDocument": {
                "uri": f"file://{file_path}",
                "languageId": "cpp",
                "version": 1,
                "text": content
            }
        }

        self._print_verbose(f"📂 Opening file to trigger indexing: {file_path.name}")
        self._send_notification("textDocument/didOpen", did_open_params)
        return True

    def ensure_all_files_indexed(self):
        """Open every file from compile_commands.json to ensure complete indexing"""
        if not self.compile_commands_files:
            return True

        # Track which files we've tried to open
        files_opened = set()
        
        print(f"📂 Opening all {len(self.compile_commands_files)} files from compile_commands.json to ensure complete indexing...")
        
        for i, file_path in enumerate(sorted(self.compile_commands_files)):
            progress = f"({i+1}/{len(self.compile_commands_files)})"
            
            if self.verbose:
                status = "✅ already processed" if file_path in self.processed_compile_files else "🔄 needs processing"
                print(f"   {progress} Opening {file_path.name} {status}")
            else:
                print(f"   {progress} Opening {file_path.name}...")
            
            # Always open the file, regardless of processing status
            if self.open_file_for_indexing(file_path):
                files_opened.add(file_path)
                
                # Give clangd a moment to process
                time.sleep(0.2)
                
                # Update activity timestamp
                self.last_indexing_activity = time.time()
            else:
                self._print_verbose(f"❌ Failed to open {file_path.name}")
        
        # Wait a bit for final processing
        if files_opened:
            if not self.verbose:
                print(f"   Waiting for final processing...")
            
            final_wait_start = time.time()
            max_final_wait = 10  # 10 seconds for final processing
            
            while time.time() - final_wait_start < max_final_wait:
                time.sleep(0.5)
                
                # Check if there's still indexing activity
                if time.time() - self.last_indexing_activity < 3:
                    # Reset wait timer if there's still activity
                    final_wait_start = time.time()
                    continue
                else:
                    # No recent activity, probably done
                    break
        
        # Final status
        processed_count = len(self.processed_compile_files)
        total_count = len(self.compile_commands_files)
        
        if processed_count == total_count:
            print(f"✅ All {total_count} files have been opened and processed by clangd!")
            return True
        elif processed_count > 0:
            print(f"⚠️  {processed_count}/{total_count} files were processed after opening")
            unprocessed = []
            for file_path in self.compile_commands_files:
                if file_path not in self.processed_compile_files:
                    unprocessed.append(file_path.name)
            
            if self.verbose or len(unprocessed) <= 10:
                print(f"   Unprocessed files: {', '.join(sorted(unprocessed))}")
            else:
                sample = sorted(unprocessed)[:5]
                print(f"   Unprocessed files: {', '.join(sample)} ... and {len(unprocessed) - 5} more")
            
            return False
        else:
            print(f"⚠️  No files were detected as processed by clangd")
            print(f"   This could indicate clangd logging issues or compilation database problems")
            return False

    def wait_for_indexing_completion(self):
        """Wait for clangd to finish indexing (no timeout)"""
        print("👀 Waiting for indexing to complete...")
        print("   Use Ctrl+C to interrupt if needed")
        print()

        # Wait for completion signals or process end
        start_time = time.time()
        last_progress_time = time.time()

        while self.process and self.process.poll() is None:
            time.sleep(1)
            current_time = time.time()

            # Primary completion signal: LSP progress "end"
            if self.indexing_complete:
                print("🎯 Primary signal: LSP progress indicates indexing complete")
                # Give a bit more time for final file indexing messages
                time.sleep(2)
                break

            # Fallback 1: No indexing activity for extended period
            if (self.indexed_files and
                    current_time - self.last_indexing_activity > 45):
                print("🔄 No indexing activity for 45 seconds, assuming completion")
                break

            # Fallback 2: Reasonable time limit (10 minutes max)
            if current_time - start_time > 600:
                print("⏰ Maximum indexing time reached (10 minutes), stopping")
                break

        # Determine completion status
        if self.indexing_complete:
            print("✅ Initial indexing completed successfully!")
            # Now check coverage and ensure all files are indexed
            print("\n🔍 Checking compile commands coverage...")
            if not self.ensure_all_files_indexed():
                print("⚠️  Not all files could be indexed completely")
        elif self.process and self.process.poll() is not None:
            print("⚠️  clangd process ended")
        else:
            print("🔄 Indexing monitoring stopped")

        # Final summary with detailed analysis
        if self.compile_commands_files:
            processed_from_compile = len(self.processed_compile_files)
            total_compile = len(self.compile_commands_files)
            final_percentage = (processed_from_compile /
                                total_compile) * 100 if total_compile > 0 else 0
            
            if not self.verbose:
                print()  # Ensure we end the progress line
            
            print(f"\n📊 FINAL SUMMARY")
            print("=" * 40)
            print(f"📋 Files in compile_commands.json: {total_compile}")
            print(f"✅ Files processed by clangd: {processed_from_compile}")
            print(f"📊 Coverage: {final_percentage:.1f}%")

            # Show which files were/weren't processed only in verbose mode or if there are missing files
            if processed_from_compile < total_compile:
                missing_files = set()
                for file_path in self.compile_commands_files:
                    if file_path not in self.processed_compile_files:
                        missing_files.add(file_path.name)
                
                print(f"❓ Files not processed: {len(missing_files)}")
                if self.verbose or len(missing_files) <= 5:
                    for filename in sorted(missing_files):
                        print(f"   - {filename}")
                elif len(missing_files) > 5:
                    for filename in sorted(list(missing_files)[:3]):
                        print(f"   - {filename}")
                    print(f"   ... and {len(missing_files) - 3} more (use --verbose to see all)")

            # Also show total indexed files (including headers) in verbose mode
            if self.verbose:
                print(f"\n📁 Total files indexed (including headers): {len(self.indexed_files)}")
                if len(self.indexed_files) <= 20:
                    print(f"   Files: {', '.join(sorted(self.indexed_files))}")
                else:
                    sample_files = sorted(list(self.indexed_files))[:10]
                    print(f"   Sample: {', '.join(sample_files)} ... and {len(self.indexed_files) - 10} more")

        # Additional diagnostics only if no files were indexed at all
        if not self.indexed_files and self.verbose:
            print("⚠️  Warning: No individual file indexing detected in logs")
            print("   This might indicate:")
            print("   - Files were indexed but not logged with expected format")
            print("   - Indexing completed too quickly to capture")
            print("   - clangd used cached index data")

        # Report indexing failures and issues only if there are errors or in verbose mode
        has_issues = (self.files_with_errors or self.diagnostic_errors > 0 or 
                     self.diagnostic_warnings > 0 or self.lsp_errors > 0)
        
        if has_issues or self.verbose:
            print("\n📊 INDEXING ISSUES SUMMARY")
            print("=" * 40)

            if self.files_with_errors:
                print(f"❌ Files with errors: {len(self.files_with_errors)}")
                if self.verbose:
                    for filename, error_count in sorted(self.files_with_errors.items()):
                        print(f"   • {filename}: {error_count} error(s)")
                elif len(self.files_with_errors) <= 3:
                    for filename, error_count in sorted(self.files_with_errors.items()):
                        print(f"   • {filename}: {error_count} error(s)")
                else:
                    # Show first 3 files with errors
                    for filename, error_count in sorted(list(self.files_with_errors.items())[:3]):
                        print(f"   • {filename}: {error_count} error(s)")
                    print(f"   ... and {len(self.files_with_errors) - 3} more (use --verbose for details)")
            else:
                print("✅ No files with compile errors detected")

            if self.diagnostic_errors > 0:
                print(f"❌ Total diagnostic errors: {self.diagnostic_errors}")
            elif self.verbose:
                print("✅ No diagnostic errors reported")

            if self.diagnostic_warnings > 0:
                print(f"⚠️  Total diagnostic warnings: {self.diagnostic_warnings}")
            elif self.verbose:
                print("✅ No diagnostic warnings reported")

            if self.lsp_errors > 0:
                print(f"❌ LSP protocol errors: {self.lsp_errors}")
            elif self.verbose:
                print("✅ No LSP protocol errors")

            # Calculate success rate
            failed_files = set(self.files_with_errors.keys())
            successful_files = self.indexed_files - failed_files

            if self.compile_commands_files and self.verbose:
                success_rate = (len(successful_files & self.compile_commands_files_by_name) /
                                len(self.compile_commands_files)) * 100
                print(f"\n🎯 Overall success rate: {success_rate:.1f}% "
                      f"({len(successful_files & self.compile_commands_files_by_name)}/"
                      f"{len(self.compile_commands_files)} files)")

            print("=" * 40)

    def shutdown(self):
        """Shutdown clangd gracefully"""
        if self.writer:
            print("🛑 Shutting down clangd...")
            self._send_request("shutdown")
            time.sleep(1)
            self._send_notification("exit")

        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()

        # Close log file if it was opened
        if self.log_file_handle:
            try:
                self.log_file_handle.close()
                print(f"📝 Clangd log saved to: {self.log_file}")
            except Exception as e:
                print(f"⚠️  Warning: Error closing log file: {e}")

    def load_compile_commands_info(self):
        """Load files from compile_commands.json for information and reporting"""
        compile_commands = self.build_directory / "compile_commands.json"
        if not compile_commands.exists():
            raise FileNotFoundError(
                f"No compile_commands.json found in {self.build_directory}")

        try:
            with open(compile_commands, 'r') as f:
                commands = json.load(f)

            for cmd in commands:
                if 'file' in cmd:
                    file_path = Path(cmd['file'])
                    self.compile_commands_files.add(file_path)  # Store full Path object
                    self.compile_commands_files_by_name.add(file_path.name)  # Store filename for backward compatibility

            total_files = len(self.compile_commands_files)
            print(f"📋 Found {total_files} files in compile_commands.json")
            if self.verbose:
                print(f"   Files: {', '.join(sorted(self.compile_commands_files_by_name))}")
            else:
                print(f"   Use --verbose to see file list")

        except Exception as e:
            print(f"❌ Error reading compile_commands.json: {e}")
            raise


def main():
    parser = argparse.ArgumentParser(description="Generate clangd index")
    parser.add_argument("build_directory",
                        help="Build directory with compile_commands.json")
    parser.add_argument("--refresh-index", action="store_true",
                        help="Clean cache before indexing")
    parser.add_argument("--clangd-path", default="clangd",
                        help="Path to clangd executable (default: "
                        "CLANGD_PATH env var, then /usr/bin/clangd)")
    parser.add_argument("--log-file",
                        help="Save all clangd logs to specified file for "
                        "investigation (optional)")
    parser.add_argument("--verbose", action="store_true",
                        help="Show detailed clangd logs and progress messages")

    args = parser.parse_args()

    if not Path(args.build_directory).exists():
        print(f"Error: Build directory {args.build_directory} does not exist")
        sys.exit(1)

    # Determine clangd path with priority order
    clangd_path = None

    # 1. Command-line argument (highest priority)
    if args.clangd_path != "clangd":  # Only if user explicitly provided a path
        clangd_path = args.clangd_path
        print(f"Using clangd from command line: {clangd_path}")

    # 2. Environment variable
    elif "CLANGD_PATH" in os.environ:
        clangd_path = os.environ["CLANGD_PATH"]
        print(f"Using clangd from CLANGD_PATH env var: {clangd_path}")

    # 3. Fallback to /usr/bin/clangd
    elif Path("/usr/bin/clangd").exists():
        clangd_path = "/usr/bin/clangd"
        print(f"Using fallback clangd: {clangd_path}")

    # 4. Error if none found
    else:
        print("Error: clangd not found.")
        print("Please either:")
        print("  1. Use --clangd-path to specify the path")
        print("  2. Set CLANGD_PATH environment variable")
        print("  3. Install clangd at /usr/bin/clangd")
        sys.exit(1)

    # Verify the clangd path works
    try:
        subprocess.run([clangd_path, "--version"],
                       capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        print(f"Error: clangd not found or not executable at '{clangd_path}'")
        sys.exit(1)

    generator = ClangdIndexGenerator(
        args.build_directory, clangd_path, args.refresh_index, args.log_file, args.verbose)

    try:
        print("=" * 60)
        print("🎯 clangd Index Generator")
        print("=" * 60)

        generator.load_compile_commands_info()  # Load files for information
        generator.clean_cache()  # Clean cache only if --refresh-index
        generator.start_clangd()
        time.sleep(1)

        generator.initialize_lsp()
        time.sleep(2)

        generator.trigger_indexing_by_opening_file()

        # Wait for indexing to complete (no timeout)
        generator.wait_for_indexing_completion()

    except KeyboardInterrupt:
        print("\n🛑 Interrupted by user")
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        generator.shutdown()
        print("👋 Done!")


if __name__ == "__main__":
    main()
