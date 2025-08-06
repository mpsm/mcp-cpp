#!/usr/bin/env python3
"""
clangd Index File Viewer - Parse and display contents of clangd .idx files

Supports clangd format versions 12-20 (LLVM 10+)
Displays all available information including symbols, references, relations, and includes.
"""

from io import BytesIO
import argparse
import struct
import zlib
import json
import sys
import os
from typing import Dict, List, Optional, Any, Tuple, BinaryIO
from dataclasses import dataclass, field, asdict
from enum import IntEnum

try:
    from rich.console import Console
    from rich.table import Table
    from rich.panel import Panel
    RICH_AVAILABLE = True
except ImportError:
    RICH_AVAILABLE = False


# Enums from the specification
class SymbolKind(IntEnum):
    Unknown = 0
    Module = 1
    Namespace = 2
    NamespaceAlias = 3
    Macro = 4
    Enum = 5
    Struct = 6
    Class = 7
    Protocol = 8
    Extension = 9
    Union = 10
    TypeAlias = 11
    Function = 12
    Variable = 13
    Field = 14
    EnumConstant = 15
    InstanceMethod = 16
    ClassMethod = 17
    StaticMethod = 18
    InstanceProperty = 19
    ClassProperty = 20
    StaticProperty = 21
    Constructor = 22
    Destructor = 23
    ConversionFunction = 24
    Parameter = 25
    Using = 26
    TemplateTypeParm = 27
    TemplateTemplateParm = 28
    NonTypeTemplateParm = 29
    Concept = 30

    @classmethod
    def _missing_(cls, value):
        pseudo_member = int.__new__(cls, value)
        pseudo_member._name_ = f"Unknown_{value}"
        pseudo_member._value_ = value
        return pseudo_member


class SymbolLanguage(IntEnum):
    C = 0
    ObjC = 1
    CXX = 2
    Swift = 3

    @classmethod
    def _missing_(cls, value):
        pseudo_member = int.__new__(cls, value)
        pseudo_member._name_ = f"Unknown_{value}"
        pseudo_member._value_ = value
        return pseudo_member


class RefKind(IntEnum):
    Unknown = 0
    Declaration = 1 << 0  # 1
    Definition = 1 << 1   # 2
    Reference = 1 << 2    # 4
    Spelled = 1 << 3      # 8
    Call = 1 << 4         # 16
    All = Declaration | Definition | Reference | Spelled  # Combination flag

    @classmethod
    def _missing_(cls, value):
        # Return a dynamic member for unknown values
        pseudo_member = int.__new__(cls, value)
        pseudo_member._name_ = f"Combination_{value}"
        pseudo_member._value_ = value
        return pseudo_member


class RelationKind(IntEnum):
    BaseOf = 0
    OverriddenBy = 1

    @classmethod
    def _missing_(cls, value):
        pseudo_member = int.__new__(cls, value)
        pseudo_member._name_ = f"Unknown_{value}"
        pseudo_member._value_ = value
        return pseudo_member


class SymbolFlag(IntEnum):
    None_ = 0
    IndexedForCodeCompletion = 1
    Deprecated = 2
    ImplementationDetail = 4
    VisibleOutsideFile = 8
    HasDocComment = 16


class IncludeDirective(IntEnum):
    Invalid = 0
    Include = 1
    Import = 2


class SourceFlag(IntEnum):
    None_ = 0
    IsTU = 1
    HadErrors = 2


@dataclass
class SymbolLocation:
    file_uri: str
    start_line: int
    start_column: int
    end_line: int
    end_column: int


@dataclass
class IncludeHeaderWithReferences:
    header: str
    references: int
    supported_directives: int = 1  # Default to Include


@dataclass
class Symbol:
    id: bytes
    kind: SymbolKind
    language: SymbolLanguage
    name: str
    scope: str
    template_specialization_args: str
    definition: Optional[SymbolLocation]
    canonical_declaration: Optional[SymbolLocation]
    references: int
    flags: int
    signature: str
    completion_snippet_suffix: str
    documentation: str
    return_type: str
    type: str
    include_headers: List[IncludeHeaderWithReferences] = field(
        default_factory=list)


@dataclass
class Reference:
    kind: RefKind
    location: SymbolLocation
    container: Optional[bytes] = None


@dataclass
class Relation:
    subject: bytes
    predicate: RelationKind
    object: bytes


@dataclass
class IncludeGraphNode:
    flags: int
    uri: str
    digest: bytes
    direct_includes: List[str] = field(default_factory=list)


class VarintReader:
    """Helper class to read variable-length integers"""

    @staticmethod
    def read_varint(f: BinaryIO) -> int:
        """Read a variable-length integer from the stream"""
        result = 0
        shift = 0
        while True:
            byte = f.read(1)
            if not byte:
                raise EOFError("Unexpected end of file reading varint")
            b = byte[0]
            result |= (b & 0x7F) << shift
            if (b & 0x80) == 0:
                break
            shift += 7
        return result


