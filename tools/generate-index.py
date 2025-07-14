#!/usr/bin/env python3
"""
Script to generate clangd index by mimicking VS Code's initialization sequence.
Automatically exits when clangd finishes indexing.
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
    def __init__(self, build_directory: str, clangd_path: str = "clangd", refresh_index: bool = False):
        self.build_directory = Path(build_directory).resolve()
        self.clangd_path = clangd_path
        self.refresh_index = refresh_index
        self.process = None
        self.reader = None
        self.writer = None
        self.request_id = 0
        self.indexing_progress = {}
        self.indexed_files = set()
        # Files from compile_commands.json for reporting
        self.compile_commands_files = set()
        self.failed_files = set()  # Files that failed to index
        self.files_with_errors = {}  # filename -> error count
        self.indexing_complete = False
        self.last_indexing_activity = time.time()
        self.diagnostic_errors = 0  # Total count of diagnostic errors
        self.diagnostic_warnings = 0  # Total count of diagnostic warnings
        self.lsp_errors = 0  # Count of LSP protocol errors

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
            "--log=verbose"  # To see indexing messages
        ]

        print(f"Starting clangd with args: {' '.join(args)}")
        print(f"Working directory: {self.build_directory}")

        self.process = subprocess.Popen(
            args,
            cwd=self.build_directory,
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
                    # Look for indexing-related messages and errors
                    if any(keyword in line for keyword in [
                        "Enqueueing", "commands for indexing", "Indexed",
                        "symbols", "backgroundIndexProgress",
                        "Building first preamble", "compilation database",
                        "Broadcasting", "ASTWorker", "Error", "Failed",
                        "error:", "warning:", "fatal error"
                    ]):
                        print(f"[CLANGD LOG] {line}")

                        # Track indexed files from various log patterns
                        if "Indexed " in line and "symbols" in line:
                            # Extract filename from log line like:
                            # "Indexed /path/to/file.cpp (1234 symbols, ...)"
                            try:
                                file_part = line.split(
                                    "Indexed ")[1].split(" (")[0]
                                filename = Path(file_part).name
                                self.indexed_files.add(filename)
                                self.last_indexing_activity = time.time()

                                # Calculate progress based on compile_commands.json files
                                compile_commands_indexed = len(
                                    self.indexed_files & self.compile_commands_files
                                )
                                total_compile_commands = len(
                                    self.compile_commands_files)

                                if total_compile_commands > 0:
                                    percentage = (
                                        compile_commands_indexed / total_compile_commands) * 100
                                    print(f"✅ Indexed {filename} "
                                          f"({compile_commands_indexed}/{total_compile_commands} = {percentage:.1f}% of compile_commands.json files)")
                                else:
                                    print(f"✅ Indexed {filename}")

                            except Exception:
                                pass  # Ignore parsing errors

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
                                            filename_with_ext = file_part.split(
                                                "/")[-1] + ext[:-1]
                                            if filename_with_ext in self.compile_commands_files:
                                                if filename_with_ext not in self.files_with_errors:
                                                    self.files_with_errors[filename_with_ext] = 0
                                                self.files_with_errors[filename_with_ext] += 1
                                                print(
                                                    f"❌ Error in {filename_with_ext}: {line.split('error:')[-1].strip()}")
                                            break
                                else:
                                    print(f"❌ General indexing error: {line}")
                            except Exception:
                                print(f"❌ Parse error in log: {line}")

                        # Also track from symbol slab messages
                        elif "symbol slab:" in line and "symbols" in line:
                            # These indicate files being processed
                            symbols_count = line.split("symbol slab:")[
                                1].split("symbols")[0].strip()
                            if symbols_count.isdigit() and int(symbols_count) > 0:
                                print(
                                    f"📊 Processing symbols: {symbols_count} symbols indexed")
                                self.last_indexing_activity = time.time()

                        # Check for indexing completion signals
                        elif "backgroundIndexProgress" in line and "end" in line:
                            print("🎯 Background indexing progress ended")
                            self.indexing_complete = True

                        elif "ASTWorker" in line and ("idle" in line.lower() or "finished" in line.lower()):
                            print("🔄 ASTWorker activity completed")

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
        method = message.get("method", "")

        # Debug: print all methods we receive
        if method:
            print(f"🔍 Received method: {method}")

        if method == "window/workDoneProgress/create":
            # clangd is requesting to create a progress token
            token = message.get("params", {}).get("token", "")
            print(f"🔄 Progress token created: {token}")
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
                    print(f"🚀 Indexing started: {title} ({percentage}%)")
                elif kind == "report":
                    message_text = value.get("message", "")
                    percentage = value.get("percentage", 0)
                    print(
                        f"📊 Indexing progress: {message_text} ({percentage}%)")
                    self.last_indexing_activity = time.time()
                elif kind == "end":
                    print("✅ Background indexing completed!")
                    self.indexing_complete = True

        elif method == "textDocument/clangd.fileStatus":
            # File status updates
            params = message.get("params", {})
            uri = params.get("uri", "")
            state = params.get("state", "")
            filename = Path(uri.replace("file://", "")).name
            print(f"📄 File status: {filename} - {state}")

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
                if filename in self.compile_commands_files:
                    if errors:
                        self.diagnostic_errors += len(errors)
                        if filename not in self.files_with_errors:
                            self.files_with_errors[filename] = 0
                        self.files_with_errors[filename] += len(errors)
                        print(f"❌ {filename}: {len(errors)} error(s)")
                    if warnings:
                        self.diagnostic_warnings += len(warnings)
                        print(f"⚠️  {filename}: {len(warnings)} warning(s)")
                else:
                    print(f"🔍 Diagnostics for {filename}: "
                          f"{len(diagnostics)} issues")
            # Don't print "Unknown method" for this standard LSP method

        elif "result" in message:
            # Response to our request
            request_id = message.get("id")
            if request_id:
                print(f"✅ Response to request {request_id}")

        elif "error" in message:
            # Error response to our request
            request_id = message.get("id")
            error = message.get("error", {})
            error_code = error.get("code", "")
            error_message = error.get("message", "")
            if error_code == -32601:  # Method not found
                print(
                    f"⚠️  Method not supported by clangd (request {request_id})")
            else:
                print(f"❌ Error response to request {request_id}: "
                      f"{error_message}")
                # Track LSP errors as potential indexing failures
                self.lsp_errors += 1
        else:
            # Debug: print unknown messages
            if method:
                print(f"❓ Unknown method: {method}")
            elif "result" not in message and "error" not in message:
                print(f"❓ Unknown message: {message}")

    def _send_json_rpc(self, message: Dict[str, Any]):
        """Send a JSON-RPC message to clangd using LSP format"""
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
        print(f"📤 Sending {method} request (id: {self.request_id})")
        self._send_json_rpc(request)
        return self.request_id

    def _send_notification(self, method: str, params: Any = None):
        """Send a notification to clangd"""
        notification = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {}
        }
        print(f"📤 Sending {method} notification")
        self._send_json_rpc(notification)

    def initialize_lsp(self):
        """Initialize clangd with comprehensive capabilities for AI agent use"""
        # Comprehensive capabilities for AI agents that work like humans
        init_params = {
            "processId": os.getpid(),
            "rootPath": str(self.build_directory),
            "rootUri": f"file://{self.build_directory}",
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
            "trace": "off",
            "workspaceFolders": [
                {
                    "name": self.build_directory.name,
                    "uri": f"file://{self.build_directory}"
                }
            ]
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
        # Find a C++ file in the build directory or source
        cpp_files = []

        # Look for source files relative to build directory
        source_dirs = [
            self.build_directory.parent / "src",
            self.build_directory.parent / "source",
            self.build_directory / ".." / "src",
            self.build_directory.parent
        ]

        for source_dir in source_dirs:
            if source_dir.exists():
                cpp_files.extend(source_dir.rglob("*.cpp"))
                cpp_files.extend(source_dir.rglob("*.cc"))
                cpp_files.extend(source_dir.rglob("*.cxx"))
                if cpp_files:
                    break

        if not cpp_files:
            print(
                "⚠️  No C++ files found. Indexing may not start without opening a file.")
            return

        # Use the first C++ file found
        cpp_file = cpp_files[0]
        print(f"📂 Opening file to trigger indexing: {cpp_file}")

        try:
            with open(cpp_file, 'r', encoding='utf-8') as f:
                content = f.read()
        except Exception as e:
            print(f"❌ Could not read file {cpp_file}: {e}")
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

            # Secondary completion: All compile_commands.json files indexed
            if (self.compile_commands_files and
                    len(self.indexed_files & self.compile_commands_files) >= len(self.compile_commands_files)):
                print("🎯 Secondary signal: All compile_commands.json files indexed")
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
            print("✅ Indexing completed successfully!")
        elif self.process and self.process.poll() is not None:
            print("⚠️  clangd process ended")
        else:
            print("🔄 Indexing monitoring stopped")

        # Final summary with detailed analysis
        if self.compile_commands_files:
            indexed_from_compile = len(
                self.indexed_files & self.compile_commands_files)
            total_compile = len(self.compile_commands_files)
            final_percentage = (indexed_from_compile /
                                total_compile) * 100 if total_compile > 0 else 0
            print(
                f"📊 Final summary: {indexed_from_compile}/{total_compile} = {final_percentage:.1f}% of compile_commands.json files indexed")

            # Show which files were/weren't indexed
            if indexed_from_compile < total_compile:
                missing_files = self.compile_commands_files - self.indexed_files
                print(
                    f"❓ Files not detected as indexed: {', '.join(sorted(missing_files))}")

        print(f"📁 Total files indexed: {len(self.indexed_files)}")
        if self.indexed_files:
            print(f"   Files: {', '.join(sorted(self.indexed_files))}")
        else:
            print("⚠️  Warning: No individual file indexing detected in logs")
            print("   This might indicate:")
            print("   - Files were indexed but not logged with expected format")
            print("   - Indexing completed too quickly to capture")
            print("   - clangd used cached index data")

        # Report indexing failures and issues
        print("\n" + "="*60)
        print("📊 INDEXING FAILURE SUMMARY")
        print("="*60)

        if self.files_with_errors:
            print(f"❌ Files with errors: {len(self.files_with_errors)}")
            for filename, error_count in sorted(self.files_with_errors.items()):
                print(f"   • {filename}: {error_count} error(s)")
        else:
            print("✅ No files with compile errors detected")

        if self.diagnostic_errors > 0:
            print(f"❌ Total diagnostic errors: {self.diagnostic_errors}")
        else:
            print("✅ No diagnostic errors reported")

        if self.diagnostic_warnings > 0:
            print(f"⚠️  Total diagnostic warnings: {self.diagnostic_warnings}")
        else:
            print("✅ No diagnostic warnings reported")

        if self.lsp_errors > 0:
            print(f"❌ LSP protocol errors: {self.lsp_errors}")
        else:
            print("✅ No LSP protocol errors")

        # Calculate success rate
        failed_files = set(self.files_with_errors.keys())
        successful_files = self.indexed_files - failed_files

        if self.compile_commands_files:
            success_rate = (len(successful_files & self.compile_commands_files) /
                            len(self.compile_commands_files)) * 100
            print(f"\n🎯 Overall success rate: {success_rate:.1f}% "
                  f"({len(successful_files & self.compile_commands_files)}/"
                  f"{len(self.compile_commands_files)} files)")

        print("="*60)

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
                    self.compile_commands_files.add(file_path.name)

            total_files = len(self.compile_commands_files)
            print(f"📋 Found {total_files} files in compile_commands.json")
            print(
                f"   Files: {', '.join(sorted(self.compile_commands_files))}")

        except Exception as e:
            print(f"❌ Error reading compile_commands.json: {e}")
            raise


def main():
    parser = argparse.ArgumentParser(description="Generate clangd index")
    parser.add_argument("build_directory",
                        help="Build directory with compile_commands.json")
    parser.add_argument("--refresh-index", action="store_true",
                        help="Clean cache before indexing")

    args = parser.parse_args()

    if not Path(args.build_directory).exists():
        print(f"Error: Build directory {args.build_directory} does not exist")
        sys.exit(1)

    # Check for clangd
    clangd_path = "clangd"
    try:
        subprocess.run([clangd_path, "--version"],
                       capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("Error: clangd not found. "
              "Please install clangd or set CLANGD_PATH")
        sys.exit(1)

    generator = ClangdIndexGenerator(
        args.build_directory, clangd_path, args.refresh_index)

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
