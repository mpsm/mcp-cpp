# clangd Index File Format - Complete Implementation Specification

**Version-specific binary format specification for external parser implementation**

## Format Version Matrix & Compatibility

| Format Version | LLVM Versions | Hash Function | Exact Binary Format Changes |
|----------------|---------------|---------------|------------------------------|
| 12 | 10.0.1 | xxHash64 | Baseline format |
| 13 | 11.1.0 | xxHash64 | **Ref struct**: Added Container field (8 bytes SymbolID after Location) |
| 14 | - | xxHash64 | **No format change** - version bump for ref structure change invalidation |
| 15 | - | xxHash64 | **No format change** - version bump for string table corruption detection |
| 16 | 12.0.1, 13.0.1 | xxHash64 | **No format change** - version bump for symbol collector changes |
| 17 | 14.0.6, 15.0.7 | xxHash64 | **No format change** - version bump for readRIFF API signature change |
| 18 | 16.0.6, 17.0.6 | xxHash64 | **IncludeHeaderWithReferences**: Changed References from varint to packed varint (References << 2 \| SupportedDirectives) |
| 19 | 18.1.8, 19.1.7 | **xxh3_64bits** | **Hash function change** - file naming and content hashing algorithm |
| 20 | 20.1.8 | xxh3_64bits | **No format change** - version bump to force rebuild after SymbolID generation changes |

**CRITICAL**: No backward compatibility. Version mismatch = complete rejection.

## RIFF Container Format

```
RIFF Header (12 bytes):
  Magic: "RIFF" (4 bytes, ASCII)
  FileSize: uint32_le (remaining bytes in file)
  Type: "CdIx" (4 bytes, ASCII) - clangd index identifier

Chunk Format:
  ChunkID: 4 bytes ASCII
  ChunkSize: uint32_le (data bytes, excluding header)
  ChunkData: variable length
  Padding: to even boundary if needed
```

## Data Encoding Primitives

### Variable-Length Integer (varint)
```cpp
// Encoding: bottom 7 bits = data, top bit = continuation
// Little-endian, 1-5 bytes for uint32
// Examples:
//   0x1A      -> [0x1A]
//   0x2F9A    -> [0x9A, 0x2F] = [0x1A|0x80, 0x2F]
//   0x12345678 -> [0xF8, 0xAC, 0xD1, 0x91, 0x01]
```

### Position Encoding
```cpp
// Packed 32-bit: upper 20 bits = line, lower 12 bits = column
// Both 0-based
// Max line: 1,048,575 (0xFFFFF)
// Max column: 4,095 (0xFFF)
// Column units: UTF-16 code units (or bytes if UTF-8 negotiated)
```

## Enum Definitions

### SymbolKind (uint8_t)
```cpp
Unknown = 0,         Module = 1,           Namespace = 2,       NamespaceAlias = 3,
Macro = 4,           Enum = 5,             Struct = 6,          Class = 7,
Protocol = 8,        Extension = 9,        Union = 10,          TypeAlias = 11,
Function = 12,       Variable = 13,        Field = 14,          EnumConstant = 15,
InstanceMethod = 16, ClassMethod = 17,     StaticMethod = 18,   InstanceProperty = 19,
ClassProperty = 20,  StaticProperty = 21,  Constructor = 22,    Destructor = 23,
ConversionFunction = 24, Parameter = 25,   Using = 26,          TemplateTypeParm = 27,
TemplateTemplateParm = 28, NonTypeTemplateParm = 29, Concept = 30
```

### SymbolLanguage (uint8_t)
```cpp
C = 0, ObjC = 1, CXX = 2, Swift = 3
```

### RefKind (uint8_t)
```cpp
Unknown = 0, Declaration = 1, Definition = 2, Reference = 4, Spelled = 8, Call = 16
```

### RelationKind (uint8_t)
```cpp
BaseOf = 0, OverriddenBy = 1
```

### SymbolFlag (uint8_t) - Bitmask
```cpp
None = 0, IndexedForCodeCompletion = 1, Deprecated = 2, ImplementationDetail = 4,
VisibleOutsideFile = 8, HasDocComment = 16
```

### IncludeDirective (uint8_t) - Bitmask
```cpp
Invalid = 0, Include = 1, Import = 2
```

### IncludeGraphNode::SourceFlag (uint8_t) - Bitmask
```cpp
None = 0, IsTU = 1, HadErrors = 2
```

