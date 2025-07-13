import { describe, it, expect, beforeEach, afterEach, beforeAll } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject, ProjectTemplate, BuildConfiguration } from '../framework/TestProject.js';
import * as path from 'path';

describe('Parameter Parsing Tests', () => {
  let client: McpClient;
  let project: TestProject;
  const serverPath = path.resolve(
    process.cwd(),
    '../../target/debug/mcp-cpp-server'
  );

  beforeAll(async () => {
    // Check that the MCP server binary exists
    const { access } = await import('fs/promises');

    try {
      await access(serverPath);
    } catch {
      throw new Error(
        `MCP server binary not found at ${serverPath}. ` +
          `Please build the project first: cd ../.. && cargo build`
      );
    }
  });

  beforeEach(async () => {
    project = await TestProject.fromBaseProject({
      enableDebugLogging: true,
      enableMemoryStorage: true,
      buildType: BuildConfiguration.DEBUG
    });

    // Configure debug build
    await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

    // Create client with working directory set to project path
    client = new McpClient(serverPath, 10000, project.projectPath);
    await client.start();

    // Setup clangd first
    const setupResponse = await client.callTool('setup_clangd', {
      buildDirectory: 'build'
    });
    const setupResult = JSON.parse(setupResponse.content[0].text);
    expect(setupResult.success).toBe(true);
  });

  afterEach(async () => {
    if (client) {
      await client.stop();
    }
    if (project) {
      await project.cleanup();
    }
  });

  describe('Parameter format handling', () => {
    it('should handle parameters as JSON objects (current E2E test format)', async () => {
      // This is how our current E2E tests work - params as proper objects
      const response = await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {}
        }
      });

      const result = JSON.parse(response.content[0].text);

      expect(result.success).toBe(true);
      expect(result.method).toBe('initialize');
      expect(result.result).toBeDefined();
      expect(result.result.capabilities).toBeDefined();
    });

    it('should handle parameters as JSON strings (simulates problematic case from logs)', async () => {
      // This simulates the case from the original log where params were passed as JSON strings
      // Our custom deserializer should parse this correctly
      const paramsAsString = JSON.stringify({
        processId: null,
        rootUri: `file://${project.projectPath}`,
        capabilities: {}
      });

      // Note: This test simulates sending a JSON string as the params value
      // In practice, this would happen at the MCP protocol level
      const response = await client.callTool('lsp_request', {
        method: 'initialize',
        params: paramsAsString as any // Force TypeScript to accept string
      });

      const result = JSON.parse(response.content[0].text);

      expect(result.success).toBe(true);
      expect(result.method).toBe('initialize');
      expect(result.result).toBeDefined();
      expect(result.result.capabilities).toBeDefined();
    });

    it('should handle notification methods (no params needed)', async () => {
      // Initialize first
      await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {}
        }
      });

      // Send initialized notification (no params needed)
      const response = await client.callTool('lsp_request', {
        method: 'initialized',
        params: {}
      });

      const result = JSON.parse(response.content[0].text);

      expect(result.success).toBe(true);
      expect(result.method).toBe('initialized');
      expect(result.message).toContain('successfully');
    });

    it('should handle invalid JSON strings gracefully', async () => {
      // This tests what happens when params is an invalid JSON string
      // Our deserializer should log a warning and use the string as-is
      const response = await client.callTool('lsp_request', {
        method: 'invalidMethod',
        params: 'not-valid-json-{' as any
      });

      const result = JSON.parse(response.content[0].text);

      // Should fail but not crash
      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.method).toBe('invalidMethod');
    });
  });

  describe('Reference workflow demonstration', () => {
    it('should demonstrate complete successful workflow with object params', async () => {
      // 1. Initialize LSP
      const initResponse = await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {
            textDocument: {
              hover: { dynamicRegistration: false },
              completion: { dynamicRegistration: false }
            }
          }
        }
      });
      const initResult = JSON.parse(initResponse.content[0].text);
      expect(initResult.success).toBe(true);

      // 2. Send initialized notification
      const initializedResponse = await client.callTool('lsp_request', {
        method: 'initialized',
        params: {}
      });
      const initializedResult = JSON.parse(initializedResponse.content[0].text);
      expect(initializedResult.success).toBe(true);

      // 3. Open a document
      const mainPath = path.join(project.projectPath, 'src/main.cpp');
      const mainContent = await project.readFile('src/main.cpp');
      const didOpenResponse = await client.callTool('lsp_request', {
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
            languageId: 'cpp',
            version: 1,
            text: mainContent
          }
        }
      });
      const didOpenResult = JSON.parse(didOpenResponse.content[0].text);
      expect(didOpenResult.success).toBe(true);

      // 4. Request document symbols
      const symbolsResponse = await client.callTool('lsp_request', {
        method: 'textDocument/documentSymbol',
        params: {
          textDocument: {
            uri: `file://${mainPath}`
          }
        }
      });
      const symbolsResult = JSON.parse(symbolsResponse.content[0].text);
      expect(symbolsResult.success).toBe(true);
      expect(Array.isArray(symbolsResult.result)).toBe(true);
    });
  });
});