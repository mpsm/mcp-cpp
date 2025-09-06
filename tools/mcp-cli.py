#!/usr/bin/env python3
"""
MCP CLI Tool - Command line interface for the C++ MCP Server

This tool provides a convenient way to interact with the MCP C++ server
from the command line, supporting all available tools with comprehensive
argument handling and pretty-formatted output.
"""

import argparse
import json
import subprocess
import sys
import os
from pathlib import Path
from typing import Dict, List, Optional, Any, Union
from uuid import uuid4

try:
    from rich.console import Console
    from rich.table import Table
    from rich.panel import Panel
    from rich.syntax import Syntax
    from rich.tree import Tree
    from rich import print as rprint
    RICH_AVAILABLE = True
except ImportError:
    RICH_AVAILABLE = False
    console = None


class McpCliError(Exception):
    """Custom exception for MCP CLI errors"""
    pass


class McpClient:
    """JSON-RPC client for communicating with the MCP server"""
    
    def __init__(self, server_path: str):
        self.server_path = server_path
        self.console = Console() if RICH_AVAILABLE else None
        
    def _validate_server(self) -> None:
        """Validate that the MCP server exists and is executable"""
        if not os.path.exists(self.server_path):
            raise McpCliError(f"MCP server not found at: {self.server_path}")
        if not os.access(self.server_path, os.X_OK):
            raise McpCliError(f"MCP server is not executable: {self.server_path}")
    
    def _send_request(self, method: str, params: Optional[Dict] = None) -> Dict:
        """Send a JSON-RPC request to the MCP server and return the response"""
        self._validate_server()
        
        request = {
            "jsonrpc": "2.0",
            "id": str(uuid4()),
            "method": method
        }
        
        if params:
            request["params"] = params
            
        try:
            # Start the MCP server process
            process = subprocess.Popen(
                [self.server_path],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,  # Discard stderr as requested
                text=True
            )
            
            # Send the request
            request_json = json.dumps(request)
            stdout, _ = process.communicate(input=request_json)
            
            if process.returncode != 0:
                raise McpCliError(f"MCP server exited with code {process.returncode}")
            
            # Parse the response
            try:
                response = json.loads(stdout.strip())
            except json.JSONDecodeError as e:
                raise McpCliError(f"Invalid JSON response from server: {e}")
                
            # Check for JSON-RPC errors
            if "error" in response:
                error = response["error"]
                raise McpCliError(f"Server error ({error.get('code', 'unknown')}): {error.get('message', 'Unknown error')}")
                
            return response
            
        except subprocess.TimeoutExpired:
            raise McpCliError("MCP server timed out")
        except FileNotFoundError:
            raise McpCliError(f"Could not execute MCP server: {self.server_path}")
    
    def list_tools(self) -> Dict:
        """List available tools"""
        return self._send_request("tools/list")
    
    def call_tool(self, name: str, arguments: Dict) -> Dict:
        """Call a specific tool with arguments"""
        params = {
            "name": name,
            "arguments": arguments
        }
        return self._send_request("tools/call", params)


def find_server_binary() -> str:
    """Find the MCP server binary in PATH"""
    import shutil
    if shutil.which("mcp-cpp-server"):
        return "mcp-cpp-server"
    
    raise McpCliError("Could not find mcp-cpp-server binary. Please install it in PATH or specify --server-path")


