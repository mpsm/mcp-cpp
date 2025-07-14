import { describe, it, expect, beforeEach, afterEach, beforeAll } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import {
  TestProject,
  ProjectTemplate,
  BuildConfiguration,
} from '../framework/TestProject.js';
import * as path from 'path';

// Helper to extract text from MCP response
function getResponseText(response: { content: unknown[] }): string {
  const textContent = response.content[0] as { text: string };
  return textContent.text;
}

describe('LSP tools', () => {
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
    // Client will be created per test with specific working directory
  });

  afterEach(async () => {
    if (client) {
      await client.stop();
    }
    if (project) {
      await project.cleanup();
    }
  });

  describe('tool availability', () => {
    it('should list LSP tools', async () => {
      client = new McpClient(serverPath);
      await client.start();

      const tools = await client.listTools();

      expect(tools).toBeDefined();
      expect(Array.isArray(tools)).toBe(true);
      expect(tools.length).toBeGreaterThan(0);

      const setupClangdTool = tools.find(
        (tool) => tool.name === 'setup_clangd'
      );
      expect(setupClangdTool).toBeDefined();
      expect(setupClangdTool?.description).toContain('clangd');

      const lspRequestTool = tools.find((tool) => tool.name === 'lsp_request');
      expect(lspRequestTool).toBeDefined();
      expect(lspRequestTool?.description).toContain('LSP request');
    });
  });

  describe('setup_clangd tool', () => {
    it('should setup clangd successfully with valid build directory', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      // Create client with working directory set to project path
      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      const response = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });

      expect(response).toBeDefined();
      expect(response.content).toBeDefined();
      expect(Array.isArray(response.content)).toBe(true);
      expect(response.content.length).toBeGreaterThan(0);

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.message).toBeDefined();
      expect(result.build_directory).toContain('build');
      expect(result.compile_commands).toContain('compile_commands.json');
      expect(result.next_step).toContain('lsp_request');
    });

    it('should fail with non-existent build directory', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      const response = await client.callTool('setup_clangd', {
        buildDirectory: 'nonexistent-build',
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.workflow_reminder).toContain('cpp_project_status');
    });

    it('should fail with build directory missing compile_commands.json', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);

      // Create build directory but don't run cmake (no compile_commands.json)
      await project.createDirectory('build');

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      const response = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe('lsp_request tool', () => {
    it('should fail when clangd not setup', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      const response = await client.callTool('lsp_request', {
        method: 'initialize',
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(false);
      expect(result.error).toBe('clangd not setup');
      expect(result.workflow).toContain('setup_clangd');
    });

    it('should send initialize request successfully after setup', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd first
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Send initialize request
      const response = await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: null,
          capabilities: {},
        },
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.method).toBe('initialize');
      expect(result.result).toBeDefined();
    });

    it('should handle textDocument/hover request', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Initialize clangd
      await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {},
        },
      });

      // Initialized notification
      await client.callTool('lsp_request', {
        method: 'initialized',
        params: {},
      });

      // Open the file first
      const mainPath = path.join(project.projectPath, 'src/main.cpp');
      const mainContent = await project.readFile('src/main.cpp');
      await client.callTool('lsp_request', {
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
            languageId: 'cpp',
            version: 1,
            text: mainContent,
          },
        },
      });

      // Get hover information for Math::factorial function
      const response = await client.callTool('lsp_request', {
        method: 'textDocument/hover',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
          },
          position: {
            line: 17, // Math::factorial(n) line
            character: 52, // on 'factorial'
          },
        },
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.method).toBe('textDocument/hover');
      expect(result.result).toBeDefined();
    });

    it('should handle textDocument/definition request', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Initialize clangd
      await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {},
        },
      });

      await client.callTool('lsp_request', {
        method: 'initialized',
        params: {},
      });

      // Open the file first
      const mainPath = path.join(project.projectPath, 'src/main.cpp');
      const mainContent = await project.readFile('src/main.cpp');
      await client.callTool('lsp_request', {
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
            languageId: 'cpp',
            version: 1,
            text: mainContent,
          },
        },
      });

      // Request definition of 'Math::factorial' function call in main.cpp
      const response = await client.callTool('lsp_request', {
        method: 'textDocument/definition',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
          },
          position: {
            line: 17, // line with Math::factorial(n)
            character: 52, // on 'factorial'
          },
        },
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.method).toBe('textDocument/definition');
      expect(result.result).toBeDefined();

      // Should have location information
      if (Array.isArray(result.result)) {
        expect(result.result.length).toBeGreaterThan(0);
        expect(result.result[0]).toHaveProperty('uri');
        expect(result.result[0]).toHaveProperty('range');
      }
    });

    it('should handle textDocument/completion request', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Initialize clangd
      await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {
            textDocument: {
              completion: {
                completionItem: {
                  snippetSupport: true,
                },
              },
            },
          },
        },
      });

      await client.callTool('lsp_request', {
        method: 'initialized',
        params: {},
      });

      // Open the file first
      const mainPath = path.join(project.projectPath, 'src/main.cpp');
      const mainContent = await project.readFile('src/main.cpp');
      await client.callTool('lsp_request', {
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
            languageId: 'cpp',
            version: 1,
            text: mainContent,
          },
        },
      });

      // Request completion for std:: in main.cpp
      const response = await client.callTool('lsp_request', {
        method: 'textDocument/completion',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
          },
          position: {
            line: 25, // after std:: in main.cpp
            character: 9,
          },
        },
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.method).toBe('textDocument/completion');
      expect(result.result).toBeDefined();
    });

    it('should handle invalid LSP method gracefully', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Send invalid method
      const response = await client.callTool('lsp_request', {
        method: 'invalidMethod/doesNotExist',
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.method).toBe('invalidMethod/doesNotExist');
    });
  });

  describe('LSP workflow integration', () => {
    it('should demonstrate complete LSP workflow', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // 1. Check project status
      const statusResponse = await client.callTool('cpp_project_status');
      const statusResult = JSON.parse(getResponseText(statusResponse));
      expect(statusResult.success).toBe(true);
      expect(statusResult.is_configured).toBe(true);

      // 2. Setup clangd
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // 3. Initialize LSP
      const initResponse = await client.callTool('lsp_request', {
        method: 'initialize',
        params: {
          processId: null,
          rootUri: `file://${project.projectPath}`,
          capabilities: {},
        },
      });
      const initResult = JSON.parse(getResponseText(initResponse));
      expect(initResult.success).toBe(true);

      // 4. Send initialized notification
      const initializedResponse = await client.callTool('lsp_request', {
        method: 'initialized',
        params: {},
      });
      const initializedResult = JSON.parse(
        getResponseText(initializedResponse)
      );
      expect(initializedResult.success).toBe(true);

      // 4.5. Open file for LSP operations
      const mainPath = path.join(project.projectPath, 'src/main.cpp');
      const mainContent = await project.readFile('src/main.cpp');
      const didOpenResponse = await client.callTool('lsp_request', {
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
            languageId: 'cpp',
            version: 1,
            text: mainContent,
          },
        },
      });
      const didOpenResult = JSON.parse(getResponseText(didOpenResponse));
      expect(didOpenResult.success).toBe(true);

      // 5. Use LSP features (hover)
      const hoverResponse = await client.callTool('lsp_request', {
        method: 'textDocument/hover',
        params: {
          textDocument: {
            uri: `file://${mainPath}`,
          },
          position: {
            line: 17, // on Math::factorial line
            character: 52,
          },
        },
      });
      const hoverResult = JSON.parse(getResponseText(hoverResponse));
      expect(hoverResult.success).toBe(true);

      // 6. Shutdown
      const shutdownResponse = await client.callTool('lsp_request', {
        method: 'shutdown',
      });
      const shutdownResult = JSON.parse(getResponseText(shutdownResponse));
      expect(shutdownResult.success).toBe(true);
    });
  });

  describe('LSP symbol requests', () => {
    it('should handle textDocument/documentSymbol request', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd (this now performs full initialization automatically)
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Wait a moment for clangd to complete indexing
      await new Promise(resolve => setTimeout(resolve, 2000));

      // Request document symbols for Math.cpp (the file opened by setup_clangd)
      const mathPath = path.join(project.projectPath, 'src/Math.cpp');
      const response = await client.callTool('lsp_request', {
        method: 'textDocument/documentSymbol',
        params: {
          textDocument: {
            uri: `file://${mathPath}`,
          },
        },
      });

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.method).toBe('textDocument/documentSymbol');
      expect(result.result).toBeDefined();
      expect(Array.isArray(result.result)).toBe(true);

      // Verify we get symbols from the Math class
      const symbols = result.result;
      expect(symbols.length).toBeGreaterThan(0);

      // Get all symbol names (including nested ones)
      const getAllSymbolNames = (symbols: any[]): string[] => {
        const names: string[] = [];
        for (const symbol of symbols) {
          names.push(symbol.name);
          if (symbol.children) {
            names.push(...getAllSymbolNames(symbol.children));
          }
        }
        return names;
      };

      const allSymbolNames = getAllSymbolNames(symbols);

      // Should include the Math class methods with their fully qualified names
      expect(allSymbolNames).toContain('Math::factorial');
      expect(allSymbolNames).toContain('Math::gcd');
      // Also verify we have the namespace
      expect(allSymbolNames).toContain('TestProject');
    });

    it('should fail documentSymbol request without didOpen (regression test)', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure debug build
      await project.runCmake({ buildType: 'Debug', buildDir: 'build' });

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
      });
      await client.start();

      // Setup clangd (this now performs full initialization automatically)
      const setupResponse = await client.callTool('setup_clangd', {
        buildDirectory: 'build',
      });
      const setupResult = JSON.parse(getResponseText(setupResponse));
      expect(setupResult.success).toBe(true);

      // Wait a moment for initialization to complete
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Try to request document symbols WITHOUT didOpen first (should fail)
      const mathPath = path.join(project.projectPath, 'include/Math.hpp');
      const response = await client.callTool('lsp_request', {
        method: 'textDocument/documentSymbol',
        params: {
          textDocument: {
            uri: `file://${mathPath}`,
          },
        },
      });

      const result = JSON.parse(getResponseText(response));

      // This should fail with "trying to get AST for non-added document"
      expect(result.success).toBe(false);
      expect(result.error).toContain(
        'trying to get AST for non-added document'
      );
      expect(result.method).toBe('textDocument/documentSymbol');
    });
  });
});
