# MCP C++ End-to-End Testing Framework

## Overview

This comprehensive E2E testing framework provides seamless integration testing for the MCP C++ server with intelligent test identification, logging, and debugging capabilities.

## Quick Start

### Prerequisites

```bash
# Build the MCP server first
cd ../..
cargo build

# Install E2E test dependencies
cd test/e2e
npm install
```

#### System Requirements

- **clangd version 20**: Required for consistent LSP testing
  - Ubuntu/Debian: 
    ```bash
    # Add LLVM APT repository
    wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key | sudo apt-key add -
    echo "deb http://apt.llvm.org/jammy/ llvm-toolchain-jammy-20 main" | sudo tee /etc/apt/sources.list.d/llvm.list
    sudo apt-get update
    sudo apt-get install clangd-20
    ```
  - The framework is configured to use clangd-20 by default via `.env` file
  - CI environment automatically installs and configures clangd-20

#### Environment Configuration

The E2E tests use a `.env` file to configure the testing environment. Create one based on the example:

```bash
# Copy the example file and customize as needed
cp .env.example .env
```

Example `.env` configuration:
```env
# .env (in test/e2e directory)
CLANGD_PATH=/usr/bin/clangd-20
```

You can override these settings by:
1. Creating/modifying the `.env` file for permanent changes
2. Setting environment variables before running tests:
   ```bash
   CLANGD_PATH=/usr/bin/clangd-19 npm test
   ```

**Note**: The `.env` file is gitignored and created automatically in CI environments.

### Running Tests

```bash
# Run all E2E tests
npm test

# Run with UI interface
npm run test:ui

# Run with coverage
npm run test:coverage

# Run tests continuously (watch mode)
npm run test:watch
```

## Test Identification System

### The Problem We Solved

Previously, test temp folders were named with random UUIDs like `test-project-029e7d5a-a9e5-4634-8911-17cad9cacd28`, making it impossible to identify which test created which folder or logs.

### The Solution

Our enhanced system provides:

#### 1. **Descriptive Folder Names**
- **Before**: `test-project-029e7d5a-a9e5-4634-8911-17cad9cacd28`
- **After**: `list-build-dirs-test-029e7d5a` (test name + short UUID)

#### 2. **Test-Aware Log Files**
- **Before**: `mcp-cpp-server.log`, `mcp-cpp-clangd.log`
- **After**: `mcp-cpp-server-list-build-dirs-test.log`, `mcp-cpp-clangd-list-build-dirs-test.log`

#### 3. **Test Metadata Files**
Each temp folder contains `.test-info.json` with comprehensive test context:
```json
{
  "testName": "list-build-dirs-test",
  "describe": "List Build Dirs Tool",
  "timestamp": 1642680000000,
  "testId": "list-build-dirs-test-1642680000000",
  "uuid": "029e7d5a-a9e5-4634-8911-17cad9cacd28",
  "projectPath": "/path/to/temp/folder",
  "createdAt": "2024-01-20T12:00:00.000Z",
  "nodeVersion": "v18.17.0",
  "platform": "linux"
}
```

## Test Directory Inspector

### Basic Usage

```bash
# Inspect all test directories
npm run inspect

# Detailed view with metadata
npm run inspect:verbose

# Show log file details
npm run inspect:logs

# Preview cleanup (dry run)
npm run cleanup:dry

# Actually cleanup test directories
npm run cleanup
```

### Example Output

