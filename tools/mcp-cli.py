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
  %(prog)s analyze-symbol "Math::sqrt" --include-usage-patterns
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
        help="Search query (supports fuzzy matching and qualified names)"
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
    
    # analyze-symbol subcommand
    analyze_parser = subparsers.add_parser(
        "analyze-symbol",
        help="Perform comprehensive analysis of a C++ symbol"
    )
    analyze_parser.add_argument(
        "symbol",
        help="Symbol name to analyze"
    )
    analyze_parser.add_argument(
        "--location-file",
        type=str,
        help="File URI for symbol disambiguation"
    )
    analyze_parser.add_argument(
        "--location-line",
        type=int,
        help="Line number for symbol disambiguation (0-based)"
    )
    analyze_parser.add_argument(
        "--location-char",
        type=int,
        help="Character position for symbol disambiguation (0-based)"
    )
    analyze_parser.add_argument(
        "--include-usage-patterns",
        action="store_true",
        help="Include usage statistics and examples"
    )
    analyze_parser.add_argument(
        "--max-usage-examples",
        type=int,
        default=5,
        help="Maximum number of usage examples (1-20, default: 5)"
    )
    analyze_parser.add_argument(
        "--include-inheritance",
        action="store_true",
        help="Include class inheritance hierarchy analysis"
    )
    analyze_parser.add_argument(
        "--include-call-hierarchy",
        action="store_true",
        help="Include function call hierarchy analysis"
    )
    analyze_parser.add_argument(
        "--max-call-depth",
        type=int,
        default=3,
        help="Maximum call hierarchy depth (1-10, default: 3)"
    )
    analyze_parser.add_argument(
        "--build-directory",
        type=str,
        help="Specify build directory path"
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
                
            response = client.call_tool("search_symbols", arguments)
            
        elif args.command == "analyze-symbol":
            arguments = {"symbol": args.symbol}
            
            # Add location if specified
            if args.location_file and args.location_line is not None and args.location_char is not None:
                arguments["location"] = {
                    "file_uri": args.location_file,
                    "position": {
                        "line": args.location_line,
                        "character": args.location_char
                    }
                }
            
            # Add optional boolean flags
            if args.include_usage_patterns:
                arguments["include_usage_patterns"] = args.include_usage_patterns
            if args.max_usage_examples != 5:
                arguments["max_usage_examples"] = args.max_usage_examples
            if args.include_inheritance:
                arguments["include_inheritance"] = args.include_inheritance
            if args.include_call_hierarchy:
                arguments["include_call_hierarchy"] = args.include_call_hierarchy
            if args.max_call_depth != 3:
                arguments["max_call_depth"] = args.max_call_depth
            if args.build_directory:
                arguments["build_directory"] = args.build_directory
                
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
            format_output(args.command, response)
            
    except McpCliError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nOperation cancelled", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(f"Unexpected error: {e}", file=sys.stderr)
        sys.exit(1)


def format_output(command: str, response: Dict) -> None:
    """Format and display the response in a user-friendly way"""
    if not RICH_AVAILABLE:
        _format_simple_output(response)
    else:
        _format_rich_output(command, response)


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


def _format_rich_output(command: str, response: Dict) -> None:
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
            _format_symbol_analysis(console, data)
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


def _format_symbols_search(console, data: Dict) -> None:
    """Format symbol search results"""
    if not data.get("success", False):
        console.print(f"[red]Search failed: {data.get('error', 'Unknown error')}[/red]")
        return
        
    query = data.get("query", "Unknown")
    symbols = data.get("symbols", [])
    total_found = data.get("total_found", len(symbols))
    
    console.print(f"[bold]Search Results for '[cyan]{query}[/cyan]'[/bold]")
    console.print(f"Found {total_found} symbols (showing {len(symbols)})")
    console.print()
    
    if not symbols:
        console.print("[yellow]No symbols found[/yellow]")
        return
    
    table = Table(show_header=True, header_style="bold magenta")
    table.add_column("Symbol", style="cyan", width=30)
    table.add_column("Kind", style="blue", width=12)
    table.add_column("Location", style="green")
    table.add_column("Container", style="yellow", width=20)
    
    for symbol in symbols:
        name = symbol.get("name", "Unknown")
        kind = symbol.get("kind", "unknown")
        
        # Format location
        location = "Unknown"
        if "location" in symbol:
            loc = symbol["location"]
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
        
        container = symbol.get("containerName", "")
        
        table.add_row(name, kind, location, container)
    
    console.print(table)


def _format_symbol_analysis(console, data: Dict) -> None:
    """Format symbol analysis results"""
    if not data.get("success", False):
        console.print(f"[red]Analysis failed: {data.get('error', 'Unknown error')}[/red]")
        return
        
    # Handle the actual response structure from the MCP server
    symbol_data = data.get("symbol", {})
    symbol_name = symbol_data.get("name", "Unknown Symbol")
    
    # Check if this is a namespace with class members
    if "class_members" in symbol_data and "members" in symbol_data["class_members"]:
        members = symbol_data["class_members"]["members"]
        console.print(Panel(f"[bold cyan]Namespace Members ({len(members)} items)[/bold cyan]", 
                           title=f"Analysis: {symbol_name}", border_style="blue"))
        
        # Group members by kind
        by_kind = {}
        for member in members:
            kind = member.get("kind", "unknown")
            if kind not in by_kind:
                by_kind[kind] = []
            by_kind[kind].append(member)
        
        # Display each kind group
        for kind, items in by_kind.items():
            console.print(f"\n[bold green]{kind.upper()}S ({len(items)}):[/bold green]")
            
            table = Table(show_header=True, header_style="bold magenta", box=None, padding=(0, 1))
            table.add_column("Name", style="cyan", width=20)
            table.add_column("Signature", style="white")
            table.add_column("Location", style="green", width=15)
            
            for item in items[:10]:  # Limit to first 10 items per kind
                name = item.get("name", "Unknown")
                detail = item.get("detail", "")
                
                # Extract location
                location = ""
                if "range" in item and "start" in item["range"]:
                    line = item["range"]["start"].get("line", 0) + 1
                    location = f"Line {line}"
                
                table.add_row(name, detail, location)
            
            if len(items) > 10:
                table.add_row("...", f"({len(items) - 10} more)", "")
            
            console.print(table)
        
        return
    
    # Handle individual symbol analysis
    if "definition" in symbol_data or "declaration" in symbol_data or "type_info" in symbol_data:
        console.print(Panel(f"[bold cyan]Symbol Analysis: {symbol_name}[/bold cyan]", 
                           title="Symbol Information", border_style="blue"))
        
        # Basic symbol info
        kind = symbol_data.get("kind", "Unknown")
        console.print(f"[bold]Kind:[/bold] {kind}")
        
        # Type information
        if "type_info" in symbol_data:
            type_info = symbol_data["type_info"]
            console.print(f"[bold]Type:[/bold] {type_info.get('type', 'Unknown')}")
            console.print(f"[bold]Fully Qualified Name:[/bold] {symbol_data.get('fully_qualified_name', 'Unknown')}")
            
            # Additional type properties
            properties = []
            if type_info.get("is_static"):
                properties.append("static")
            if type_info.get("is_const"):
                properties.append("const")
            if type_info.get("is_template"):
                properties.append("template")
            if properties:
                console.print(f"[bold]Properties:[/bold] {', '.join(properties)}")
        
        console.print()
        
        # Location information
        if "definition" in symbol_data:
            definition = symbol_data["definition"]
            file_uri = definition.get("uri", "")
            if file_uri.startswith("file://"):
                file_path = file_uri[7:]
            else:
                file_path = file_uri
            
            if "range" in definition and "start" in definition["range"]:
                line = definition["range"]["start"].get("line", 0) + 1
                console.print(f"[bold]Definition:[/bold] {file_path}:{line}")
        
        if "declaration" in symbol_data:
            declaration = symbol_data["declaration"]
            file_uri = declaration.get("uri", "")
            if file_uri.startswith("file://"):
                file_path = file_uri[7:]
            else:
                file_path = file_uri
                
            if "range" in declaration and "start" in declaration["range"]:
                line = declaration["range"]["start"].get("line", 0) + 1
                console.print(f"[bold]Declaration:[/bold] {file_path}:{line}")
        
        # Documentation
        if "documentation" in symbol_data:
            doc = symbol_data["documentation"]
            console.print(f"\n[bold]Documentation:[/bold]")
            # Display as code syntax for better formatting
            syntax = Syntax(doc, "markdown", theme="monokai", line_numbers=False)
            console.print(Panel(syntax, border_style="dim"))
        
        return
    
    # Fall back to the original analysis format if it's something else
    analysis = symbol_data
    
    console.print(Panel(f"[bold cyan]Symbol Analysis: {symbol_name}[/bold cyan]", 
                       title="Symbol Information", border_style="blue"))
    
    # Basic info
    if "definition" in analysis:
        definition = analysis["definition"]
        console.print(f"[bold]Type:[/bold] {definition.get('type', 'Unknown')}")
        console.print(f"[bold]Kind:[/bold] {definition.get('kind', 'Unknown')}")
        
        if "location" in definition:
            loc = definition["location"]
            file_uri = loc.get("uri", "")
            if file_uri.startswith("file://"):
                file_path = file_uri[7:]
            else:
                file_path = file_uri
            console.print(f"[bold]Location:[/bold] {file_path}")
        console.print()
    
    # Inheritance hierarchy
    if "inheritance" in analysis and analysis["inheritance"]:
        inheritance = analysis["inheritance"]
        tree = Tree(f"[bold green]Class Hierarchy[/bold green]")
        
        if "base_classes" in inheritance:
            base_node = tree.add("[blue]Base Classes[/blue]")
            for base_class in inheritance["base_classes"]:
                base_node.add(f"[cyan]{base_class.get('name', 'Unknown')}[/cyan]")
        
        if "derived_classes" in inheritance:
            derived_node = tree.add("[blue]Derived Classes[/blue]")
            for derived_class in inheritance["derived_classes"]:
                derived_node.add(f"[cyan]{derived_class.get('name', 'Unknown')}[/cyan]")
        
        console.print(tree)
        console.print()
    
    # Call hierarchy
    if "call_hierarchy" in analysis and analysis["call_hierarchy"]:
        call_hierarchy = analysis["call_hierarchy"]
        
        if "incoming_calls" in call_hierarchy:
            incoming = call_hierarchy["incoming_calls"]
            if incoming:
                console.print("[bold green]Incoming Calls:[/bold green]")
                for call in incoming[:5]:  # Limit display
                    caller_name = call.get("name", "Unknown")
                    console.print(f"  • [cyan]{caller_name}[/cyan]")
                console.print()
        
        if "outgoing_calls" in call_hierarchy:
            outgoing = call_hierarchy["outgoing_calls"]
            if outgoing:
                console.print("[bold green]Outgoing Calls:[/bold green]")
                for call in outgoing[:5]:  # Limit display
                    callee_name = call.get("name", "Unknown")
                    console.print(f"  • [cyan]{callee_name}[/cyan]")
                console.print()
    
    # Usage patterns
    if "usage_patterns" in analysis and analysis["usage_patterns"]:
        usage = analysis["usage_patterns"]
        console.print("[bold green]Usage Examples:[/bold green]")
        
        for i, example in enumerate(usage.get("examples", [])[:3], 1):
            if "code_snippet" in example:
                console.print(f"[bold]Example {i}:[/bold]")
                syntax = Syntax(example["code_snippet"], "cpp", theme="monokai", line_numbers=False)
                console.print(Panel(syntax, border_style="dim"))
                console.print()


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