## Chunk Specifications

### `meta` Chunk - Always Present
```
FormatVersion: uint32_le
```

### `stri` Chunk - String Table - Always Present
```
UncompressedSize: uint32_le
  If 0: raw data follows
  If >0: zlib compressed data follows, decompresses to this size

String Data:
  Sequence of null-terminated strings: "str1\0str2\0str3\0"
  Sorted alphabetically for compression
  Empty string always at index 0
```

### `symb` Chunk - Symbols - Optional
```
Sequence of Symbol records:

SymbolID: 8 bytes (truncated SHA1 of USR)
SymbolKind: uint8 (see enum)
SymbolLanguage: uint8 (see enum)
Name: varint (string table index)
Scope: varint (string table index)
TemplateSpecializationArgs: varint (string table index)
Definition: SymbolLocation
CanonicalDeclaration: SymbolLocation
References: varint (reference count)
Flags: uint8 (SymbolFlag bitmask)
Signature: varint (string table index)
CompletionSnippetSuffix: varint (string table index)
Documentation: varint (string table index)
ReturnType: varint (string table index)
Type: varint (string table index)
IncludeHeaderCount: varint
IncludeHeaders[Count]: IncludeHeaderWithReferences records
```

### `refs` Chunk - References - Optional
```
Sequence of Reference Groups:

SymbolID: 8 bytes
RefCount: varint
Refs[Count]:
  Kind: uint8 (RefKind)
  Location: SymbolLocation
  Container: 8 bytes (SymbolID) - ONLY in format versions 13+
```

**Format-specific parsing:**
- **Versions 12**: No Container field
- **Versions 13+**: Container field present (8 bytes SymbolID)

### `rela` Chunk - Relations - Optional
```
Sequence of Relation records:

Subject: 8 bytes (SymbolID)
Predicate: uint8 (RelationKind)
Object: 8 bytes (SymbolID)
```

### `srcs` Chunk - Include Graph - Optional
```
Sequence of IncludeGraphNode records:

Flags: uint8 (IncludeGraphNode::SourceFlag)
URI: varint (string table index)
Digest: 8 bytes (content hash for staleness detection)
DirectIncludeCount: varint
DirectIncludes[Count]: varint (string table index for each)
```

### `cmdl` Chunk - Compile Command - Optional
```
Directory: varint (string table index)
CommandLineCount: varint
CommandLine[Count]: varint (string table index for each argument)
```

## Complex Structure Encodings

### SymbolLocation
```
FileURI: varint (string table index)
StartLine: varint
StartColumn: varint
EndLine: varint
EndColumn: varint
```

### IncludeHeaderWithReferences
```
IncludeHeader: varint (string table index)
Data: FORMAT-DEPENDENT
```

**Format-specific encoding:**
- **Versions 12-17**: `References: varint (32-bit reference count)`
- **Versions 18+**: `PackedData: varint (References << 2 | SupportedDirectives)`
  - References: upper 30 bits (extract with `>> 2`)
  - SupportedDirectives: lower 2 bits (extract with `& 0x3`)

## SymbolID Generation Algorithm
```cpp
// Input: USR (Unified Symbol Resolution) string
// Algorithm:
1. Compute SHA1 hash of USR string: hash = SHA1(USR)
2. Truncate to first 8 bytes: symbolID = hash[0:8]
3. Store in little-endian byte order
```

## File Naming Convention
```
Pattern: {basename}.{hash}.idx
basename: source file name (e.g., "main.cpp")
hash: hex-encoded hash of full file path
  Versions 12-18: xxHash64(full_path)
  Versions 19-20: xxh3_64bits(full_path)
```

## Content Staleness Detection
```cpp
// Stored in srcs chunk, Digest field (8 bytes)
// Hash of file content for cache invalidation
// Uses same hash function as file naming:
//   Versions 12-18: xxHash64(file_content)
//   Versions 19-20: xxh3_64bits(file_content)
```

## Hash Function Definitions