```
🔍 Inspecting test directories...

📝 list-build-dirs-test-029e7d5a
   Path: /path/to/temp/list-build-dirs-test-029e7d5a
   Size: 2.3 MB
   Created: 1/20/2024, 12:00:00 PM
   Modified: 1/20/2024, 12:01:30 PM
   Test: list-build-dirs-test
   Suite: List Build Dirs Tool
   ID: list-build-dirs-test-1642680000000
   📋 Log files:
     • mcp-cpp-server-list-build-dirs-test.log: 45.2 KB (342 lines)
     • mcp-cpp-clangd-list-build-dirs-test.log: 12.8 KB (89 lines)

🔍📝 symbol-search-test-1a2b3c4d
   Path: /path/to/temp/symbol-search-test-1a2b3c4d
   Size: 1.8 MB
   Created: 1/20/2024, 11:45:00 AM
   Modified: 1/20/2024, 11:46:15 AM
   Test: symbol-search-test
   Suite: Symbol Search Tool
   🔍 PRESERVED FOR DEBUGGING
   Reason: Test failed - investigating symbol resolution
   Preserved: 1/20/2024, 11:46:15 AM

📊 SUMMARY
   Total directories: 2
   With metadata: 2
   Preserved for debugging: 1
   With errors: 0
   Total size: 4.1 MB
```

### Icons Reference

- 📝 = Directory with test metadata
- ❓ = Directory without metadata
- 🔍 = Directory preserved for debugging
- ❌ = Directory with errors

## Writing Tests

### Option 1: Enhanced Helper Functions (Recommended)

```typescript
import { TestHelpers } from '../framework/TestHelpers.js';

describe('My Test Suite', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    const setup = await TestHelpers.setupTest({
      template: 'base',
      testName: 'my-test-name',
      describe: 'My Test Suite',
      logLevel: 'info'
    });
    
    project = setup.project;
    client = setup.client;
  });

  afterEach(async () => {
    await TestHelpers.cleanup(client, project);
  });

  it('should do something', async () => {
    // Your test logic here
    const result = await client.callTool('list_build_dirs');
    expect(result.content).toBeDefined();
  });
});
```

### Option 2: Manual Context Creation

```typescript
import { TestUtils } from '../framework/TestUtils.js';

describe('My Test Suite', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    const testContext = TestUtils.createTestContext('my-test-name', 'My Test Suite');
    project = await TestProject.fromBaseProject(undefined, testContext);
    
    const serverPath = await TestUtils.findMcpServer();
    const logEnv = TestUtils.createTestEnvironment(
      project.getProjectPath(),
      testContext.testName,
      'warn'
    );

    client = new McpClient(serverPath, {
      workingDirectory: project.getProjectPath(),
      timeout: 15000,
      env: logEnv.env,
    });
    
    await client.start();
  });

  afterEach(async () => {
    await client.stop();
    await project.cleanup();
  });
});
```

## Debugging Failed Tests

### Preserve Test Artifacts

```typescript
it('should handle complex operation', async () => {
  try {
    // Test logic that might fail
    await project.runCmake();
    const result = await client.callTool('complex_operation');
    expect(result.content).toBeDefined();
  } catch (error) {
    // Preserve the test folder for debugging
    await TestHelpers.preserveForDebugging(project, `Test failed: ${error.message}`);
    throw error;
  }
});
```

### Inspect Preserved Directories

```bash
# See which directories are preserved
npm run inspect

# Look for the 🔍 icon indicating preserved directories
# Then manually investigate the logs and files
```

## Framework Components

### TestProject Class

Enhanced project management with test context:

```typescript
// Create project with test context
const project = await TestProject.fromBaseProject(options, testContext);

// Get test metadata
const metadata = await project.getTestMetadata();

// Preserve for debugging
await project.preserveForDebugging("Investigation needed");
```

### McpClient Class

MCP server communication with test-aware logging:

```typescript
const client = new McpClient(serverPath, {
  workingDirectory: project.getProjectPath(),
  timeout: 15000,
  env: logEnv.env, // Test-aware environment
});
```

### TestUtils Class

Utility functions for test setup:

```typescript
// Create test environment with logging
const { env, serverLogPath, clangdLogPath } = TestUtils.createTestEnvironment(
  projectPath,
  testName,
  'warn'
);

// Create test context
const testContext = TestUtils.createTestContext('my-test', 'My Suite');
```

## Directory Structure

After running tests, you'll see:

