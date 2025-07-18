import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject, BuildConfiguration } from '../framework/TestProject.js';
import { findMcpServer, TestUtils } from '../framework/TestUtils.js';
import { TestHelpers } from '../framework/TestHelpers.js';

describe('List Build Dirs Tool', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    // Enhanced setup with test context tracking
    const testContext = TestUtils.createTestContext('list-build-dirs-test', 'List Build Dirs Tool');
    project = await TestProject.fromBaseProject(undefined, testContext);

    const serverPath = await findMcpServer();
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

  describe('Basic functionality', () => {
    it('should list build directories with valid CMake configuration', async () => {
      await project.runCmake();

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();

      const responseText = (result.content?.[0]?.text ?? '{}') as string;

      const response = JSON.parse(responseText);

      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBeGreaterThan(0);

      const buildDir = response.build_dirs[0];
      expect(buildDir.path).toBeDefined();
      expect(buildDir.generator).toBeDefined();
      expect(buildDir.build_type).toBeDefined();
      expect(buildDir.compile_commands_exists).toBeDefined();
    });

    it('should handle project without CMake configuration', async () => {
      // Don't run CMake, so no build directories exist
      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBe(0);
    });

    it('should handle non-existent project path', async () => {
      // Remove CMakeLists.txt to simulate non-CMake project
      await project.removeFile('CMakeLists.txt');

      const result = await client.callTool('list_build_dirs');

      // Check if it's an error by looking at the response
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.error).toBeDefined();
      expect(responseText).toContain('CMakeLists.txt');
    });

    it('should use current directory when no project_path provided', async () => {
      await project.runCmake();

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBeGreaterThan(0);
    });
  });

  describe('Multiple build configurations', () => {
    it('should detect multiple build directories', async () => {
      // Create debug build
      await project.switchBuildConfig(BuildConfiguration.DEBUG);
      await project.runCmake({ buildType: 'Debug' });

      // Create release build
      await project.switchBuildConfig(BuildConfiguration.RELEASE);
      await project.runCmake({ buildType: 'Release' });

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBeGreaterThanOrEqual(2);

      const buildTypes = response.build_dirs.map(
        (d: { build_type: string }) => d.build_type
      );
      expect(buildTypes).toContain('Debug');
      expect(buildTypes).toContain('Release');
    });

    it('should detect different generators', async () => {
      // Try different generators (if available)
      const generators = ['Unix Makefiles', 'Ninja'];
      let successfulBuilds = 0;

      for (const generator of generators) {
        try {
          await project.runCmake({
            generator,
            buildDir: `build-${generator.toLowerCase().replace(' ', '-')}`,
          });
          successfulBuilds++;
        } catch {
          // Generator might not be available, skip
        }
      }

      if (successfulBuilds === 0) {
        // Skip test if no generators are available
        return;
      }

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs.length).toBeGreaterThanOrEqual(
        successfulBuilds
      );
    });
  });

  describe('Build directory analysis', () => {
    it('should provide detailed build information', async () => {
      await project.runCmake({
        buildType: 'Debug',
        options: {
          CMAKE_EXPORT_COMPILE_COMMANDS: 'ON',
          ENABLE_DEBUG_LOGGING: 'ON',
        },
      });

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBeGreaterThan(0);

      const buildDir = response.build_dirs[0];
      expect(buildDir.path).toBeDefined();
      expect(buildDir.generator).toBeDefined();
      expect(buildDir.build_type).toBe('Debug');
      expect(buildDir.compile_commands_exists).toBeDefined();
      expect(buildDir.options).toBeDefined();
      expect(buildDir.options['ENABLE_DEBUG_LOGGING']).toBe('ON');
    });

    it('should indicate compile_commands.json availability', async () => {
      await project.runCmake({
        options: { CMAKE_EXPORT_COMPILE_COMMANDS: 'ON' },
      });

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      const buildDir = response.build_dirs[0];
      expect(buildDir.compile_commands_exists).toBeDefined();
      expect(typeof buildDir.compile_commands_exists).toBe('boolean');
    });

    it('should handle corrupted CMake cache gracefully', async () => {
      await project.runCmake();

      // Corrupt the CMake cache
      await project.writeFile('build/CMakeCache.txt', 'corrupted content');

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      // Should either handle gracefully or exclude the corrupted build
    });
  });

  describe('CMake options and variables', () => {
    it('should report custom CMake options', async () => {
      await project.runCmake({
        options: {
          USE_MEMORY_STORAGE: 'ON',
          ENABLE_DEBUG_LOGGING: 'ON',
          CUSTOM_OPTION: 'custom_value',
        },
      });

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      const buildDir = response.build_dirs[0];
      expect(buildDir.options).toBeDefined();
      expect(buildDir.options['USE_MEMORY_STORAGE']).toBe('ON');
      expect(buildDir.options['ENABLE_DEBUG_LOGGING']).toBe('ON');
      expect(buildDir.options['CUSTOM_OPTION']).toBe('custom_value');
    });

    it('should report compiler information', async () => {
      await project.runCmake();

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      const buildDir = response.build_dirs[0];
      expect(buildDir.options).toBeDefined();
      // Note: CMAKE_ variables are now filtered out from options
      expect(typeof buildDir.options).toBe('object');
    });
  });

  describe('Project structure analysis', () => {
    it('should provide project metadata', async () => {
      await project.runCmake();

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.project_name).toBeDefined();
      expect(response.project_root).toBeDefined();
      expect(response.project_root).toBe(project.getProjectPath());
    });

    it('should handle missing CMakeLists.txt gracefully', async () => {
      await project.removeFile('CMakeLists.txt');

      const result = await client.callTool('list_build_dirs');

      // Check if it's an error by looking at the response
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.error).toBeDefined();
      expect(responseText).toContain('CMakeLists.txt');
    });
  });

  describe('Performance and edge cases', () => {
    it('should handle projects with nested build directories', async () => {
      // Create nested structure
      await project.createDirectory('subproject');
      await project.writeFile(
        'subproject/CMakeLists.txt',
        `
        cmake_minimum_required(VERSION 3.16)
        project(SubProject)
        add_executable(SubProject main.cpp)
      `
      );
      await project.writeFile(
        'subproject/main.cpp',
        `
        #include <iostream>
        int main() { return 0; }
      `
      );

      await project.runCmake();

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBeGreaterThan(0);
    });

    it('should handle large number of build directories', async () => {
      // Create multiple build configurations
      const configs = ['Debug', 'Release', 'RelWithDebInfo', 'MinSizeRel'];

      for (const config of configs) {
        try {
          await project.runCmake({
            buildType: config as
              | 'Debug'
              | 'Release'
              | 'RelWithDebInfo'
              | 'MinSizeRel',
            buildDir: `build-${config.toLowerCase()}`,
          });
        } catch {
          // Some configs might not be supported
        }
      }

      const result = await client.callTool('list_build_dirs');

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = JSON.parse(responseText);
      expect(response.build_dirs).toBeDefined();
      expect(response.build_dirs.length).toBeGreaterThan(0);
    });
  });
});