def create_parser() -> argparse.ArgumentParser:
    """Create the argument parser with all subcommands"""
    parser = argparse.ArgumentParser(
        description="Command line interface for the C++ MCP Server",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s list-tools
  %(prog)s search-symbols Math --max-results 20
  %(prog)s analyze-symbol "Math::sqrt" --max-examples 3
  %(prog)s get-project-details --pretty-json
        """
    )
    
    # Global options
    output_group = parser.add_mutually_exclusive_group()
    output_group.add_argument(
        "--raw-output",
        action="store_true",
        help="Output raw JSON instead of pretty-formatted text"
    )
    output_group.add_argument(
        "--pretty-json",
        action="store_true",
        help="Pretty print the 'text' field of JSON-RPC response as formatted JSON"
    )
    parser.add_argument(
        "--server-path",
        type=str,
        help="Path to the MCP server binary (auto-detected by default)"
    )
    
    # Subcommands
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # list-tools subcommand
    list_tools_parser = subparsers.add_parser(
        "list-tools",
        help="List available MCP tools"
    )
    
    # search-symbols subcommand
    search_parser = subparsers.add_parser(
        "search-symbols",
        help="Search for C++ symbols in the codebase"
    )
    search_parser.add_argument(
        "query",
        nargs="?",
        default="",
        help="Search query (supports fuzzy matching and qualified names). Use empty string \"\" with --files to list all symbols in specified files."
    )
    search_parser.add_argument(
        "--kinds",
        nargs="+",
        help="Filter by symbol types (class, function, method, variable, etc.)"
    )
    search_parser.add_argument(
        "--files",
        nargs="+",
        help="Limit search to specific files"
    )
    search_parser.add_argument(
        "--max-results",
        type=int,
        default=100,
        help="Maximum number of results to return (1-1000, default: 100)"
    )
    search_parser.add_argument(
        "--include-external",
        action="store_true",
        help="Include symbols from external/system libraries"
    )
    search_parser.add_argument(
        "--build-directory",
        type=str,
        help="Specify build directory path"
    )
    search_parser.add_argument(
        "--wait-timeout",
        type=int,
        help="Timeout for waiting on indexing completion in seconds (default: 20, 0 = no wait)"
    )
    
    # analyze-symbol subcommand
    analyze_parser = subparsers.add_parser(
        "analyze-symbol",
        help="Perform comprehensive analysis of a C++ symbol"
    )
    analyze_parser.add_argument(
        "symbol",
        help="Symbol name to analyze (e.g., 'Math', 'std::vector', 'MyClass::method')"
    )
    analyze_parser.add_argument(
        "--max-examples",
        type=int,
        help="Maximum number of usage examples to include (unlimited by default)"
    )
    analyze_parser.add_argument(
        "--build-directory",
        type=str,
        help="Specify build directory path containing compile_commands.json"
    )
    analyze_parser.add_argument(
        "--no-code",
        action="store_true",
        help="Don't extract and display source code snippets"
    )
    analyze_parser.add_argument(
        "--show-all-members",
        action="store_true",
        help="Show all class members instead of just a summary (useful for classes with many members)"
    )
    analyze_parser.add_argument(
        "--location-hint",
        type=str,
        help="Location hint for disambiguating overloaded symbols (format: /path/file.cpp:line:column, 1-based)"
    )
    analyze_parser.add_argument(
        "--wait-timeout",
        type=int,
        help="Timeout for waiting on indexing completion in seconds (default: 20, 0 = no wait)"
    )
    
    # get-project-details subcommand
    project_details_parser = subparsers.add_parser(
        "get-project-details",
        help="Get comprehensive project analysis including build configurations and global compilation database"
    )
    project_details_parser.add_argument(
        "--path",
        type=str,
        help="Project root path to scan (triggers fresh scan if different from server default)"
    )
    project_details_parser.add_argument(
        "--depth",
        type=int,
        choices=range(0, 11),
        metavar="0-10",
        help="Scan depth for component discovery (triggers fresh scan if different from server default)"
    )
    
    return parser


def main():
    """Main entry point"""
    parser = create_parser()
    args = parser.parse_args()
    
    # Show help if no command specified
    if not args.command:
        parser.print_help()
        sys.exit(1)
    
    try:
        # Find server binary
        server_path = args.server_path or find_server_binary()
        client = McpClient(server_path)
        
        # Execute the appropriate command
        if args.command == "list-tools":
            response = client.list_tools()
            
        elif args.command == "search-symbols":
            # Validate empty query only allowed with files parameter
            if args.query == "" and not args.files:
                print("Error: Empty query requires --files parameter for file-specific search", file=sys.stderr)
                sys.exit(1)
                
            arguments = {"query": args.query}
            
            # Add optional parameters
            if args.kinds:
                arguments["kinds"] = args.kinds
            if args.files:
                arguments["files"] = args.files
            if args.max_results != 100:
                arguments["max_results"] = args.max_results
            if args.include_external:
                arguments["include_external"] = args.include_external
            if args.build_directory:
                arguments["build_directory"] = args.build_directory
            if args.wait_timeout is not None:
                arguments["wait_timeout"] = args.wait_timeout
                
            response = client.call_tool("search_symbols", arguments)
            
        elif args.command == "analyze-symbol":
            arguments = {"symbol": args.symbol}
            
            # Add optional parameters
            if args.max_examples is not None:
                arguments["max_examples"] = args.max_examples
            if args.build_directory:
                arguments["build_directory"] = args.build_directory
            if args.location_hint:
                arguments["location_hint"] = args.location_hint
            if args.wait_timeout is not None:
                arguments["wait_timeout"] = args.wait_timeout
                
            response = client.call_tool("analyze_symbol_context", arguments)
            
        elif args.command == "get-project-details":
            arguments = {}
            if hasattr(args, 'path') and args.path:
                arguments["path"] = args.path
            if hasattr(args, 'depth') and args.depth is not None:
                arguments["depth"] = args.depth
            response = client.call_tool("get_project_details", arguments)
        
        # Output the response
        if args.raw_output:
            print(json.dumps(response, indent=2))
        elif args.pretty_json:
            format_pretty_json_output(response)
        else:
            # Pass flags for analyze-symbol command
            show_code = not (args.command == "analyze-symbol" and getattr(args, 'no_code', False))
            show_all_members = args.command == "analyze-symbol" and getattr(args, 'show_all_members', False)
            format_output(args.command, response, show_code=show_code, show_all_members=show_all_members)
            
    except McpCliError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nOperation cancelled", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(f"Unexpected error: {e}", file=sys.stderr)
        sys.exit(1)


def format_output(command: str, response: Dict, show_code: bool = True, show_all_members: bool = False) -> None:
    """Format and display the response in a user-friendly way"""
    if not RICH_AVAILABLE:
        _format_simple_output(response)
    else:
        _format_rich_output(command, response, show_code=show_code, show_all_members=show_all_members)


def format_pretty_json_output(response: Dict) -> None:
    """Pretty print the 'text' field of JSON-RPC response as formatted JSON"""
    if "result" in response and "content" in response["result"]:
        content = response["result"]["content"]
        if content and len(content) > 0 and "text" in content[0]:
            try:
                # Parse the JSON in the text field
                data = json.loads(content[0]["text"])
                # Pretty print it with syntax highlighting if rich is available
                if RICH_AVAILABLE:
                    console = Console()
                    syntax = Syntax(json.dumps(data, indent=2), "json", theme="monokai")
                    console.print(syntax)
                else:
                    print(json.dumps(data, indent=2))
            except json.JSONDecodeError:
                # If text field is not valid JSON, just print it as-is
                print(content[0]["text"])
        else:
            print("No text content found in response")
    else:
        print("Invalid response format: missing result or content")


def _format_simple_output(response: Dict) -> None:
    """Simple text output when rich is not available"""
    # Handle list-tools specially (different response format)
    if "result" in response and "tools" in response["result"]:
        # This is a list-tools response
        print(json.dumps(response["result"], indent=2))
        return
    
    # Handle tool call responses
    if "result" in response and "content" in response["result"]:
        content = response["result"]["content"]
        if content and len(content) > 0 and "text" in content[0]:
            try:
                data = json.loads(content[0]["text"])
                print(json.dumps(data, indent=2))
            except json.JSONDecodeError:
                print(content[0]["text"])
        else:
            print(json.dumps(response, indent=2))
    else:
        print(json.dumps(response, indent=2))


def _format_rich_output(command: str, response: Dict, show_code: bool = True, show_all_members: bool = False) -> None:
    """Rich formatted output with colors and tables"""
    console = Console()
    
    try:
        # Handle list-tools specially (different response format)
        if command == "list-tools":
            if "result" not in response or "tools" not in response["result"]:
                console.print("[red]Invalid response format for list-tools[/red]")
                return
            _format_tools_list(console, response["result"])
            return
        
        # Extract the actual data from MCP response for tool calls
        if "result" not in response or "content" not in response["result"]:
            console.print("[red]Invalid response format[/red]")
            return
            
        content = response["result"]["content"]
        if not content or len(content) == 0 or "text" not in content[0]:
            console.print("[yellow]No content in response[/yellow]")
            return
            
        try:
            data = json.loads(content[0]["text"])
        except json.JSONDecodeError:
            console.print("[red]Invalid JSON in response[/red]")
            console.print(content[0]["text"])
            return
            
        # Format based on command type
        if command == "list-tools":
            _format_tools_list(console, data)
        elif command == "search-symbols":
            _format_symbols_search(console, data)
        elif command == "analyze-symbol":
            _format_symbol_analysis(console, data, show_code=show_code, show_all_members=show_all_members)
        elif command == "get-project-details":
            _format_project_details(console, data)
        else:
            # Fallback to JSON
            syntax = Syntax(json.dumps(data, indent=2), "json", theme="monokai")
            console.print(syntax)
            
    except Exception as e:
        console.print(f"[red]Error formatting output: {e}[/red]")
        _format_simple_output(response)


def _format_tools_list(console, data: Dict) -> None:
    """Format tools list output"""
    if "tools" not in data:
        console.print("[yellow]No tools found in response[/yellow]")
        return
        
    table = Table(title="Available MCP Tools", show_header=True, header_style="bold magenta")
    table.add_column("Tool Name", style="cyan", width=20)
    table.add_column("Description", style="white")
    table.add_column("Input Schema", style="green", width=30)
    
    for tool in data["tools"]:
        name = tool.get("name", "Unknown")
        description = tool.get("description", "No description")
        
        # Extract input schema info
        schema_info = "No schema"
        if "inputSchema" in tool and "properties" in tool["inputSchema"]:
            props = tool["inputSchema"]["properties"]
            required = tool["inputSchema"].get("required", [])
            schema_parts = []
            for prop, details in props.items():
                prop_type = details.get("type", "unknown")
                is_required = prop in required
                marker = "*" if is_required else ""
                schema_parts.append(f"{prop}{marker}: {prop_type}")
            schema_info = "\n".join(schema_parts)
        
        table.add_row(name, description, schema_info)
    
    console.print(table)


def _format_index_status(console, index_status: Dict) -> None:
    """Format and display indexing status information with ETA"""
    if not index_status:
        return
    
    in_progress = index_status.get("in_progress", False)
    progress_percentage = index_status.get("progress_percentage")
    indexed_files = index_status.get("indexed_files", 0)
    total_files = index_status.get("total_files", 0)
    estimated_time_remaining = index_status.get("estimated_time_remaining")
    state = index_status.get("state", "Unknown")
    
    # Helper function to format duration
    def format_duration(duration_dict):
        if not duration_dict or not isinstance(duration_dict, dict):
            return "unknown"
        
        secs = duration_dict.get("secs", 0)
        if secs < 60:
            return f"{secs} seconds"
        elif secs < 3600:
            minutes = secs // 60
            remaining_secs = secs % 60
            if remaining_secs == 0:
                return f"{minutes} minute{'s' if minutes != 1 else ''}"
            else:
                return f"{minutes} minute{'s' if minutes != 1 else ''} {remaining_secs} second{'s' if remaining_secs != 1 else ''}"
        else:
            hours = secs // 3600
            minutes = (secs % 3600) // 60
            if minutes == 0:
                return f"{hours} hour{'s' if hours != 1 else ''}"
            else:
                return f"{hours} hour{'s' if hours != 1 else ''} {minutes} minute{'s' if minutes != 1 else ''}"
    
    # Determine color based on state
    if in_progress:
        status_color = "yellow"
        status_icon = "⚡"
        title_text = "Indexing in progress"
    elif "Completed" in state:
        status_color = "green"
        status_icon = "✓"
        title_text = "Indexing completed"
    elif "Partial" in state:
        status_color = "orange"
        status_icon = "⚠"
        title_text = "Indexing partial/timeout"
    else:
        status_color = "blue"
        status_icon = "ℹ"
        title_text = "Indexing status"
    
    # Build the status display
    status_lines = [f"[{status_color}]{status_icon} {title_text}[/{status_color}]"]
    
    # Progress bar and percentage
    if progress_percentage is not None and total_files > 0:
        progress = progress_percentage / 100.0
        bar_width = 20
        filled = int(bar_width * progress)
        bar = "█" * filled + "░" * (bar_width - filled)
        status_lines.append(f"Progress: [{status_color}][{bar}] {progress_percentage:.1f}%[/{status_color}]")
    
    # Files count
    if total_files > 0:
        status_lines.append(f"Files: [bold]{indexed_files}/{total_files}[/bold]")
    
    # ETA
    if estimated_time_remaining and in_progress:
        eta_text = format_duration(estimated_time_remaining)
        status_lines.append(f"ETA: [cyan]{eta_text}[/cyan]")
    
    # State
    status_lines.append(f"State: [dim]{state}[/dim]")
    
    # Create and display the panel
    panel_content = "\n".join(status_lines)
    panel = Panel(panel_content, title="Indexing Status", border_style=status_color, padding=(0, 1))
    console.print(panel)
    console.print()


def _format_symbols_search(console, data: Dict) -> None:
    """Format symbol search results"""
    if not data.get("success", False):
        console.print(f"[red]Search failed: {data.get('error', 'Unknown error')}[/red]")
        return
        
    query = data.get("query", "Unknown")
    symbols = data.get("symbols", [])
    total_matches = data.get("total_matches", len(symbols))  # Fixed: use total_matches instead of total_found
    metadata = data.get("metadata", {})
    
    # Panel header similar to analyze_symbol
    console.print(Panel(f"[bold cyan]Search Results for '[yellow]{query}[/yellow]'[/bold cyan]", 
                       title="Symbol Search Results", border_style="blue"))
    
    # Display metadata information
    search_type = metadata.get("search_type", "unknown")
    build_dir = metadata.get("build_directory", "")
    if build_dir:
        console.print(f"[bold]Build Directory:[/bold] {build_dir}")
    console.print(f"[bold]Search Type:[/bold] {search_type}")
    console.print(f"[bold]Results:[/bold] Found {total_matches} symbols (showing {len(symbols)})")
    console.print()
    
    # Display index status if available
    index_status = data.get("index_status")
    if index_status:
        _format_index_status(console, index_status)
    
    if not symbols:
        console.print("[yellow]No symbols found[/yellow]")
        return
    
    table = Table(show_header=True, header_style="bold magenta")
    table.add_column("Symbol", style="cyan", width=25)
    table.add_column("Kind", style="blue", width=12)
    table.add_column("Location", style="green", width=25)
    table.add_column("Container", style="yellow", width=25)
    
    for symbol in symbols:
        name = symbol.get("name", "Unknown")
        kind = symbol.get("kind", "unknown")
        
        # Convert LSP symbol kind number to readable string if needed
        if isinstance(kind, int):
            kind_names = {
                1: "File", 2: "Module", 3: "Namespace", 4: "Package", 5: "Class",
                6: "Method", 7: "Property", 8: "Field", 9: "Constructor", 10: "Enum",
                11: "Interface", 12: "Function", 13: "Variable", 14: "Constant",
                15: "String", 16: "Number", 17: "Boolean", 18: "Array", 19: "Object",
                20: "Key", 21: "Null", 22: "EnumMember", 23: "Struct", 24: "Event",
                25: "Operator", 26: "TypeParameter"
            }
            kind = kind_names.get(kind, f"Unknown({kind})")
        
        # Format location - handle FileLocation string format
        location = "Unknown"
        if "location" in symbol:
            loc = symbol["location"]
            if isinstance(loc, str):
                # Handle FileLocation string format: /path/file.cpp:18:7-11
                try:
                    # Extract just the filename and line number
                    if ':' in loc:
                        parts = loc.rsplit(':', 2)  # Split from right to handle paths with colons
                        if len(parts) >= 2:
                            file_path = Path(parts[0]).name  # Just filename
                            line_num = parts[1]
                            location = f"{file_path}:{line_num}"
                        else:
                            location = Path(loc).name
                    else:
                        location = Path(loc).name
                except Exception:
                    location = str(loc)
            elif isinstance(loc, dict):
                # Handle LSP Location object format (legacy support)
                file_uri = loc.get("uri", "")
                if file_uri.startswith("file://"):
                    file_path = Path(file_uri[7:]).name  # Just filename
                else:
                    file_path = file_uri
                    
                if "range" in loc and "start" in loc["range"]:
                    line = loc["range"]["start"].get("line", 0) + 1  # Convert to 1-based
                    location = f"{file_path}:{line}"
                else:
                    location = file_path
        
        container = symbol.get("container_name", "")
        
        table.add_row(name, kind, location, container)
    
    console.print(table)


def extract_code_from_location(location_str: str) -> Dict[str, Union[str, int]]:
    """Extract code snippet from FileLocation string format.
    
    Args:
        location_str: Format like "/path/to/file.cpp:18:7-11" 
                     (file:line:start_col-end_col)
    
    Returns:
        Dict with 'code', 'line_number', 'file_path', 'error' keys
    """
    try:
        # Parse the location format: /path/to/file.cpp:18:7-11
        if ':' not in location_str:
            return {"error": "Invalid location format", "code": "", "line_number": 0, "file_path": ""}
            
        parts = location_str.rsplit(':', 2)  # Split from right to handle paths with colons
        if len(parts) < 3:
            return {"error": "Invalid location format", "code": "", "line_number": 0, "file_path": ""}
            
        file_path = parts[0]
        line_part = parts[1]
        col_part = parts[2]
        
        # Extract line number
        try:
            line_num = int(line_part)
        except ValueError:
            return {"error": "Invalid line number", "code": "", "line_number": 0, "file_path": file_path}
        
        # Try to read the file and extract the line
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                lines = f.readlines()
                
            if line_num <= 0 or line_num > len(lines):
                return {"error": f"Line {line_num} out of range", "code": "", "line_number": line_num, "file_path": file_path}
                
            # Get the line (convert from 1-based to 0-based indexing)
            code_line = lines[line_num - 1].rstrip('\n\r')
            
            return {
                "code": code_line,
                "line_number": line_num,
                "file_path": file_path,
                "error": None
            }
            
        except FileNotFoundError:
            return {"error": "File not found", "code": "", "line_number": line_num, "file_path": file_path}
        except PermissionError:
            return {"error": "Permission denied", "code": "", "line_number": line_num, "file_path": file_path}
        except Exception as e:
            return {"error": f"Error reading file: {e}", "code": "", "line_number": line_num, "file_path": file_path}
            
    except Exception as e:
        return {"error": f"Error parsing location: {e}", "code": "", "line_number": 0, "file_path": ""}


def _format_symbol_analysis(console, data: Dict, show_code: bool = True, show_all_members: bool = False) -> None:
    """Format symbol analysis results from AnalyzerResult structure"""
    
    # Extract data from AnalyzerResult structure
    symbol_data = data.get("symbol", {})
    query = data.get("query", "Unknown")
    definitions = data.get("definitions", [])
    declarations = data.get("declarations", [])
    hover_doc = data.get("hover_documentation")
    detail = data.get("detail")
    examples = data.get("examples", [])
    type_hierarchy = data.get("type_hierarchy")
    call_hierarchy = data.get("call_hierarchy")
    members = data.get("members")
    
    symbol_name = symbol_data.get("name", query)
    
    console.print(Panel(f"[bold cyan]Symbol Analysis: {symbol_name}[/bold cyan]", 
                       title="Symbol Information", border_style="blue"))
    
    # Basic symbol information
    if symbol_data:
        kind = symbol_data.get("kind")
        if kind:
            # Convert LSP symbol kind number to readable string if needed
            if isinstance(kind, int):
                kind_names = {
                    1: "File", 2: "Module", 3: "Namespace", 4: "Package", 5: "Class",
                    6: "Method", 7: "Property", 8: "Field", 9: "Constructor", 10: "Enum",
                    11: "Interface", 12: "Function", 13: "Variable", 14: "Constant",
                    15: "String", 16: "Number", 17: "Boolean", 18: "Array", 19: "Object",
                    20: "Key", 21: "Null", 22: "EnumMember", 23: "Struct", 24: "Event",
                    25: "Operator", 26: "TypeParameter"
                }
                kind = kind_names.get(kind, f"Unknown({kind})")
            console.print(f"[bold]Kind:[/bold] {kind}")
        
        fully_qualified_name = symbol_data.get("fully_qualified_name")
        if fully_qualified_name:
            console.print(f"[bold]Fully Qualified Name:[/bold] {fully_qualified_name}")
    
    # Show detail if available
    if detail:
        console.print(f"[bold]Detail:[/bold] {detail}")
    
    console.print()
    
    # Display index status if available
    index_status = data.get("index_status")
    if index_status:
        _format_index_status(console, index_status)
    
    # Helper to format location with code snippet
    def format_location_with_code(location_str, show_code_snippet=True):
        """Format FileLocation with optional code snippet"""
        if not show_code_snippet:
            return location_str
            
        code_info = extract_code_from_location(location_str)
        if code_info["error"]:
            return f"{location_str} [dim](code unavailable: {code_info['error']})[/dim]"
        
        # Format with syntax highlighting
        code = code_info["code"].strip()
        if code:
            # Use a more compact format for definitions/declarations
            return f"{location_str}\n    [green]→[/green] [cyan]{code}[/cyan]"
        else:
            return location_str
    
    if definitions:
        console.print(f"[bold]Definitions ({len(definitions)}):[/bold]")
        for i, definition in enumerate(definitions[:3]):  # Show first 3
            formatted = format_location_with_code(definition, show_code)
            console.print(f"  {i+1}. {formatted}")
        if len(definitions) > 3:
            console.print(f"  ... and {len(definitions) - 3} more")
    
    if declarations:
        console.print(f"[bold]Declarations ({len(declarations)}):[/bold]")
        for i, declaration in enumerate(declarations[:3]):  # Show first 3
            formatted = format_location_with_code(declaration, show_code)
            console.print(f"  {i+1}. {formatted}")
        if len(declarations) > 3:
            console.print(f"  ... and {len(declarations) - 3} more")
    
    # Documentation
    if hover_doc:
        console.print(f"\n[bold]Documentation:[/bold]")
        syntax = Syntax(hover_doc, "markdown", theme="monokai", line_numbers=False)
        console.print(Panel(syntax, border_style="dim"))
    
    # Usage examples
    if examples:
        console.print(f"\n[bold green]Usage Examples ({len(examples)}):[/bold green]")
        for i, example in enumerate(examples[:5], 1):  # Show first 5
            formatted = format_location_with_code(example, show_code)
            console.print(f"  {i}. {formatted}")
        if len(examples) > 5:
            console.print(f"  ... and {len(examples) - 5} more examples")
    
    # Type hierarchy
    if type_hierarchy:
        console.print(f"\n[bold green]Type Hierarchy:[/bold green]")
        
        supertypes = type_hierarchy.get("supertypes", [])
        subtypes = type_hierarchy.get("subtypes", [])
        
        if supertypes:
            console.print(f"[bold]Base Types ({len(supertypes)}):[/bold]")
            for supertype in supertypes[:3]:
                # Handle both string names and objects
                if isinstance(supertype, str):
                    name = supertype
                else:
                    name = supertype.get("name", "Unknown")
                console.print(f"  • [cyan]{name}[/cyan]")
            if len(supertypes) > 3:
                console.print(f"  ... and {len(supertypes) - 3} more")
        
        if subtypes:
            console.print(f"[bold]Derived Types ({len(subtypes)}):[/bold]")
            for subtype in subtypes[:3]:
                # Handle both string names and objects
                if isinstance(subtype, str):
                    name = subtype
                else:
                    name = subtype.get("name", "Unknown")
                console.print(f"  • [cyan]{name}[/cyan]")
            if len(subtypes) > 3:
                console.print(f"  ... and {len(subtypes) - 3} more")
    
    # Call hierarchy
    if call_hierarchy:
        console.print(f"\n[bold green]Call Hierarchy:[/bold green]")
        
        callers = call_hierarchy.get("callers", [])
        callees = call_hierarchy.get("callees", [])
        
        if callers:
            console.print(f"[bold]Callers ({len(callers)}):[/bold]")
            for caller in callers[:5]:
                # Handle both string names and objects
                if isinstance(caller, str):
                    name = caller
                else:
                    name = caller.get("name", "Unknown")
                console.print(f"  • [cyan]{name}[/cyan]")
            if len(callers) > 5:
                console.print(f"  ... and {len(callers) - 5} more")
        
        if callees:
            console.print(f"[bold]Callees ({len(callees)}):[/bold]")
            for callee in callees[:5]:
                # Handle both string names and objects
                if isinstance(callee, str):
                    name = callee
                else:
                    name = callee.get("name", "Unknown")
                console.print(f"  • [cyan]{name}[/cyan]")
            if len(callees) > 5:
                console.print(f"  ... and {len(callees) - 5} more")
    
    # Members (for classes/structs)
    if members:
        total_members = (len(members.get("methods", [])) + 
                        len(members.get("constructors", [])) + 
                        len(members.get("destructors", [])) + 
                        len(members.get("operators", [])))
        
        console.print(f"\n[bold green]Class Members ({total_members} total):[/bold green]")
        
        # Show methods
        methods = members.get("methods", [])
        if methods:
            console.print(f"[bold]Methods ({len(methods)}):[/bold]")
            method_limit = len(methods) if show_all_members else 5
            for method in methods[:method_limit]:
                name = method.get("name", "Unknown")
                signature = method.get("signature", "")
                console.print(f"  • [cyan]{name}[/cyan] {signature}")
            if len(methods) > method_limit:
                console.print(f"  ... and {len(methods) - method_limit} more methods")
        
        # Show constructors
        constructors = members.get("constructors", [])
        if constructors:
            console.print(f"[bold]Constructors ({len(constructors)}):[/bold]")
            constructor_limit = len(constructors) if show_all_members else 3
            for constructor in constructors[:constructor_limit]:
                signature = constructor.get("signature", "")
                console.print(f"  • [cyan]{symbol_name}[/cyan] {signature}")
            if len(constructors) > constructor_limit:
                console.print(f"  ... and {len(constructors) - constructor_limit} more constructors")
        
        # Show destructors
        destructors = members.get("destructors", [])
        if destructors:
            console.print(f"[bold]Destructors ({len(destructors)}):[/bold]")
            for destructor in destructors:
                signature = destructor.get("signature", "")
                console.print(f"  • [cyan]~{symbol_name}[/cyan] {signature}")
        
        # Show operators
        operators = members.get("operators", [])
        if operators:
            console.print(f"[bold]Operators ({len(operators)}):[/bold]")
            operator_limit = len(operators) if show_all_members else 3
            for operator in operators[:operator_limit]:
                name = operator.get("name", "Unknown")
                signature = operator.get("signature", "")
                console.print(f"  • [cyan]{name}[/cyan] {signature}")
            if len(operators) > operator_limit:
                console.print(f"  ... and {len(operators) - operator_limit} more operators")


def _format_project_details(console, data: Dict) -> None:
    """Format comprehensive project details including components and global configuration"""
    project_root_path = data.get("project_root_path", "Unknown")
    global_compilation_db = data.get("global_compilation_database_path")
    components = data.get("components", [])
    scan_depth = data.get("scan_depth", 0)
    discovered_at = data.get("discovered_at", "Unknown")
    rescanned = data.get("rescanned", False)
    
    # Compute values client-side
    project_name = "Unknown"
    if project_root_path != "Unknown":
        import os
        project_name = os.path.basename(str(project_root_path)) or "Unknown"
    
    component_count = len(components)
    
    # Extract unique provider types from components
    provider_types = []
    if components:
        provider_set = set(comp.get("provider_type", "unknown") for comp in components)
        provider_types = sorted(list(provider_set))
    
    # Project header with multi-provider info
    if project_name != "Unknown":
        providers_text = f" • {', '.join(provider_types)}" if provider_types else ""
        console.print(Panel(f"[bold cyan]Project: {project_name}[/bold cyan]{providers_text}", 
                           title="Project Details Analysis", border_style="blue"))
        
        if project_root_path != "Unknown":
            console.print(f"[bold]Project Root:[/bold] {project_root_path}")
        
        # Display global compilation database if configured
        if global_compilation_db:
            console.print(f"[bold]Global Compilation DB:[/bold] [green]{global_compilation_db}[/green]")
        else:
            console.print(f"[bold]Global Compilation DB:[/bold] [dim]Not configured (using component-specific databases)[/dim]")
            
        console.print(f"[bold]Scan Depth:[/bold] {scan_depth} levels")
        scan_status = " (fresh scan)" if rescanned else " (cached)"
        console.print(f"[bold]Discovered:[/bold] {discovered_at}{scan_status}")
        console.print()
    
    # Component summary
    if component_count == 0:
        console.print("[yellow]No project components found[/yellow]")
        console.print("This directory may not contain any supported build system configurations.")
        return
        
    console.print(f"[bold green]Found {component_count} project component{'s' if component_count != 1 else ''}:[/bold green]")
    console.print(f"[dim]Provider types: {', '.join(provider_types)}[/dim]")
    console.print()
    
    # Group components by provider type
    components_by_provider = {}
    for component in components:
        provider = component.get("provider_type", "unknown")
        if provider not in components_by_provider:
            components_by_provider[provider] = []
        components_by_provider[provider].append(component)
    
    # Display components grouped by provider
    for provider_type, provider_components in components_by_provider.items():
        provider_icon = "🔨" if provider_type == "cmake" else "⚡" if provider_type == "meson" else "🔧"
        console.print(f"[bold yellow]{provider_icon} {provider_type.upper()} Components ({len(provider_components)}):[/bold yellow]")
        
        for i, component in enumerate(provider_components, 1):
            build_path = component.get("build_dir_path", "Unknown")
            source_path = component.get("source_root_path", "Unknown")
            generator = component.get("generator", "Unknown")
            build_type = component.get("build_type", "Unknown")
            
            console.print(f"  [bold cyan]{i}. {build_path}[/bold cyan]")
            
            if source_path != "Unknown":
                console.print(f"     Source Root: {source_path}")
            if generator != "Unknown":
                console.print(f"     Generator: {generator}")
            if build_type != "Unknown":
                console.print(f"     Build Type: {build_type}")
            
            # Check if compilation database exists
            compile_db_path = component.get("compilation_database_path", "")
            if compile_db_path:
                console.print(f"     Compile DB: ✓ {compile_db_path}")
            else:
                console.print(f"     Compile DB: ✗ Not found")
            
            # Show build options if available (limit to important ones)
            build_options = component.get("build_options", {})
            if build_options:
                important_options = {k: v for k, v in build_options.items() 
                                   if not k.endswith(("_BINARY_DIR", "_SOURCE_DIR")) and len(str(v)) < 100}
                if important_options:
                    console.print("     [dim]Build Options:[/dim]")
                    for key, value in list(important_options.items())[:5]:  # Limit to 5 options
                        console.print(f"       {key}: {value}")
                    if len(important_options) > 5:
                        console.print(f"       ... and {len(important_options) - 5} more")
            
            console.print()
        
        console.print()


if __name__ == "__main__":
    main()