```
temp/
├── list-build-dirs-test-029e7d5a/
│   ├── .test-info.json                           # Test metadata
│   ├── mcp-cpp-server-list-build-dirs-test.log  # Server logs
│   ├── mcp-cpp-clangd-list-build-dirs-test.log  # Clangd logs
│   ├── CMakeLists.txt                           # Project files
│   ├── src/
│   ├── include/
│   └── build-debug/
├── symbol-search-test-1a2b3c4d/
│   ├── .test-info.json
│   ├── .debug-preserved.json                    # Debug preservation info
│   ├── mcp-cpp-server-symbol-search-test.log
│   ├── mcp-cpp-clangd-symbol-search-test.log
│   └── ...
└── ...
```

## Available Templates

### Base Template (Default)
Full C++ project with:
- CMakeLists.txt
- Source files (src/)
- Header files (include/)
- Multiple build configurations

### Empty Template
Minimal temp directory for custom project setup

### Minimal CMake Template
Basic CMake project with:
- Simple CMakeLists.txt
- Single main.cpp file

## CI/CD Integration

### GitHub Actions

```yaml
- name: Run E2E Tests
  run: |
    cd test/e2e
    npm install
    npm run test:full
    
- name: Inspect Test Results
  if: failure()
  run: |
    cd test/e2e
    npm run inspect:verbose
```

### Environment Variables

The framework supports these environment variables:

- `MCP_SERVER_PATH`: Custom path to MCP server binary
- `MCP_TEST_NAME`: Current test name (auto-set)
- `MCP_TEST_ID`: Unique test identifier (auto-set)
- `MCP_LOG_FILE`: Server log file path (auto-set)
- `RUST_LOG`: Rust logging level

## Performance Considerations

### Test Isolation
- Each test gets its own temp directory
- Automatic cleanup after successful tests
- Log files are test-specific

### Resource Management
- Proper cleanup of MCP server processes
- Automatic temp directory cleanup
- Configurable timeouts

### Build Management
- Smart build directory detection
- CMake cache management
- Compilation database generation

## Troubleshooting

### Common Issues

1. **MCP server not found**
   ```bash
   # Build the server first
   cd ../..
   cargo build
   ```

2. **Test folders not cleaning up**
   ```bash
   # Manual cleanup
   npm run cleanup
   ```

3. **Port conflicts**
   - Each test uses stdio transport (no ports)
   - Tests are isolated by process

### Debug Logging

```typescript
// Enable debug logging
const setup = await TestHelpers.setupTest({
  logLevel: 'debug',
  testName: 'my-debug-test'
});
```

### Log Analysis

```bash
# Check specific test logs
npm run inspect:logs

# Look for ERROR or WARN entries in the output
```

## Best Practices

1. **Always use test context** for new tests
2. **Preserve failed tests** for debugging
3. **Use descriptive test names** for easy identification
4. **Clean up regularly** to avoid disk space issues
5. **Check logs** when tests fail unexpectedly

## Migration Guide

### Updating Existing Tests

Replace this:
```typescript
beforeEach(async () => {
  project = await TestProject.fromBaseProject();
  client = new McpClient(serverPath, {
    workingDirectory: project.getProjectPath(),
    timeout: 15000,
  });
});
```

With this:
```typescript
beforeEach(async () => {
  const setup = await TestHelpers.setupTest({
    testName: 'my-test',
    describe: 'My Test Suite'
  });
  project = setup.project;
  client = setup.client;
});
```

## Advanced Features

### Custom Test Templates

```typescript
// Create custom project structure
const project = await TestProject.fromTemplate('empty', testContext);
await project.writeFile('custom.cpp', customCode);
await project.runCmake({ options: { CUSTOM_FLAG: 'ON' } });
```

### Log Analysis

```typescript
// Read and analyze logs
const logEntries = await TestUtils.readLogFile(serverLogPath);
const analysis = TestUtils.analyzeLogEntries(logEntries);
expect(analysis.errors.length).toBe(0);
```

### Multi-Configuration Testing

```typescript
// Test different build configurations
await project.switchBuildConfig(BuildConfiguration.RELEASE);
await project.runCmake({ buildType: 'Release' });
```

This framework provides a robust foundation for testing the MCP C++ server with full traceability and debugging capabilities.