class StringTable:
    """Manages the string table with decompression support"""

    def __init__(self, data: bytes):
        # First 4 bytes indicate uncompressed size
        uncompressed_size = struct.unpack('<I', data[:4])[0]

        if uncompressed_size == 0:
            # Raw data follows
            self.strings = self._parse_strings(data[4:])
        else:
            # zlib compressed data
            compressed_data = data[4:]
            try:
                decompressed = zlib.decompress(compressed_data)
                if len(decompressed) != uncompressed_size:
                    raise ValueError(
                        f"Decompressed size mismatch: expected {uncompressed_size}, got {len(decompressed)}")
                self.strings = self._parse_strings(decompressed)
            except zlib.error as e:
                raise ValueError(f"Failed to decompress string table: {e}")

    def _parse_strings(self, data: bytes) -> List[str]:
        """Parse null-terminated strings from data"""
        strings = []
        current = []
        for byte in data:
            if byte == 0:
                strings.append(bytes(current).decode(
                    'utf-8', errors='replace'))
                current = []
            else:
                current.append(byte)
        # Handle case where data doesn't end with null
        if current:
            strings.append(bytes(current).decode('utf-8', errors='replace'))
        return strings

    def get(self, index: int) -> str:
        """Get string by index"""
        if 0 <= index < len(self.strings):
            return self.strings[index]
        return ""


class FormatStrategy:
    """Base class for version-specific parsing strategies"""

    def parse_ref(self, f: BinaryIO, string_table: StringTable) -> Reference:
        raise NotImplementedError

    def parse_include_header(self, f: BinaryIO, string_table: StringTable) -> IncludeHeaderWithReferences:
        raise NotImplementedError


class Format12Strategy(FormatStrategy):
    """Parser for format version 12"""

    def parse_ref(self, f: BinaryIO, string_table: StringTable) -> Reference:
        kind = RefKind(f.read(1)[0])
        location = self._read_location(f, string_table)
        # No container field in format 12
        return Reference(kind=kind, location=location, container=None)

    def parse_include_header(self, f: BinaryIO, string_table: StringTable) -> IncludeHeaderWithReferences:
        header_idx = VarintReader.read_varint(f)
        references = VarintReader.read_varint(f)
        return IncludeHeaderWithReferences(
            header=string_table.get(header_idx),
            references=references,
            supported_directives=IncludeDirective.Include
        )

    def _read_location(self, f: BinaryIO, string_table: StringTable) -> SymbolLocation:
        file_uri_idx = VarintReader.read_varint(f)
        start_line = VarintReader.read_varint(f)
        start_column = VarintReader.read_varint(f)
        end_line = VarintReader.read_varint(f)
        end_column = VarintReader.read_varint(f)
        return SymbolLocation(
            file_uri=string_table.get(file_uri_idx),
            start_line=start_line,
            start_column=start_column,
            end_line=end_line,
            end_column=end_column
        )


class Format13To17Strategy(FormatStrategy):
    """Parser for format versions 13-17"""

    def parse_ref(self, f: BinaryIO, string_table: StringTable) -> Reference:
        kind = RefKind(f.read(1)[0])
        location = self._read_location(f, string_table)
        container = f.read(8)  # Container field added in format 13
        return Reference(kind=kind, location=location, container=container)

    def parse_include_header(self, f: BinaryIO, string_table: StringTable) -> IncludeHeaderWithReferences:
        header_idx = VarintReader.read_varint(f)
        references = VarintReader.read_varint(f)
        return IncludeHeaderWithReferences(
            header=string_table.get(header_idx),
            references=references,
            supported_directives=IncludeDirective.Include
        )

    def _read_location(self, f: BinaryIO, string_table: StringTable) -> SymbolLocation:
        file_uri_idx = VarintReader.read_varint(f)
        start_line = VarintReader.read_varint(f)
        start_column = VarintReader.read_varint(f)
        end_line = VarintReader.read_varint(f)
        end_column = VarintReader.read_varint(f)
        return SymbolLocation(
            file_uri=string_table.get(file_uri_idx),
            start_line=start_line,
            start_column=start_column,
            end_line=end_line,
            end_column=end_column
        )


class Format18PlusStrategy(FormatStrategy):
    """Parser for format versions 18+"""

    def parse_ref(self, f: BinaryIO, string_table: StringTable) -> Reference:
        kind = RefKind(f.read(1)[0])
        location = self._read_location(f, string_table)
        container = f.read(8)  # Container field present
        return Reference(kind=kind, location=location, container=container)

    def parse_include_header(self, f: BinaryIO, string_table: StringTable) -> IncludeHeaderWithReferences:
        header_idx = VarintReader.read_varint(f)
        packed = VarintReader.read_varint(f)
        references = packed >> 2  # Upper 30 bits
        supported_directives = packed & 0x3  # Lower 2 bits
        return IncludeHeaderWithReferences(
            header=string_table.get(header_idx),
            references=references,
            supported_directives=supported_directives
        )

    def _read_location(self, f: BinaryIO, string_table: StringTable) -> SymbolLocation:
        file_uri_idx = VarintReader.read_varint(f)
        start_line = VarintReader.read_varint(f)
        start_column = VarintReader.read_varint(f)
        end_line = VarintReader.read_varint(f)
        end_column = VarintReader.read_varint(f)
        return SymbolLocation(
            file_uri=string_table.get(file_uri_idx),
            start_line=start_line,
            start_column=start_column,
            end_line=end_line,
            end_column=end_column
        )


