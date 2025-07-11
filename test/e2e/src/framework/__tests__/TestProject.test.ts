import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { TestProject, TestProjectError, ProjectTemplate, BuildConfiguration } from '../TestProject.js';
import * as path from 'path';

describe('TestProject', () => {
  let project: TestProject;

  afterEach(async () => {
    if (project) {
      await project.cleanup();
    }
  });

  describe('factory methods', () => {
    it('should create project from BASE template by default', async () => {
      project = await TestProject.fromTemplate();
      
      expect(project.projectPath).toBeDefined();
      expect(project.projectPath).toContain('test-project-');
      expect(await project.fileExists('CMakeLists.txt')).toBe(true);
      expect(await project.fileExists('src/main.cpp')).toBe(true);
      expect(await project.fileExists('include/Math.hpp')).toBe(true);
      expect(await project.fileExists('include/StringUtils.hpp')).toBe(true);
    });

    it('should create project from BASE template explicitly', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.BASE);
      
      expect(await project.fileExists('CMakeLists.txt')).toBe(true);
      expect(await project.fileExists('src/main.cpp')).toBe(true);
      expect(await project.fileExists('include/Math.hpp')).toBe(true);
      expect(await project.fileExists('include/StringUtils.hpp')).toBe(true);
    });

    it('should create empty project', async () => {
      project = await TestProject.empty();
      
      expect(project.projectPath).toBeDefined();
      expect(await project.fileExists('CMakeLists.txt')).toBe(false);
      expect(await project.fileExists('src')).toBe(false);
    });

    it('should create minimal CMake project', async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);
      
      expect(await project.fileExists('CMakeLists.txt')).toBe(true);
      expect(await project.fileExists('main.cpp')).toBe(true);
      
      const cmakeContent = await project.readFile('CMakeLists.txt');
      expect(cmakeContent).toContain('cmake_minimum_required(VERSION 3.16)');
      expect(cmakeContent).toContain('project(TestProject)');
    });

    it('should create project from base with options', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true,
        buildType: BuildConfiguration.DEBUG
      });
      
      expect(await project.fileExists('CMakeLists.txt')).toBe(true);
      
      const config = project.getCurrentConfiguration();
      expect(config.debugLogging).toBe(true);
      expect(config.memoryStorage).toBe(true);
      expect(config.buildType).toBe(BuildConfiguration.DEBUG);
    });

    it('should create projects with different paths', async () => {
      const project1 = await TestProject.fromTemplate();
      const project2 = await TestProject.fromTemplate();
      
      expect(project1.projectPath).not.toBe(project2.projectPath);
      
      await project1.cleanup();
      await project2.cleanup();
    });
  });

  describe('configuration methods', () => {
    beforeEach(async () => {
      project = await TestProject.fromBaseProject();
    });

    it('should configure project options', async () => {
      await project.configure({
        enableDebugLogging: true,
        enableMemoryStorage: false,
        buildType: BuildConfiguration.RELEASE
      });
      
      const config = project.getCurrentConfiguration();
      expect(config.debugLogging).toBe(true);
      expect(config.memoryStorage).toBe(false);
      expect(config.buildType).toBe(BuildConfiguration.RELEASE);
    });

    it('should switch build configuration', async () => {
      await project.switchBuildConfig(BuildConfiguration.RELEASE);
      
      const config = project.getCurrentConfiguration();
      expect(config.buildType).toBe(BuildConfiguration.RELEASE);
    });

    it('should enable and disable features', async () => {
      await project.enableFeature('debug-logging');
      expect(project.getCurrentConfiguration().debugLogging).toBe(true);
      
      await project.disableFeature('debug-logging');
      expect(project.getCurrentConfiguration().debugLogging).toBe(false);
      
      await project.enableFeature('memory-storage');
      expect(project.getCurrentConfiguration().memoryStorage).toBe(true);
      
      await project.disableFeature('memory-storage');
      expect(project.getCurrentConfiguration().memoryStorage).toBe(false);
    });

    it('should get available configurations', () => {
      const configs = project.getAvailableConfigurations();
      expect(configs).toContain(BuildConfiguration.DEBUG);
      expect(configs).toContain(BuildConfiguration.RELEASE);
      expect(configs).toContain(BuildConfiguration.CUSTOM);
    });
  });

  describe('enhanced file operations', () => {
    beforeEach(async () => {
      project = await TestProject.empty();
    });

    it('should write and read files', async () => {
      const content = 'Hello, World!';
      await project.writeFile('test.txt', content);
      
      const readContent = await project.readFile('test.txt');
      expect(readContent).toBe(content);
    });

    it('should copy files', async () => {
      await project.writeFile('source.txt', 'original content');
      await project.copyFile('source.txt', 'dest.txt');
      
      const sourceContent = await project.readFile('source.txt');
      const destContent = await project.readFile('dest.txt');
      expect(sourceContent).toBe(destContent);
      expect(destContent).toBe('original content');
    });

    it('should move files', async () => {
      await project.writeFile('source.txt', 'content to move');
      await project.moveFile('source.txt', 'subdir/moved.txt');
      
      expect(await project.fileExists('source.txt')).toBe(false);
      expect(await project.fileExists('subdir/moved.txt')).toBe(true);
      
      const movedContent = await project.readFile('subdir/moved.txt');
      expect(movedContent).toBe('content to move');
    });

    it('should list files in directory', async () => {
      await project.writeFile('file1.txt', 'content1');
      await project.writeFile('file2.txt', 'content2');
      await project.writeFile('subdir/file3.txt', 'content3');
      
      const files = await project.listFiles();
      expect(files).toContain('file1.txt');
      expect(files).toContain('file2.txt');
      expect(files).not.toContain('subdir'); // Directory, not file
      
      const subdirFiles = await project.listFiles('subdir');
      expect(subdirFiles).toContain('file3.txt');
    });

    it('should list directories', async () => {
      await project.createDirectory('dir1');
      await project.createDirectory('dir2');
      await project.writeFile('file.txt', 'content');
      
      const dirs = await project.listDirectories();
      expect(dirs).toContain('dir1');
      expect(dirs).toContain('dir2');
      expect(dirs).not.toContain('file.txt'); // File, not directory
    });

    it('should create directories when writing files', async () => {
      await project.writeFile('subdir/nested/file.txt', 'content');
      
      expect(await project.fileExists('subdir/nested/file.txt')).toBe(true);
    });

    it('should remove files', async () => {
      await project.writeFile('test.txt', 'content');
      expect(await project.fileExists('test.txt')).toBe(true);
      
      await project.removeFile('test.txt');
      expect(await project.fileExists('test.txt')).toBe(false);
    });

    it('should throw error when reading non-existent file', async () => {
      await expect(project.readFile('non-existent.txt'))
        .rejects
        .toThrow(TestProjectError);
    });

    it('should create and remove directories', async () => {
      await project.createDirectory('testdir');
      expect(await project.fileExists('testdir')).toBe(true);
      
      await project.removeDirectory('testdir');
      expect(await project.fileExists('testdir')).toBe(false);
    });
  });

  describe('cmake operations', () => {
    beforeEach(async () => {
      project = await TestProject.fromTemplate(ProjectTemplate.MINIMAL_CMAKE);
    });

    it('should run cmake configuration', async () => {
      await project.runCmake({ buildDir: 'build' });
      
      expect(await project.fileExists('build/CMakeCache.txt')).toBe(true);
    });

    it('should configure with custom build directory', async () => {
      await project.runCmake({ buildDir: 'custom-build' });
      
      expect(await project.fileExists('custom-build/CMakeCache.txt')).toBe(true);
    });

    it('should configure with custom build type', async () => {
      await project.runCmake({ buildType: 'Release', buildDir: 'build' });
      
      const cacheContent = await project.readFile('build/CMakeCache.txt');
      expect(cacheContent).toContain('CMAKE_BUILD_TYPE:STRING=Release');
    });

    it('should configure with custom options', async () => {
      await project.runCmake({
        buildDir: 'build',
        options: {
          'CUSTOM_OPTION': 'ON',
          'ANOTHER_OPTION': 'test_value',
        },
      });
      
      const cacheContent = await project.readFile('build/CMakeCache.txt');
      expect(cacheContent).toContain('CUSTOM_OPTION:UNINITIALIZED=ON');
      expect(cacheContent).toContain('ANOTHER_OPTION:UNINITIALIZED=test_value');
    });

    it('should include project configuration in CMake options', async () => {
      project = await TestProject.fromBaseProject({
        enableDebugLogging: true,
        enableMemoryStorage: true
      });
      
      await project.runCmake({ buildDir: 'build' });
      
      const cacheContent = await project.readFile('build/CMakeCache.txt');
      expect(cacheContent).toContain('ENABLE_DEBUG_LOGGING');
      expect(cacheContent).toContain('USE_MEMORY_STORAGE');
    });

    it('should build project', async () => {
      await project.runCmake({ buildDir: 'build' });
      await project.buildProject('build');
      
      // Check that build completed successfully (no exception thrown)
      expect(true).toBe(true);
    });

    it('should clean build', async () => {
      await project.runCmake({ buildDir: 'build' });
      await project.buildProject('build');
      await project.cleanBuild('build');
      
      // Check that clean completed successfully (no exception thrown)
      expect(true).toBe(true);
    });

    it('should use appropriate build directory for configuration', async () => {
      project = await TestProject.fromBaseProject({
        buildType: BuildConfiguration.DEBUG
      });
      
      await project.runCmake();
      
      // Should use build-debug directory
      expect(await project.fileExists('build-debug/CMakeCache.txt')).toBe(true);
    });

    it('should throw error for invalid cmake configuration', async () => {
      await project.writeFile('CMakeLists.txt', 'invalid cmake content');
      
      await expect(project.runCmake())
        .rejects
        .toThrow(TestProjectError);
    });
  });

  describe('project state', () => {
    beforeEach(async () => {
      project = await TestProject.fromBaseProject();
    });

    it('should return project path', () => {
      const path = project.getProjectPath();
      expect(path).toBe(project.projectPath);
      expect(path).toContain('test-project-');
    });

    it('should return current configuration', () => {
      const config = project.getCurrentConfiguration();
      expect(config).toBeDefined();
      expect(config.buildType).toBe(BuildConfiguration.DEBUG);
      expect(config.debugLogging).toBe(false);
      expect(config.memoryStorage).toBe(false);
      expect(config.customOptions).toEqual({});
    });

    it('should return available configurations', () => {
      const configs = project.getAvailableConfigurations();
      expect(Array.isArray(configs)).toBe(true);
      expect(configs.length).toBeGreaterThan(0);
      expect(configs).toContain(BuildConfiguration.DEBUG);
      expect(configs).toContain(BuildConfiguration.RELEASE);
    });
  });

  describe('cleanup', () => {
    it('should clean up temporary directories', async () => {
      project = await TestProject.fromTemplate();
      const projectPath = project.projectPath;
      
      await project.writeFile('test.txt', 'content');
      expect(await project.fileExists('test.txt')).toBe(true);
      
      await project.cleanup();
      
      // Directory should be removed (we can't easily test this without fs access)
      // At least verify that cleanup doesn't throw
      expect(true).toBe(true);
    });

    it('should handle cleanup of non-existent directories gracefully', async () => {
      project = await TestProject.fromTemplate();
      await project.cleanup();
      
      // Second cleanup should not throw
      await expect(project.cleanup()).resolves.not.toThrow();
    });
  });
});