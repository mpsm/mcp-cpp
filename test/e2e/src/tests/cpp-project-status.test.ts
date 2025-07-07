import { describe, it, expect, beforeEach, afterEach, beforeAll } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject } from '../framework/TestProject.js';
import * as path from 'path';

describe('cpp_project_status tool', () => {
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
    it('should list cpp_project_status tool', async () => {
      client = new McpClient(serverPath);
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
      project = await TestProject.create('cmake-basic');

      // Create client with working directory set to project path
      client = new McpClient(serverPath, 10000, project.projectPath);
      await client.start();

      const response = await client.callTool('cpp_project_status');

      expect(response).toBeDefined();
      expect(response.content).toBeDefined();
      expect(Array.isArray(response.content)).toBe(true);
      expect(response.content.length).toBeGreaterThan(0);

      const result = JSON.parse(response.content[0].text);

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
      project = await TestProject.create('empty-project');

      client = new McpClient(serverPath, 10000, project.projectPath);
      await client.start();

      const response = await client.callTool('cpp_project_status');

      expect(response).toBeDefined();
      const result = JSON.parse(response.content[0].text);

      expect(result.success).toBe(true);
      expect(result.project_type).toBe('unknown');
      expect(result.is_configured).toBe(false);
      expect(result.message).toContain('not a CMake project');
      expect(result.build_directories).toEqual([]);
    });

    it('should handle CMake project without build directories', async () => {
      project = await TestProject.create();

      // Create CMakeLists.txt but don't run cmake
      await project.writeFile(
        'CMakeLists.txt',
        `
cmake_minimum_required(VERSION 3.15)
project(TestProject)
add_executable(TestProject main.cpp)
`
      );

      await project.writeFile(
        'main.cpp',
        `
#include <iostream>
int main() { return 0; }
`
      );

      client = new McpClient(serverPath, 10000, project.projectPath);
      await client.start();

      const response = await client.callTool('cpp_project_status');
      const result = JSON.parse(response.content[0].text);

      expect(result.success).toBe(true);
      expect(result.project_type).toBe('cmake');
      expect(result.is_configured).toBe(false);
      expect(result.build_directories).toEqual([]);
      expect(result.summary).toContain('not configured');
    });
  });

  describe('error handling', () => {
    it('should handle corrupted CMakeCache.txt gracefully', async () => {
      project = await TestProject.create();

      await project.writeFile(
        'CMakeLists.txt',
        `
cmake_minimum_required(VERSION 3.15)
project(TestProject)
`
      );

      // Create build directory with corrupted cache
      await project.createDirectory('build');
      await project.writeFile(
        'build/CMakeCache.txt',
        'corrupted cache content'
      );

      client = new McpClient(serverPath, 10000, project.projectPath);
      await client.start();

      const response = await client.callTool('cpp_project_status');
      const result = JSON.parse(response.content[0].text);

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