class RIFFParser:
    """Parser for RIFF container format"""

    def __init__(self, file_path: str):
        self.file_path = file_path
        self.chunks = {}
        self._parse()

    def _parse(self):
        """Parse the RIFF file structure"""
        with open(self.file_path, 'rb') as f:
            # Read RIFF header
            magic = f.read(4)
            if magic != b'RIFF':
                raise ValueError(f"Not a RIFF file: {magic}")

            file_size = struct.unpack('<I', f.read(4))[0]
            type_id = f.read(4)
            if type_id != b'CdIx':
                raise ValueError(f"Not a clangd index file: {type_id}")

            # Read chunks
            while f.tell() < file_size + 8:
                chunk_id = f.read(4)
                if not chunk_id:
                    break

                chunk_size = struct.unpack('<I', f.read(4))[0]
                chunk_data = f.read(chunk_size)

                # Store chunk
                self.chunks[chunk_id.decode(
                    'ascii', errors='ignore')] = chunk_data

                # Skip padding to even boundary
                if chunk_size % 2:
                    f.read(1)

    def get_chunk(self, chunk_id: str) -> Optional[bytes]:
        """Get chunk data by ID"""
        return self.chunks.get(chunk_id)


class IdxFileParser:
    """Main parser for clangd index files"""

    def __init__(self, file_path: str):
        self.file_path = file_path
        self.riff = RIFFParser(file_path)
        self.format_version = None
        self.string_table = None
        self.strategy = None
        self._initialize()

    def _initialize(self):
        """Initialize parser with format version and string table"""
        # Parse meta chunk
        meta_data = self.riff.get_chunk('meta')
        if not meta_data:
            raise ValueError("Missing required 'meta' chunk")

        self.format_version = struct.unpack('<I', meta_data[:4])[0]

        # Select strategy based on version
        if self.format_version == 12:
            self.strategy = Format12Strategy()
        elif 13 <= self.format_version <= 17:
            self.strategy = Format13To17Strategy()
        elif 18 <= self.format_version <= 20:
            self.strategy = Format18PlusStrategy()
        else:
            raise ValueError(
                f"Unsupported format version: {self.format_version}")

        # Parse string table
        stri_data = self.riff.get_chunk('stri')
        if not stri_data:
            raise ValueError("Missing required 'stri' chunk")

        self.string_table = StringTable(stri_data)

    def parse_symbols(self) -> List[Symbol]:
        """Parse symbols from the symb chunk"""
        symb_data = self.riff.get_chunk('symb')
        if not symb_data:
            return []

        symbols = []
        with BytesIO(symb_data) as f:
            while f.tell() < len(symb_data):
                try:
                    symbol = self._parse_symbol(f)
                    symbols.append(symbol)
                except (EOFError, struct.error):
                    break

        return symbols

    def _parse_symbol(self, f: BinaryIO) -> Symbol:
        """Parse a single symbol"""
        symbol_id = f.read(8)
        kind = SymbolKind(f.read(1)[0])
        language = SymbolLanguage(f.read(1)[0])

        name_idx = VarintReader.read_varint(f)
        scope_idx = VarintReader.read_varint(f)
        template_args_idx = VarintReader.read_varint(f)

        definition = self._read_location(f)
        canonical_declaration = self._read_location(f)

        references = VarintReader.read_varint(f)
        flags = f.read(1)[0]

        signature_idx = VarintReader.read_varint(f)
        snippet_idx = VarintReader.read_varint(f)
        documentation_idx = VarintReader.read_varint(f)
        return_type_idx = VarintReader.read_varint(f)
        type_idx = VarintReader.read_varint(f)

        # Parse include headers
        include_count = VarintReader.read_varint(f)
        include_headers = []
        for _ in range(include_count):
            inc = self.strategy.parse_include_header(f, self.string_table)
            include_headers.append(inc)

        return Symbol(
            id=symbol_id,
            kind=kind,
            language=language,
            name=self.string_table.get(name_idx),
            scope=self.string_table.get(scope_idx),
            template_specialization_args=self.string_table.get(
                template_args_idx),
            definition=definition,
            canonical_declaration=canonical_declaration,
            references=references,
            flags=flags,
            signature=self.string_table.get(signature_idx),
            completion_snippet_suffix=self.string_table.get(snippet_idx),
            documentation=self.string_table.get(documentation_idx),
            return_type=self.string_table.get(return_type_idx),
            type=self.string_table.get(type_idx),
            include_headers=include_headers
        )

    def _read_location(self, f: BinaryIO) -> Optional[SymbolLocation]:
        """Read a symbol location"""
        file_uri_idx = VarintReader.read_varint(f)
        if file_uri_idx == 0:
            # Skip the rest of the location fields
            VarintReader.read_varint(f)  # start_line
            VarintReader.read_varint(f)  # start_column
            VarintReader.read_varint(f)  # end_line
            VarintReader.read_varint(f)  # end_column
            return None

        start_line = VarintReader.read_varint(f)
        start_column = VarintReader.read_varint(f)
        end_line = VarintReader.read_varint(f)
        end_column = VarintReader.read_varint(f)

        return SymbolLocation(
            file_uri=self.string_table.get(file_uri_idx),
            start_line=start_line,
            start_column=start_column,
            end_line=end_line,
            end_column=end_column
        )

    def parse_refs(self) -> Dict[bytes, List[Reference]]:
        """Parse references from the refs chunk"""
        refs_data = self.riff.get_chunk('refs')
        if not refs_data:
            return {}

        refs_by_symbol = {}
        with BytesIO(refs_data) as f:
            while f.tell() < len(refs_data):
                try:
                    symbol_id = f.read(8)
                    ref_count = VarintReader.read_varint(f)
                    refs = []
                    for _ in range(ref_count):
                        ref = self.strategy.parse_ref(f, self.string_table)
                        refs.append(ref)
                    refs_by_symbol[symbol_id] = refs
                except (EOFError, struct.error):
                    break

        return refs_by_symbol

    def parse_relations(self) -> List[Relation]:
        """Parse relations from the rela chunk"""
        rela_data = self.riff.get_chunk('rela')
        if not rela_data:
            return []

        relations = []
        with BytesIO(rela_data) as f:
            while f.tell() < len(rela_data):
                try:
                    subject = f.read(8)
                    predicate = RelationKind(f.read(1)[0])
                    object_id = f.read(8)
                    relations.append(
                        Relation(subject=subject, predicate=predicate, object=object_id))
                except (EOFError, struct.error):
                    break

        return relations

    def parse_sources(self) -> List[IncludeGraphNode]:
        """Parse include graph from the srcs chunk"""
        srcs_data = self.riff.get_chunk('srcs')
        if not srcs_data:
            return []

        nodes = []
        with BytesIO(srcs_data) as f:
            while f.tell() < len(srcs_data):
                try:
                    flags = f.read(1)[0]
                    uri_idx = VarintReader.read_varint(f)
                    digest = f.read(8)
                    include_count = VarintReader.read_varint(f)
                    includes = []
                    for _ in range(include_count):
                        inc_idx = VarintReader.read_varint(f)
                        includes.append(self.string_table.get(inc_idx))

                    nodes.append(IncludeGraphNode(
                        flags=flags,
                        uri=self.string_table.get(uri_idx),
                        digest=digest,
                        direct_includes=includes
                    ))
                except (EOFError, struct.error):
                    break

        return nodes

    def parse_command(self) -> Optional[Tuple[str, List[str]]]:
        """Parse compile command from the cmdl chunk"""
        cmdl_data = self.riff.get_chunk('cmdl')
        if not cmdl_data:
            return None

        with BytesIO(cmdl_data) as f:
            directory_idx = VarintReader.read_varint(f)
            cmd_count = VarintReader.read_varint(f)
            cmd_args = []
            for _ in range(cmd_count):
                arg_idx = VarintReader.read_varint(f)
                cmd_args.append(self.string_table.get(arg_idx))

            return (self.string_table.get(directory_idx), cmd_args)

    def get_file_info(self) -> Dict[str, Any]:
        """Extract file metadata including shard from filename"""
        filename = os.path.basename(self.file_path)
        parts = filename.rsplit('.', 2)

        info = {
            'filename': filename,
            'format_version': self.format_version,
            'chunks': list(self.riff.chunks.keys()),
            'chunk_sizes': {k: len(v) for k, v in self.riff.chunks.items()}
        }

        if len(parts) == 3 and parts[2] == 'idx':
            info['basename'] = parts[0]
            info['shard'] = parts[1]  # The hash part

        return info