### xxHash64 (Versions 12-18)
```c
// Core constants
#define PRIME64_1   0x9E3779B185EBCA87ULL
#define PRIME64_2   0xC2B2AE3D27D4EB4FULL  
#define PRIME64_3   0x165667B19E3779F9ULL
#define PRIME64_4   0x85EBCA77C2B2AE63ULL
#define PRIME64_5   0x27D4EB2F165667C5ULL

uint64_t XXH64(const void* input, size_t len, uint64_t seed) {
    const uint8_t* p = (const uint8_t*)input;
    const uint8_t* const bEnd = p + len;
    uint64_t h64;

    if (len >= 32) {
        const uint8_t* const limit = bEnd - 32;
        uint64_t v1 = seed + PRIME64_1 + PRIME64_2;
        uint64_t v2 = seed + PRIME64_2;
        uint64_t v3 = seed + 0;
        uint64_t v4 = seed - PRIME64_1;

        do {
            v1 = XXH64_round(v1, XXH_read64(p)); p+=8;
            v2 = XXH64_round(v2, XXH_read64(p)); p+=8;
            v3 = XXH64_round(v3, XXH_read64(p)); p+=8;
            v4 = XXH64_round(v4, XXH_read64(p)); p+=8;
        } while (p <= limit);

        h64 = XXH_rotl64(v1,1) + XXH_rotl64(v2,7) + 
              XXH_rotl64(v3,12) + XXH_rotl64(v4,18);
        h64 = XXH64_mergeRound(h64, v1);
        h64 = XXH64_mergeRound(h64, v2);
        h64 = XXH64_mergeRound(h64, v3);
        h64 = XXH64_mergeRound(h64, v4);
    } else {
        h64 = seed + PRIME64_5;
    }

    h64 += (uint64_t) len;

    while (p+8 <= bEnd) {
        uint64_t k1 = XXH64_round(0, XXH_read64(p));
        h64 ^= k1;
        h64 = XXH_rotl64(h64,27) * PRIME64_1 + PRIME64_4;
        p+=8;
    }

    if (p+4 <= bEnd) {
        h64 ^= (uint64_t)(XXH_read32(p)) * PRIME64_1;
        h64 = XXH_rotl64(h64,23) * PRIME64_2 + PRIME64_3;
        p+=4;
    }

    while (p < bEnd) {
        h64 ^= (*p) * PRIME64_5;
        h64 = XXH_rotl64(h64,11) * PRIME64_1;
        p++;
    }

    h64 ^= h64 >> 33;
    h64 *= PRIME64_2;
    h64 ^= h64 >> 29;
    h64 *= PRIME64_3;
    h64 ^= h64 >> 32;

    return h64;
}

// Helper functions
uint64_t XXH64_round(uint64_t acc, uint64_t input) {
    acc += input * PRIME64_2;
    acc = XXH_rotl64(acc, 31);
    acc *= PRIME64_1;
    return acc;
}

uint64_t XXH64_mergeRound(uint64_t acc, uint64_t val) {
    val = XXH64_round(0, val);
    acc ^= val;
    acc = acc * PRIME64_1 + PRIME64_4;
    return acc;
}

uint64_t XXH_rotl64(uint64_t x, int r) {
    return (x << r) | (x >> (64 - r));
}

uint64_t XXH_read64(const void* ptr) {
    // Little-endian read
    return *(const uint64_t*)ptr;  // Assumes aligned access
}

uint32_t XXH_read32(const void* ptr) {
    return *(const uint32_t*)ptr;
}
```

