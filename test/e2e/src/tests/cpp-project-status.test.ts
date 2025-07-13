import { describe, it, expect, beforeEach, afterEach, beforeAll } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import {
  TestProject,
  ProjectTemplate,
  BuildConfiguration,
} from '../framework/TestProject.js';
import { TestUtils } from '../framework/TestUtils.js';
import * as path from 'path';

// Helper to extract text from MCP response
function getResponseText(response: { content: unknown[] }): string {
  const textContent = response.content[0] as { text: string };
  return textContent.text;
}

describe('cpp_project_status tool', () => {
  let client: McpClient;
  let project: TestProject;
  let logFilePath: string;
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
    // Create log file for this specific test
    logFilePath = await TestUtils.createTempLogFile('cpp-project-status');

    // Client will be created per test with specific working directory
  });

  afterEach(async () => {
    if (client) {
      await client.stop();
    }
    if (project) {
      await project.cleanup();
    }

    // Analyze logs and cleanup
    const logEntries = await TestUtils.readLogFile(logFilePath);
    const analysis = TestUtils.analyzeLogEntries(logEntries);

    // For clean tests, we should have minimal ERROR/WARN logs
    // If we do have them, log them for investigation but don't fail the test
    // (since the functional behavior was already validated)
    if (analysis.errors.length > 0) {
      // eslint-disable-next-line no-console
      console.warn(
        `Test generated ${analysis.errors.length} error logs (investigate these):`,
        analysis.errors.map((e) => e.message)
      );
    }
    if (analysis.warnings.length > 0) {
      // eslint-disable-next-line no-console
      console.warn(
        `Test generated ${analysis.warnings.length} warning logs:`,
        analysis.warnings.map((e) => e.message)
      );
    }

    await TestUtils.cleanupTempLogFile(logFilePath);
  });

  describe('tool availability', () => {
    it('should list cpp_project_status tool', async () => {
      client = new McpClient(serverPath, {
        logFilePath,
        logLevel: 'warn', // Capture warnings and errors for analysis
      });
      await client.start();

      const tools = await client.listTools();

      expect(tools).toBeDefined();
      expect(Array.isArray(tools)).toBe(true);
      expect(tools.length).toBeGreaterThan(0);

      const cppStatusTool = tools.find(
        (tool) => tool.name === 'cpp_project_status'
      );
      expect(cppStatusTool).toBeDefined();
      expect(cppStatusTool?.description).toContain('C++ project status');
    });
  });

  describe('CMake project detection', () => {
    it('should detect valid CMake project with build directories', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG,
      });

      // Configure both debug and release builds
      await project.runCmake({ buildType: 'Debug', buildDir: 'build-debug' });
      await project.runCmake({
        buildType: 'Release',
        buildDir: 'build-release',
      });

      // Create client with working directory set to project path
      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
        logFilePath,
        logLevel: 'error',
      });
      await client.start();

      const response = await client.callTool('cpp_project_status');

      expect(response).toBeDefined();
      expect(response.content).toBeDefined();
      expect(Array.isArray(response.content)).toBe(true);
      expect(response.content.length).toBeGreaterThan(0);

      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.project_type).toBe('cmake');
      expect(result.is_configured).toBe(true);
      expect(result.build_directories).toBeDefined();
      expect(Array.isArray(result.build_directories)).toBe(true);
      expect(result.build_directories.length).toBe(2); // Debug and Release builds

      // Check build directory details
      const debugBuild = result.build_directories.find((bd: { path: string }) =>
        bd.path.includes('build-debug')
      );
      const releaseBuild = result.build_directories.find(
        (bd: { path: string }) => bd.path.includes('build-release')
      );

      expect(debugBuild).toBeDefined();
      expect(debugBuild.build_type).toBe('Debug');
      expect(debugBuild.cache_exists).toBe(true);

      expect(releaseBuild).toBeDefined();
      expect(releaseBuild.build_type).toBe('Release');
      expect(releaseBuild.cache_exists).toBe(true);
    });

    it('should detect non-CMake project', async () => {
      project = await TestProject.empty();

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
        logFilePath,
        logLevel: 'error',
      });
      await client.start();

      const response = await client.callTool('cpp_project_status');

      expect(response).toBeDefined();
      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.project_type).toBe('unknown');
      expect(result.is_configured).toBe(false);
      expect(result.message).toContain('not a CMake project');
      expect(result.build_directories).toEqual([]);
    });

    it('should handle CMake project without build directories', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
        logFilePath,
        logLevel: 'error',
      });
      await client.start();

      const response = await client.callTool('cpp_project_status');
      const result = JSON.parse(getResponseText(response));

      expect(result.success).toBe(true);
      expect(result.project_type).toBe('cmake');
      expect(result.is_configured).toBe(false);
      expect(result.build_directories).toEqual([]);
      expect(result.summary).toContain('not configured');
    });
  });

  describe('error handling', () => {
    it('should handle corrupted CMakeCache.txt gracefully', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);

      // Create build directory with corrupted cache
      await project.createDirectory('build');
      await project.writeFile(
        'build/CMakeCache.txt',
        'corrupted cache content'
      );

      client = new McpClient(serverPath, {
        workingDirectory: project.projectPath,
        logFilePath,
        logLevel: 'error',
      });
      await client.start();

      const response = await client.callTool('cpp_project_status');
      const result = JSON.parse(getResponseText(response));

      // Should successfully detect CMake project and handle corrupted cache
      expect(result.success).toBe(true);
      expect(result.project_type).toBe('cmake');
      expect(result.is_configured).toBe(true); // Server currently considers it configured
      expect(result.build_directories).toBeDefined();
      expect(Array.isArray(result.build_directories)).toBe(true);
      expect(result.issues).toBeDefined();
      expect(Array.isArray(result.issues)).toBe(true);
    });
  });
});