# Import BytesIO for in-memory file operations


def format_symbol_id(symbol_id: bytes) -> str:
    """Format symbol ID as hex string"""
    return symbol_id.hex()


def format_flags(flags: int) -> List[str]:
    """Format symbol flags as list of strings"""
    flag_names = []
    if flags & SymbolFlag.IndexedForCodeCompletion:
        flag_names.append("IndexedForCodeCompletion")
    if flags & SymbolFlag.Deprecated:
        flag_names.append("Deprecated")
    if flags & SymbolFlag.ImplementationDetail:
        flag_names.append("ImplementationDetail")
    if flags & SymbolFlag.VisibleOutsideFile:
        flag_names.append("VisibleOutsideFile")
    if flags & SymbolFlag.HasDocComment:
        flag_names.append("HasDocComment")
    return flag_names


def format_ref_kind(kind: RefKind) -> str:
    """Format reference kind as bitwise flags string"""
    # Handle dynamic combination values
    if hasattr(kind, '_name_') and kind._name_.startswith('Combination_'):
        # Still parse the bits for combination values
        pass

    if kind == RefKind.Unknown:
        return "Unknown"

    kinds = []
    if kind & RefKind.Declaration:
        kinds.append("Declaration")
    if kind & RefKind.Definition:
        kinds.append("Definition")
    if kind & RefKind.Reference:
        kinds.append("Reference")
    if kind & RefKind.Spelled:
        kinds.append("Spelled")
    if kind & RefKind.Call:
        kinds.append("Call")

    return "|".join(kinds) if kinds else f"Unknown({int(kind)})"