### xxh3_64bits (Versions 19-20)
```c
// 192-byte secret array for mixing
static const uint8_t kSecret[192] = {
    0xb8, 0xfe, 0x6c, 0x39, 0x23, 0xa4, 0x4b, 0xbe, 0x7c, 0x01, 0x81, 0x2c, 0xf7, 0x21, 0xad, 0x1c,
    0xde, 0xd4, 0x6d, 0xe9, 0x83, 0x90, 0x97, 0xdb, 0x72, 0x40, 0xa4, 0xa4, 0xb7, 0xb3, 0x67, 0x1f,
    0xcb, 0x79, 0xe6, 0x4e, 0xcc, 0xc0, 0xe5, 0x78, 0x82, 0x5a, 0xd0, 0x7d, 0xcc, 0xff, 0x72, 0x21,
    0xb8, 0x08, 0x46, 0x74, 0xf7, 0x43, 0x24, 0x8e, 0xe0, 0x35, 0x90, 0xe6, 0x81, 0x3a, 0x26, 0x4c,
    0x3c, 0x28, 0x52, 0xbb, 0x91, 0xc3, 0x00, 0xcb, 0x88, 0xd0, 0x65, 0x8b, 0x1b, 0x53, 0x2e, 0xa3,
    0x71, 0x64, 0x48, 0x97, 0xa2, 0x0d, 0xf9, 0x4e, 0x38, 0x19, 0xef, 0x46, 0xa9, 0xde, 0xac, 0xd8,
    0xa8, 0xfa, 0x76, 0x3f, 0xe3, 0x9c, 0x34, 0x3f, 0xf9, 0xdc, 0xbb, 0xc7, 0xc7, 0x0b, 0x4f, 0x1d,
    0x8a, 0x51, 0xe0, 0x4b, 0xcd, 0xb4, 0x59, 0x31, 0xc8, 0x9f, 0x7e, 0xc9, 0xd9, 0x78, 0x73, 0x64,
    0xea, 0xc5, 0xac, 0x83, 0x34, 0xd3, 0xeb, 0xc3, 0xc5, 0x81, 0xa0, 0xff, 0xfa, 0x13, 0x63, 0xeb,
    0x17, 0x0d, 0xdd, 0x51, 0xb7, 0xf0, 0xda, 0x49, 0xd3, 0x16, 0x55, 0x26, 0x29, 0xd4, 0x68, 0x9e,
    0x2b, 0x16, 0xbe, 0x58, 0x7d, 0x47, 0xa1, 0xfc, 0x8f, 0xf8, 0xb8, 0xd1, 0x7a, 0xd0, 0x31, 0xce,
    0x45, 0xcb, 0x3a, 0x8f, 0x95, 0x16, 0x04, 0x28, 0xaf, 0xd7, 0xfb, 0xca, 0xbb, 0x4b, 0x40, 0x7e
};

uint64_t XXH3_64bits(const void* input, size_t len) {
    if (len <= 16) return XXH3_len_0to16_64b(input, len, kSecret, 0);
    if (len <= 128) return XXH3_len_17to128_64b(input, len, kSecret, 0);
    if (len <= 240) return XXH3_len_129to240_64b(input, len, kSecret, 0);
    return XXH3_hashLong_64b(input, len, kSecret, sizeof(kSecret));
}

// Core mixing function for 16-byte blocks
uint64_t XXH3_mix16B(const uint8_t* input, const uint8_t* secret, uint64_t seed) {
    uint64_t input_lo = XXH_read64(input);
    uint64_t input_hi = XXH_read64(input+8);
    return XXH3_mul128_fold64(
        input_lo ^ (XXH_read64(secret) + seed),
        input_hi ^ (XXH_read64(secret+8) - seed)
    );
}

// 128-bit multiplication with folding to 64-bit
uint64_t XXH3_mul128_fold64(uint64_t lhs, uint64_t rhs) {
    __uint128_t product = (__uint128_t)lhs * rhs;
    return (uint64_t)product ^ (uint64_t)(product >> 64);
}

// Short input handling (0-16 bytes) 
uint64_t XXH3_len_0to16_64b(const void* data, size_t len, const uint8_t* secret, uint64_t seed) {
    if (len > 8) return XXH3_len_9to16_64b(data, len, secret, seed);
    if (len >= 4) return XXH3_len_4to8_64b(data, len, secret, seed);
    if (len) return XXH3_len_1to3_64b(data, len, secret, seed);
    return XXH64_avalanche(seed ^ (XXH_read64(secret+56) ^ XXH_read64(secret+64)));
}

// Final avalanche mixing
uint64_t XXH64_avalanche(uint64_t h64) {
    h64 ^= h64 >> 33;
    h64 *= 0xC2B2AE3D27D4EB4FULL;
    h64 ^= h64 >> 29;
    h64 *= 0x165667B19E3779F9ULL;
    h64 ^= h64 >> 32;
    return h64;
}
```

### Algorithm Comparison

| Feature | xxHash64 | xxh3_64bits |
|---------|----------|-------------|
| **Design** | Merkle-Damgård with 4-way parallel | SIMD-friendly with secret mixing |
| **Performance** | ~13-15 GB/s | ~25-35 GB/s (2-3x faster) |
| **Small inputs** | Good | Optimized (separate code paths) |
| **Quality** | Excellent | Superior avalanche properties |
| **Year** | 2012 | 2019 |
| **Usage in clangd** | File/content hashing (v12-18) | File/content hashing (v19-20) |

**Migration reason**: xxh3_64bits provides significantly better performance while maintaining excellent hash quality, crucial for clangd's intensive file hashing during background indexing.

## Format-Specific Parsing Strategies

