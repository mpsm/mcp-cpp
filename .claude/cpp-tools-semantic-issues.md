# C++ MCP Tools - Semantic Analysis Issues

## Overview
Test results from running 15+ search and analyze operations to validate LSP integration stability and semantic accuracy.

## ✅ Stability Results
- **No timeouts or hangs detected** - stderr reading fix is working
- **All 15 test calls completed successfully**
- **clangd integration is stable** across multiple concurrent requests
- **Mixed parameter combinations tested** (external symbols, inheritance, call hierarchy, usage patterns)

## ⚠️ Semantic Issues Identified

### 1. Symbol Kind Classification Issues

**Problem**: Class methods incorrectly classified as functions
- `TestProject::IStorageBackend::getBackendType` shows `kind: "function"` → should be `"method"`
- `TestProject::FileStorage::getBackendType` shows `kind: "function"` → should be `"method"`

**Impact**: Makes it harder to distinguish between free functions and class members

### 2. Symbol Search Filtering Problems

**Problem**: Kind-based filtering not working correctly
- Search for `kinds: ["class"]` returns 0 results
- Search for `kinds: ["namespace"]` returns 0 results  
- But individual name searches find these symbols successfully

**Impact**: Users can't reliably filter by symbol types

### 3. Template Parameter Parsing Issues

**Problem**: Malformed template parameter strings
```json
"template_parameters": ["char> &)`\n- `const std::string & value (aka const basic_string<char"]
```

**Expected**: Clean parameter lists like `["char"]` or `["_Tp", "_Alloc"]`

**Impact**: Template information is unusable

### 4. Standard Library Symbol Classification

**Problem**: `std::string` returns `kind: "qualified_name"` 
**Expected**: `kind: "class"` or `kind: "typedef"`

**Impact**: Inconsistent classification of well-known types

### 5. Call Hierarchy Gaps

**Constructor/Destructor Tracking**:
- No callers found for `FileStorage()`, `~FileStorage()`, etc.
- These must be called somewhere but aren't tracked

**Polymorphic Call Tracking**:
- `IStorageBackend::getBackendType` shows caller from `main`
- `FileStorage::getBackendType` and `MemoryStorage::getBackendType` show no callers
- Virtual dispatch calls not being tracked properly

**Impact**: Incomplete understanding of object lifecycle and polymorphic behavior

### 6. Type Qualifier Inconsistencies

**Problem**: Incorrect type qualifiers on function signatures
- Functions showing `is_const: true` and `is_reference: true` that don't match their actual signatures

**Impact**: Misleading information about function characteristics

## 🔧 Recommended Fixes

### High Priority
1. **Fix symbol kind classification** - ensure methods are classified as "method", not "function"
2. **Fix search filtering** - make `kinds` parameter work correctly for classes, namespaces, etc.
3. **Improve template parameter parsing** - clean up malformed template strings

### Medium Priority
4. **Enhance call hierarchy tracking** for constructors/destructors and virtual dispatch
5. **Standardize type qualifiers** - ensure `is_const`, `is_reference` flags are accurate
6. **Improve standard library symbol handling** - consistent classification for `std::` types

### Low Priority
7. **Add semantic validation tests** to catch regressions
8. **Document known limitations** in polymorphic call tracking

## Test Files Used
- **Main codebase**: `/home/ubuntu/projects/mcp-cpp/test/test-project/`
- **Test commands**: Various combinations of `search_symbols` and `analyze_symbol_context`
- **Date**: 2025-01-16

## Next Steps
- [ ] Create unit tests for each semantic issue
- [ ] Prioritize fixes based on user impact
- [ ] Add regression testing for symbol classification
- [ ] Consider clangd version compatibility issues