def format_source_flags(flags: int) -> List[str]:
    """Format source flags as list of strings"""
    flag_names = []
    if flags & SourceFlag.IsTU:
        flag_names.append("IsTU")
    if flags & SourceFlag.HadErrors:
        flag_names.append("HadErrors")
    return flag_names


class PrettyFormatter:
    """Format output using rich for pretty display"""

    def __init__(self):
        self.console = Console()

    def format_summary(self, parser: IdxFileParser):
        """Format and display a summary of the index file"""
        file_info = parser.get_file_info()
        self._show_file_info(file_info)

        # Parse all data
        symbols = parser.parse_symbols()
        refs = parser.parse_refs()
        relations = parser.parse_relations()
        sources = parser.parse_sources()
        command = parser.parse_command()

        # Create summary statistics
        summary_data = []

        # Symbols breakdown by kind
        symbol_kinds = {}
        symbol_languages = {}
        symbols_with_docs = 0
        symbols_with_defs = 0

        for symbol in symbols:
            symbol_kinds[symbol.kind.name] = symbol_kinds.get(
                symbol.kind.name, 0) + 1
            symbol_languages[symbol.language.name] = symbol_languages.get(
                symbol.language.name, 0) + 1
            if symbol.documentation:
                symbols_with_docs += 1
            if symbol.definition:
                symbols_with_defs += 1

        # Summary statistics
        summary_data.append(f"[bold]Symbols:[/bold] {len(symbols)} total")
        if symbols:
            summary_data.append(
                f"  • Languages: {', '.join(f'{lang} ({count})' for lang, count in symbol_languages.items())}")
            top_kinds = sorted(symbol_kinds.items(),
                               key=lambda x: x[1], reverse=True)[:5]
            summary_data.append(
                f"  • Top kinds: {', '.join(f'{kind} ({count})' for kind, count in top_kinds)}")
            summary_data.append(
                f"  • With definitions: {symbols_with_defs}/{len(symbols)}")
            summary_data.append(
                f"  • With documentation: {symbols_with_docs}/{len(symbols)}")

        # References summary
        total_refs = sum(len(r) for r in refs.values())
        summary_data.append(
            f"\n[bold]References:[/bold] {total_refs} total in {len(refs)} symbols")
        if refs:
            # Count reference kinds
            ref_kinds = {}
            for ref_list in refs.values():
                for ref in ref_list:
                    kind_str = format_ref_kind(ref.kind)
                    ref_kinds[kind_str] = ref_kinds.get(kind_str, 0) + 1
            top_ref_kinds = sorted(
                ref_kinds.items(), key=lambda x: x[1], reverse=True)[:3]
            summary_data.append(
                f"  • Top patterns: {', '.join(f'{kind} ({count})' for kind, count in top_ref_kinds)}")

        # Relations summary
        if relations:
            relation_types = {}
            for rel in relations:
                relation_types[rel.predicate.name] = relation_types.get(
                    rel.predicate.name, 0) + 1
            summary_data.append(
                f"\n[bold]Relations:[/bold] {len(relations)} total")
            for rel_type, count in relation_types.items():
                summary_data.append(f"  • {rel_type}: {count}")
        else:
            summary_data.append(f"\n[bold]Relations:[/bold] None")

        # Include graph summary
        summary_data.append(
            f"\n[bold]Include Graph:[/bold] {len(sources)} files")
        if sources:
            tu_count = sum(1 for s in sources if s.flags & SourceFlag.IsTU)
            error_count = sum(1 for s in sources if s.flags &
                              SourceFlag.HadErrors)
            total_includes = sum(len(s.direct_includes) for s in sources)
            summary_data.append(f"  • Translation units: {tu_count}")
            if error_count:
                summary_data.append(f"  • Files with errors: {error_count}")
            summary_data.append(f"  • Total include edges: {total_includes}")

        # Compile command
        if command:
            summary_data.append(
                f"\n[bold]Compile Command:[/bold] Present ({len(command[1])} arguments)")

        # Display summary
        self.console.print(Panel(
            "\n".join(summary_data),
            title="📊 Index File Summary",
            expand=False
        ))

    def format(self, parser: IdxFileParser, verbose: bool = False, show_all: bool = False):
        """Format and display the parsed index file"""
        # File info
        file_info = parser.get_file_info()
        self._show_file_info(file_info)

        # String table info
        self._show_string_table_info(parser.string_table)

        # Symbols
        symbols = parser.parse_symbols()
        if symbols:
            self._show_symbols(symbols, verbose, show_all)

        # References
        refs = parser.parse_refs()
        if refs:
            self._show_references(refs, verbose, show_all)

        # Relations
        relations = parser.parse_relations()
        if relations:
            self._show_relations(relations, show_all)

        # Include graph
        sources = parser.parse_sources()
        if sources:
            self._show_sources(sources, verbose, show_all)

        # Compile command
        command = parser.parse_command()
        if command:
            self._show_command(command)

    def _show_file_info(self, info: Dict[str, Any]):
        """Display file metadata"""
        panel_content = f"""[bold]Filename:[/bold] {info['filename']}
[bold]Format Version:[/bold] {info['format_version']}"""

        if 'basename' in info:
            panel_content += f"\n[bold]Base Name:[/bold] {info['basename']}"
            panel_content += f"\n[bold]Shard (Hash):[/bold] {info['shard']}"

        chunk_info = []
        for chunk, size in info['chunk_sizes'].items():
            chunk_info.append(f"{chunk}({size:,}b)")
        panel_content += f"\n[bold]Chunks:[/bold] {', '.join(chunk_info)}"

        self.console.print(
            Panel(panel_content, title="📁 Index File Info", expand=False))

    def _show_string_table_info(self, string_table: StringTable):
        """Display string table statistics"""
        total_strings = len(string_table.strings)
        total_bytes = sum(len(s.encode('utf-8')) +
                          1 for s in string_table.strings)

        info = f"""[bold]Total Strings:[/bold] {total_strings:,}
[bold]Total Size:[/bold] {total_bytes:,} bytes
[bold]Average Length:[/bold] {total_bytes // max(total_strings, 1)} bytes"""

        self.console.print(Panel(info, title="📝 String Table", expand=False))

    def _show_symbols(self, symbols: List[Symbol], verbose: bool, show_all: bool = False):
        """Display symbols in a table"""
        if not verbose:
            # Simple table view
            table = Table(title=f"🔤 Symbols ({len(symbols)} total)")
            table.add_column("ID", style="cyan", no_wrap=True)
            table.add_column("Name", style="green")
            table.add_column("Kind", style="yellow")
            table.add_column("Scope", style="blue")
            table.add_column("Refs", justify="right")

            limit = len(symbols) if show_all else 50
            for symbol in symbols[:limit]:
                row = [
                    format_symbol_id(symbol.id)[:16] + "...",
                    symbol.name or "(anonymous)",
                    symbol.kind.name,
                    symbol.scope or "(global)",
                    str(symbol.references)
                ]
                table.add_row(*row)

            if not show_all and len(symbols) > 50:
                table.add_row(
                    "...", f"({len(symbols) - 50} more symbols)", "...", "...", "...")

            self.console.print(table)
        else:
            # Detailed view with all fields
            limit = len(symbols) if show_all else 20
            for i, symbol in enumerate(symbols[:limit]):
                # Create a detailed panel for each symbol
                details = []
                details.append(
                    f"[bold cyan]ID:[/bold cyan] {format_symbol_id(symbol.id)}")
                details.append(
                    f"[bold green]Name:[/bold green] {symbol.name or '(anonymous)'}")
                details.append(
                    f"[bold yellow]Kind:[/bold yellow] {symbol.kind.name}")
                details.append(
                    f"[bold blue]Scope:[/bold blue] {symbol.scope or '(global)'}")
                details.append(
                    f"[bold white]Language:[/bold white] {symbol.language.name}")

                # Template specialization args
                if symbol.template_specialization_args:
                    details.append(
                        f"[bold]Template Args:[/bold] {symbol.template_specialization_args}")

                # Location information
                if symbol.definition:
                    loc = symbol.definition
                    details.append(
                        f"[bold green]Definition:[/bold green] {os.path.basename(loc.file_uri)}:{loc.start_line}:{loc.start_column}-{loc.end_line}:{loc.end_column}")
                if symbol.canonical_declaration:
                    loc = symbol.canonical_declaration
                    details.append(
                        f"[bold blue]Declaration:[/bold blue] {os.path.basename(loc.file_uri)}:{loc.start_line}:{loc.start_column}-{loc.end_line}:{loc.end_column}")

                # Type information
                if symbol.type:
                    details.append(
                        f"[bold magenta]Type:[/bold magenta] {symbol.type}")
                if symbol.return_type:
                    details.append(
                        f"[bold magenta]Return Type:[/bold magenta] {symbol.return_type}")
                if symbol.signature:
                    details.append(
                        f"[bold]Signature:[/bold] {symbol.signature}")

                # Documentation
                if symbol.documentation:
                    details.append(f"[bold]Documentation:[/bold] {symbol.documentation[:100]}..." if len(
                        symbol.documentation) > 100 else f"[bold]Documentation:[/bold] {symbol.documentation}")

                # Completion snippet
                if symbol.completion_snippet_suffix:
                    details.append(
                        f"[bold]Completion Snippet:[/bold] {symbol.completion_snippet_suffix}")

                # Flags and references
                flags = format_flags(symbol.flags)
                if flags:
                    details.append(
                        f"[bold red]Flags:[/bold red] {', '.join(flags)}")
                details.append(f"[bold]References:[/bold] {symbol.references}")

                # Include headers
                if symbol.include_headers:
                    headers = []
                    for inc in symbol.include_headers:
                        directive = "Include" if inc.supported_directives == 1 else "Import" if inc.supported_directives == 2 else f"Dir:{inc.supported_directives}"
                        headers.append(
                            f"{inc.header} ({inc.references} refs, {directive})")
                    details.append(
                        f"[bold]Include Headers:[/bold] {', '.join(headers)}")

                panel = Panel(
                    "\n".join(details),
                    title=f"Symbol {i+1}/{len(symbols)}: {symbol.name or '(anonymous)'}",
                    expand=False
                )
                self.console.print(panel)

            if not show_all and len(symbols) > limit:
                self.console.print(
                    f"\n[dim]... ({len(symbols) - limit} more symbols)[/dim]")

    def _show_references(self, refs: Dict[bytes, List[Reference]], verbose: bool, show_all: bool = False):
        """Display references"""
        total_refs = sum(len(r) for r in refs.values())

        table = Table(
            title=f"📌 References ({len(refs)} symbols, {total_refs} total refs)")
        table.add_column("Symbol ID", style="cyan", no_wrap=True)
        table.add_column("Ref Count", justify="right")
        if verbose:
            table.add_column("Kinds", style="yellow")
            table.add_column("Files", style="blue")
            table.add_column("Sample Location", style="magenta")

        limit = len(refs) if show_all else 20
        for symbol_id, ref_list in list(refs.items())[:limit]:
            row = [
                format_symbol_id(symbol_id)[:16] + "...",
                str(len(ref_list))
            ]
            if verbose:
                kinds = set(format_ref_kind(r.kind) for r in ref_list)
                files = set(
                    r.location.file_uri for r in ref_list if r.location)
                sample_loc = ""
                if ref_list and ref_list[0].location:
                    loc = ref_list[0].location
                    sample_loc = f"{os.path.basename(loc.file_uri)}:{loc.start_line}:{loc.start_column}"
                row.extend([
                    ", ".join(kinds)[:50],
                    f"{len(files)} file(s)",
                    sample_loc
                ])
            table.add_row(*row)

        if not show_all and len(refs) > 20:
            extra_cols = 3 if verbose else 0
            table.add_row("...", f"({len(refs) - 20} more)",
                          *["..." for _ in range(extra_cols)])

        self.console.print(table)

    def _show_relations(self, relations: List[Relation], show_all: bool = False):
        """Display relations"""
        table = Table(title=f"🔗 Relations ({len(relations)} total)")
        table.add_column("Subject", style="cyan", no_wrap=True)
        table.add_column("Predicate", style="yellow")
        table.add_column("Object", style="green", no_wrap=True)

        limit = len(relations) if show_all else 20
        for rel in relations[:limit]:
            table.add_row(
                format_symbol_id(rel.subject)[:16] + "...",
                rel.predicate.name,
                format_symbol_id(rel.object)[:16] + "..."
            )

        if not show_all and len(relations) > 20:
            table.add_row("...", f"({len(relations) - 20} more)", "...")

        self.console.print(table)

    def _show_sources(self, sources: List[IncludeGraphNode], verbose: bool, show_all: bool = False):
        """Display include graph"""
        table = Table(title=f"📂 Include Graph ({len(sources)} files)")
        table.add_column("File", style="blue")
        table.add_column("Flags", style="yellow")
        table.add_column("Includes", justify="right")
        if verbose:
            table.add_column("Digest", style="cyan", no_wrap=True)
            table.add_column("Full URI", style="white")

        limit = len(sources) if show_all else 20
        for node in sources[:limit]:
            row = [
                os.path.basename(node.uri) if node.uri else "(unknown)",
                ", ".join(format_source_flags(node.flags)) or "None",
                str(len(node.direct_includes))
            ]
            if verbose:
                row.extend([
                    node.digest.hex()[:16] + "...",
                    node.uri[:60] + "..." if len(node.uri) > 60 else node.uri
                ])
            table.add_row(*row)

        if not show_all and len(sources) > 20:
            extra_cols = 2 if verbose else 0
            table.add_row("...", f"({len(sources) - 20} more)",
                          "...", *["..." for _ in range(extra_cols)])

        self.console.print(table)

    def _show_command(self, command: Tuple[str, List[str]]):
        """Display compile command"""
        directory, args = command

        # Format command line
        cmd_line = " ".join(args) if len(args) < 10 else " ".join(
            args[:10]) + f" ... ({len(args) - 10} more args)"

        panel_content = f"""[bold]Directory:[/bold] {directory}
[bold]Command:[/bold] {cmd_line}
[bold]Total Args:[/bold] {len(args)}"""

        self.console.print(
            Panel(panel_content, title="⚙️ Compile Command", expand=False))