### Strategy Pattern Implementation
```cpp
class FormatParser {
public:
  static std::unique_ptr<FormatParser> create(uint32_t version) {
    switch(version) {
      case 12: return std::make_unique<Format12Parser>();
      case 13: case 14: case 15: case 16: case 17: 
        return std::make_unique<Format13Plus17Parser>();
      case 18: case 19: case 20:
        return std::make_unique<Format18PlusParser>();
      default: return nullptr; // Unsupported version
    }
  }
  virtual bool parseRef(Reader& r, Ref& ref) = 0;
  virtual bool parseIncludeHeader(Reader& r, IncludeHeaderWithReferences& inc) = 0;
};

class Format12Parser : public FormatParser {
  bool parseRef(Reader& r, Ref& ref) override {
    ref.kind = r.consume8();
    ref.location = readLocation(r);
    // No container field in format 12
    return !r.err();
  }
  
  bool parseIncludeHeader(Reader& r, IncludeHeaderWithReferences& inc) override {
    inc.header = r.consumeString();
    inc.references = r.consumeVar();
    inc.supportedDirectives = Include; // Default to Include only
    return !r.err();
  }
};

class Format13Plus17Parser : public FormatParser {
  bool parseRef(Reader& r, Ref& ref) override {
    ref.kind = r.consume8();
    ref.location = readLocation(r);
    ref.container = r.consumeID(); // Added in format 13
    return !r.err();
  }
  
  bool parseIncludeHeader(Reader& r, IncludeHeaderWithReferences& inc) override {
    inc.header = r.consumeString();
    inc.references = r.consumeVar();
    inc.supportedDirectives = Include; // Default to Include only
    return !r.err();
  }
};

class Format18PlusParser : public FormatParser {
  bool parseRef(Reader& r, Ref& ref) override {
    ref.kind = r.consume8();
    ref.location = readLocation(r);
    ref.container = r.consumeID(); // Container field present
    return !r.err();
  }
  
  bool parseIncludeHeader(Reader& r, IncludeHeaderWithReferences& inc) override {
    inc.header = r.consumeString();
    uint32_t packed = r.consumeVar();
    inc.references = packed >> 2;                    // Upper 30 bits
    inc.supportedDirectives = packed & 0x3;          // Lower 2 bits
    return !r.err();
  }
};
```

### Version-Specific Hash Function Selection
```cpp
uint64_t computePathHash(const std::string& path, uint32_t formatVersion) {
  if (formatVersion <= 18) {
    return xxHash64(path);
  } else {
    return xxh3_64bits(path);
  }
}
```

## Error Handling

### Version Mismatch
```cpp
if (readVersion != expectedVersion) {
  return error("Format version mismatch: expected %d, got %d", 
               expectedVersion, readVersion);
}
```

### Required Chunks
```cpp
// meta and stri chunks are mandatory
// All others are optional and can be missing
if (!chunks.contains("meta") || !chunks.contains("stri")) {
  return error("Missing required chunks");
}
```

### String Table Validation
```cpp
// Check compression ratio plausibility
const int MAX_COMPRESSION_RATIO = 1032;
if (uncompressedSize / MAX_COMPRESSION_RATIO > compressedSize) {
  return error("Implausible compression ratio");
}
```

## Memory Layout Notes

- All multi-byte integers in little-endian format
- Chunks aligned to even boundaries with padding
- String table compression uses zlib when available
- SymbolIDs are raw 8-byte sequences (not null-terminated)
- All string references are indices into string table, never inline

## Implementation Checklist

### Required Components
- [ ] RIFF container parser
- [ ] Variable-length integer encoder/decoder  
- [ ] String table decompression (zlib)
- [ ] Format version detection and strategy selection
- [ ] All enum value mappings
- [ ] SymbolID generation (SHA1 + truncation)
- [ ] Position encoding/decoding (packed 32-bit)
- [ ] Hash function implementation (xxHash64 + xxh3_64bits)

### Version-Specific Handling
- [ ] Format 12: Baseline - Ref without Container, IncludeHeader with simple References
- [ ] Formats 13-17: Ref.Container field (8 bytes), IncludeHeader with simple References
- [ ] Formats 18-20: Ref.Container field, IncludeHeader with packed References+Directives
- [ ] Formats 19-20: Hash function change to xxh3_64bits (affects file naming and content hashing)

This specification provides complete implementation details for parsing all clangd index format versions 12-20 with proper version-specific strategy handling.