class RawFormatter:
    """Format output as raw JSON"""

    def format(self, parser: IdxFileParser) -> str:
        """Format the parsed index file as JSON"""
        data = {
            'file_info': parser.get_file_info(),
            'string_table': {
                'count': len(parser.string_table.strings),
                'total_bytes': sum(len(s.encode('utf-8')) + 1 for s in parser.string_table.strings)
            }
        }

        # Symbols
        symbols = parser.parse_symbols()
        if symbols:
            data['symbols'] = [
                {
                    'id': format_symbol_id(s.id),
                    'kind': s.kind.name,
                    'language': s.language.name,
                    'name': s.name,
                    'scope': s.scope,
                    'template_args': s.template_specialization_args,
                    'definition': asdict(s.definition) if s.definition else None,
                    'declaration': asdict(s.canonical_declaration) if s.canonical_declaration else None,
                    'references': s.references,
                    'flags': format_flags(s.flags),
                    'signature': s.signature,
                    'snippet': s.completion_snippet_suffix,
                    'documentation': s.documentation,
                    'return_type': s.return_type,
                    'type': s.type,
                    'include_headers': [
                        {
                            'header': inc.header,
                            'references': inc.references,
                            'directives': inc.supported_directives
                        }
                        for inc in s.include_headers
                    ]
                }
                for s in symbols
            ]

        # References
        refs = parser.parse_refs()
        if refs:
            data['references'] = {
                format_symbol_id(symbol_id): [
                    {
                        'kind': format_ref_kind(r.kind),
                        'location': asdict(r.location) if r.location else None,
                        'container': format_symbol_id(r.container) if r.container else None
                    }
                    for r in ref_list
                ]
                for symbol_id, ref_list in refs.items()
            }

        # Relations
        relations = parser.parse_relations()
        if relations:
            data['relations'] = [
                {
                    'subject': format_symbol_id(r.subject),
                    'predicate': r.predicate.name,
                    'object': format_symbol_id(r.object)
                }
                for r in relations
            ]

        # Include graph
        sources = parser.parse_sources()
        if sources:
            data['include_graph'] = [
                {
                    'uri': node.uri,
                    'flags': format_source_flags(node.flags),
                    'digest': node.digest.hex(),
                    'includes': node.direct_includes
                }
                for node in sources
            ]

        # Compile command
        command = parser.parse_command()
        if command:
            data['compile_command'] = {
                'directory': command[0],
                'arguments': command[1]
            }

        return json.dumps(data, indent=2)


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(
        description='Parse and display contents of clangd .idx files',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s index_file.12345678.idx
  %(prog)s --raw index_file.12345678.idx > output.json
  %(prog)s --verbose index_file.12345678.idx
  %(prog)s --all --verbose index_file.12345678.idx
        """
    )

    parser.add_argument('idx_file', help='Path to the .idx file')
    parser.add_argument('--raw', action='store_true',
                        help='Output raw JSON instead of pretty formatting')
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='Show additional details')
    parser.add_argument('--all', action='store_true',
                        help='Show all symbols/references without truncation')
    parser.add_argument('--summary', action='store_true',
                        help='Show only a summary of the index file contents')

    args = parser.parse_args()

    # Check if file exists
    if not os.path.exists(args.idx_file):
        print(f"Error: File not found: {args.idx_file}", file=sys.stderr)
        sys.exit(1)

    try:
        # Parse the index file
        idx_parser = IdxFileParser(args.idx_file)

        if args.raw:
            # Raw JSON output
            formatter = RawFormatter()
            print(formatter.format(idx_parser))
        elif args.summary:
            # Summary mode
            if not RICH_AVAILABLE:
                print(
                    "Warning: 'rich' library not available. Install it for pretty output.", file=sys.stderr)
                # Fallback to simple summary
                file_info = idx_parser.get_file_info()
                symbols = idx_parser.parse_symbols()
                refs = idx_parser.parse_refs()
                relations = idx_parser.parse_relations()
                sources = idx_parser.parse_sources()

                print(f"\nFile: {file_info['filename']}")
                print(f"Format Version: {file_info['format_version']}")
                print(f"Symbols: {len(symbols)}")
                print(
                    f"References: {sum(len(r) for r in refs.values())} total in {len(refs)} symbols")
                print(f"Relations: {len(relations)}")
                print(f"Include Graph: {len(sources)} files")
            else:
                formatter = PrettyFormatter()
                formatter.format_summary(idx_parser)
        else:
            # Pretty output
            if not RICH_AVAILABLE:
                print(
                    "Warning: 'rich' library not available. Install it for pretty output.", file=sys.stderr)
                print("Falling back to raw JSON output.\n", file=sys.stderr)
                formatter = RawFormatter()
                print(formatter.format(idx_parser))
            else:
                formatter = PrettyFormatter()
                formatter.format(idx_parser, args.verbose, args.all)

    except Exception as e:
        print(f"Error parsing index file: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    main()
