import { promises as fs } from 'fs';
import * as path from 'path';
import * as fse from 'fs-extra';
import { spawn } from 'child_process';

interface VitestContext {
  task?: {
    name?: string;
    file?: { name?: string };
    suite?: { name?: string };
    result?: {
      state?: 'fail' | 'pass' | 'skip';
      errors?: Array<{ message: string }>;
      duration?: number;
    };
  };
}

export enum ProjectTemplate {
  BASE = 'base',
  EMPTY = 'empty',
  MINIMAL_CMAKE = 'minimal-cmake',
}

export enum BuildConfiguration {
  DEBUG = 'debug',
  RELEASE = 'release',
  CUSTOM = 'custom',
}

export interface ProjectOptions {
  enableDebugLogging?: boolean;
  enableMemoryStorage?: boolean;
  buildType?: BuildConfiguration;
  customCmakeOptions?: Record<string, string>;
}

export interface TestContext {
  testName: string;
  describe?: string;
  timestamp?: number;
  testId?: string;
}

export interface CmakeOptions {
  buildType?: 'Debug' | 'Release' | 'RelWithDebInfo' | 'MinSizeRel';
  generator?: string;
  options?: Record<string, string>;
  buildDir?: string;
}

export interface ProjectConfiguration {
  buildType: BuildConfiguration;
  debugLogging: boolean;
  memoryStorage: boolean;
  customOptions: Record<string, string>;
}

export class TestProjectError extends Error {
  constructor(
    message: string,
    public cause?: Error
  ) {
    super(message);
    this.name = 'TestProjectError';
  }
}

export class TestProject {
  private static tempCounter = 0;
  private cleanupCallbacks: (() => Promise<void>)[] = [];
  private currentConfig: ProjectConfiguration = {
    buildType: BuildConfiguration.DEBUG,
    debugLogging: false,
    memoryStorage: false,
    customOptions: {},
  };

  private constructor(public readonly projectPath: string) {}

  // Factory Methods
  static async fromTemplate(
    template: ProjectTemplate = ProjectTemplate.BASE,
    testContext?: TestContext
  ): Promise<TestProject> {
    const project = await TestProject.createTempProject(testContext);

    switch (template) {
      case ProjectTemplate.BASE:
        await project.copyFromBaseProject();
        break;
      case ProjectTemplate.EMPTY:
        // Empty project - just temp directory
        break;
      case ProjectTemplate.MINIMAL_CMAKE:
        await project.createMinimalCMakeProject();
        break;
    }

    return project;
  }

  static async fromBaseProject(
    options?: ProjectOptions,
    testContext?: TestContext
  ): Promise<TestProject> {
    const project = await TestProject.fromTemplate(
      ProjectTemplate.BASE,
      testContext
    );
    if (options) {
      await project.configure(options);
    }
    return project;
  }

  static async empty(testContext?: TestContext): Promise<TestProject> {
    return TestProject.fromTemplate(ProjectTemplate.EMPTY, testContext);
  }

  static async fromExisting(
    sourcePath: string,
    testContext?: TestContext
  ): Promise<TestProject> {
    const project = await TestProject.createTempProject(testContext);
    await fse.copy(sourcePath, project.projectPath);
    return project;
  }

  // Configuration Methods
  async configure(options: ProjectOptions): Promise<void> {
    if (options.enableDebugLogging !== undefined) {
      this.currentConfig.debugLogging = options.enableDebugLogging;
      await this.updateCMakeOption(
        'ENABLE_DEBUG_LOGGING',
        options.enableDebugLogging ? 'ON' : 'OFF'
      );
    }

    if (options.enableMemoryStorage !== undefined) {
      this.currentConfig.memoryStorage = options.enableMemoryStorage;
      await this.updateCMakeOption(
        'USE_MEMORY_STORAGE',
        options.enableMemoryStorage ? 'ON' : 'OFF'
      );
    }

    if (options.buildType !== undefined) {
      this.currentConfig.buildType = options.buildType;
    }

    if (options.customCmakeOptions) {
      this.currentConfig.customOptions = {
        ...this.currentConfig.customOptions,
        ...options.customCmakeOptions,
      };
    }
  }

  async switchBuildConfig(config: BuildConfiguration): Promise<void> {
    this.currentConfig.buildType = config;

    const buildDir =
      config === BuildConfiguration.DEBUG
        ? 'build-debug'
        : config === BuildConfiguration.RELEASE
          ? 'build-release'
          : 'build';

    await this.ensureBuildDirectory(buildDir);
  }

  async enableFeature(
    feature: 'debug-logging' | 'memory-storage'
  ): Promise<void> {
    const options: ProjectOptions = {};
    if (feature === 'debug-logging') {
      options.enableDebugLogging = true;
    } else if (feature === 'memory-storage') {
      options.enableMemoryStorage = true;
    }
    await this.configure(options);
  }

  async disableFeature(
    feature: 'debug-logging' | 'memory-storage'
  ): Promise<void> {
    const options: ProjectOptions = {};
    if (feature === 'debug-logging') {
      options.enableDebugLogging = false;
    } else if (feature === 'memory-storage') {
      options.enableMemoryStorage = false;
    }
    await this.configure(options);
  }

  // Enhanced File Operations
  async writeFile(relativePath: string, content: string): Promise<void> {
    const fullPath = path.join(this.projectPath, relativePath);
    const dir = path.dirname(fullPath);

    await fse.ensureDir(dir);
    await fs.writeFile(fullPath, content, 'utf-8');
  }

  async readFile(relativePath: string): Promise<string> {
    const fullPath = path.join(this.projectPath, relativePath);
    try {
      return await fs.readFile(fullPath, 'utf-8');
    } catch (error) {
      throw new TestProjectError(
        `Failed to read file ${relativePath}`,
        error as Error
      );
    }
  }

  async copyFile(from: string, to: string): Promise<void> {
    const fromPath = path.join(this.projectPath, from);
    const toPath = path.join(this.projectPath, to);
    const toDir = path.dirname(toPath);

    await fse.ensureDir(toDir);
    await fse.copy(fromPath, toPath);
  }

  async moveFile(from: string, to: string): Promise<void> {
    const fromPath = path.join(this.projectPath, from);
    const toPath = path.join(this.projectPath, to);
    const toDir = path.dirname(toPath);

    await fse.ensureDir(toDir);
    await fse.move(fromPath, toPath);
  }

  async removeFile(relativePath: string): Promise<void> {
    const fullPath = path.join(this.projectPath, relativePath);
    try {
      await fs.unlink(fullPath);
    } catch (error) {
      throw new TestProjectError(
        `Failed to remove file ${relativePath}`,
        error as Error
      );
    }
  }

  async listFiles(relativePath: string = '.'): Promise<string[]> {
    const fullPath = path.join(this.projectPath, relativePath);
    try {
      const entries = await fs.readdir(fullPath, { withFileTypes: true });
      return entries
        .filter((entry) => entry.isFile())
        .map((entry) => entry.name);
    } catch (error) {
      throw new TestProjectError(
        `Failed to list files in ${relativePath}`,
        error as Error
      );
    }
  }

  async fileExists(relativePath: string): Promise<boolean> {
    const fullPath = path.join(this.projectPath, relativePath);
    try {
      await fs.access(fullPath);
      return true;
    } catch {
      return false;
    }
  }

  // Directory Operations
  async createDirectory(relativePath: string): Promise<void> {
    const fullPath = path.join(this.projectPath, relativePath);
    await fse.ensureDir(fullPath);
  }

  async removeDirectory(relativePath: string): Promise<void> {
    const fullPath = path.join(this.projectPath, relativePath);
    await fse.remove(fullPath);
  }

  async listDirectories(relativePath: string = '.'): Promise<string[]> {
    const fullPath = path.join(this.projectPath, relativePath);
    try {
      const entries = await fs.readdir(fullPath, { withFileTypes: true });
      return entries
        .filter((entry) => entry.isDirectory())
        .map((entry) => entry.name);
    } catch (error) {
      throw new TestProjectError(
        `Failed to list directories in ${relativePath}`,
        error as Error
      );
    }
  }

  // CMake Operations
  async runCmake(options: CmakeOptions = {}): Promise<void> {
    const {
      buildType = 'Debug',
      generator,
      options: cmakeOptions = {},
      buildDir,
    } = options;

    // Use provided buildDir or determine from current config
    const actualBuildDir = buildDir ?? this.getCurrentBuildDir();
    const buildPath = path.join(this.projectPath, actualBuildDir);
    await fse.ensureDir(buildPath);

    const args = ['-S', this.projectPath, '-B', buildPath];

    if (generator) {
      args.push('-G', generator);
    }

    args.push(`-DCMAKE_BUILD_TYPE=${buildType}`);

    // Add current config options
    const allOptions = { ...this.currentConfig.customOptions, ...cmakeOptions };
    if (this.currentConfig.debugLogging) {
      allOptions['ENABLE_DEBUG_LOGGING'] = 'ON';
    }
    if (this.currentConfig.memoryStorage) {
      allOptions['USE_MEMORY_STORAGE'] = 'ON';
    }

    // Add all options
    for (const [key, value] of Object.entries(allOptions)) {
      args.push(`-D${key}=${value}`);
    }

    await this.runCommand('cmake', args);
  }

  async buildProject(buildDir?: string): Promise<void> {
    const actualBuildDir = buildDir ?? this.getCurrentBuildDir();
    const buildPath = path.join(this.projectPath, actualBuildDir);
    await this.runCommand('cmake', ['--build', buildPath]);
  }

  async cleanBuild(buildDir?: string): Promise<void> {
    const actualBuildDir = buildDir ?? this.getCurrentBuildDir();
    const buildPath = path.join(this.projectPath, actualBuildDir);
    await this.runCommand('cmake', ['--build', buildPath, '--target', 'clean']);
  }

  // Project State
  getProjectPath(): string {
    return this.projectPath;
  }

  getCurrentConfiguration(): ProjectConfiguration {
    return { ...this.currentConfig };
  }

  getAvailableConfigurations(): string[] {
    return Object.values(BuildConfiguration);
  }

  // Private Helper Methods
  private static async createTempProject(
    testContext?: TestContext
  ): Promise<TestProject> {
    // Use crypto.randomUUID() for truly unique directory names to avoid race conditions
    const { randomUUID } = await import('crypto');
    const uuid = randomUUID();

    // Create descriptive folder name that includes test information
    let folderName: string;
    if (testContext) {
      const sanitizedTestName = testContext.testName
        .replace(/[^a-zA-Z0-9-_]/g, '-')
        .replace(/-+/g, '-')
        .replace(/^-|-$/g, '')
        .toLowerCase();
      const shortUuid = uuid.substring(0, 8);
      folderName = `${sanitizedTestName}-${shortUuid}`;
    } else {
      folderName = `test-project-${uuid}`;
    }

    const tempDir = path.join(process.cwd(), 'temp', folderName);

    await fse.ensureDir(tempDir);

    const project = new TestProject(tempDir);

    // Create test metadata file
    if (testContext) {
      await project.createTestMetadata(testContext, uuid);
    }

    project.cleanupCallbacks.push(async () => {
      await fse.remove(tempDir);
    });

    return project;
  }

  private async copyFromBaseProject(): Promise<void> {
    const basePath = path.resolve(process.cwd(), '..', 'test-project');

    try {
      await fse.copy(basePath, this.projectPath, {
        filter: (src: string) => {
          // Exclude build directories to avoid CMake cache conflicts
          const relativePath = path.relative(basePath, src);
          return !relativePath.startsWith('build');
        },
      });
    } catch (error) {
      throw new TestProjectError(
        'Failed to copy base test project',
        error as Error
      );
    }
  }

  private async createMinimalCMakeProject(): Promise<void> {
    const cmakeContent = `cmake_minimum_required(VERSION 3.16)
project(TestProject)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(TestProject main.cpp)
`;

    const cppContent = `#include <iostream>

int main() {
    std::cout << "Hello, World!" << std::endl;
    return 0;
}
`;

    await this.writeFile('CMakeLists.txt', cmakeContent);
    await this.writeFile('main.cpp', cppContent);
  }

  private async updateCMakeOption(key: string, value: string): Promise<void> {
    this.currentConfig.customOptions[key] = value;
  }

  private getCurrentBuildDir(): string {
    switch (this.currentConfig.buildType) {
      case BuildConfiguration.DEBUG:
        return 'build-debug';
      case BuildConfiguration.RELEASE:
        return 'build-release';
      case BuildConfiguration.CUSTOM:
      default:
        return 'build';
    }
  }

  private async ensureBuildDirectory(buildDir: string): Promise<void> {
    const buildPath = path.join(this.projectPath, buildDir);
    await fse.ensureDir(buildPath);
  }

  private async runCommand(command: string, args: string[]): Promise<void> {
    return new Promise((resolve, reject) => {
      const process = spawn(command, args, {
        cwd: this.projectPath,
        stdio: 'pipe',
      });

      let stdout = '';
      let stderr = '';

      process.stdout?.on('data', (data) => {
        stdout += data.toString();
      });

      process.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      process.on('close', (code) => {
        if (code === 0) {
          resolve();
        } else {
          reject(
            new TestProjectError(
              `Command failed: ${command} ${args.join(' ')}\nStdout: ${stdout}\nStderr: ${stderr}`
            )
          );
        }
      });

      process.on('error', (error) => {
        reject(
          new TestProjectError(`Failed to spawn command: ${command}`, error)
        );
      });
    });
  }

  async cleanup(
    options: { cleanupOnFailure?: boolean; vitestContext?: VitestContext } = {}
  ): Promise<void> {
    const { cleanupOnFailure = false, vitestContext } = options;

    // Check test failure status - prefer vitest context over global state
    let testFailed = false;
    if (vitestContext?.task?.result?.state === 'fail') {
      testFailed = true;
    } else {
      // Fallback to global state (may not be reliable due to timing)
      const currentTestName = globalThis.__currentTestName;
      const testStatus = currentTestName
        ? globalThis.__testStates?.get(currentTestName)
        : 'unknown';
      testFailed = testStatus === 'failed';
    }

    // If test failed and cleanupOnFailure is false, preserve the folder
    if (testFailed && !cleanupOnFailure) {
      // Extract specific test case information from Vitest context
      let reason = 'Test failed - folder preserved automatically';
      let testCaseInfo = {};

      if (vitestContext?.task) {
        const task = vitestContext.task;
        testCaseInfo = {
          testCase: task.name ?? 'unknown',
          testFile: task.file?.name ?? 'unknown',
          fullName: task.suite?.name
            ? `${task.suite.name} > ${task.name}`
            : task.name,
          errors: task.result?.errors?.map((e) => e.message) ?? [],
          duration: task.result?.duration ?? 0,
        };

        reason = `Test case "${task.name}" failed - folder preserved automatically`;
      }

      await this.preserveForDebugging(reason, testCaseInfo);
      return;
    }

    // Update test status to completed before cleanup
    try {
      await this.updateTestStatus(testFailed ? 'failed' : 'completed');
    } catch {
      // Ignore metadata update errors during cleanup
    }

    for (const callback of this.cleanupCallbacks) {
      try {
        await callback();
      } catch (error) {
        // eslint-disable-next-line no-console
        console.warn('Cleanup error:', error);
      }
    }
    this.cleanupCallbacks.length = 0;
  }

  // Test identification methods
  private async createTestMetadata(
    testContext: TestContext,
    uuid: string
  ): Promise<void> {
    const metadata = {
      testName: testContext.testName,
      describe: testContext.describe,
      timestamp: testContext.timestamp ?? Date.now(),
      testId: testContext.testId,
      uuid: uuid,
      projectPath: this.projectPath,
      createdAt: new Date().toISOString(),
      nodeVersion: process.version,
      platform: process.platform,
      // Enhanced debugging metadata
      status: 'running',
      lastUpdated: new Date().toISOString(),
      processId: process.pid,
      parentProcessId: process.ppid,
      workingDirectory: process.cwd(),
      // Environment context
      vitestEnvironment: {
        pool: process.env.VITEST_POOL_ID,
        worker: process.env.VITEST_WORKER_ID,
        mode: process.env.VITEST_MODE,
        environment: process.env.NODE_ENV,
      },
      // Test configuration
      configuration: this.currentConfig,
      // Folder naming context
      folderName: this.projectPath.split('/').pop(),
    };

    await this.writeFile('.test-info.json', JSON.stringify(metadata, null, 2));
  }

  async getTestMetadata(): Promise<Record<string, unknown> | null> {
    try {
      const content = await this.readFile('.test-info.json');
      return JSON.parse(content) as Record<string, unknown>;
    } catch {
      return null;
    }
  }

  async preserveForDebugging(
    reason?: string,
    testCaseInfo?: unknown
  ): Promise<void> {
    const debugInfo = {
      preservedAt: new Date().toISOString(),
      reason: reason ?? 'Manual preservation',
      projectPath: this.projectPath,
      ...(testCaseInfo && { testCase: testCaseInfo }),
    };

    await this.writeFile(
      '.debug-preserved.json',
      JSON.stringify(debugInfo, null, 2)
    );

    // Update test status to indicate preservation
    try {
      await this.updateTestStatus('preserved', reason);
    } catch {
      // Ignore metadata update errors
    }

    // Clear cleanup callbacks to prevent folder deletion
    this.cleanupCallbacks.length = 0;

    // eslint-disable-next-line no-console
    console.log(
      `\n🔍 Test folder preserved for debugging: ${this.projectPath}`
    );
    // eslint-disable-next-line no-console
    console.log(`   Reason: ${reason ?? 'Manual preservation'}`);
  }

  private async updateTestStatus(
    status: 'running' | 'completed' | 'failed' | 'preserved',
    reason?: string
  ): Promise<void> {
    try {
      const metadata = await this.getTestMetadata();
      if (metadata) {
        metadata.status = status;
        metadata.lastUpdated = new Date().toISOString();
        if (reason) {
          metadata.statusReason = reason;
        }
        if (status === 'completed') {
          metadata.completedAt = new Date().toISOString();
        }
        await this.writeFile(
          '.test-info.json',
          JSON.stringify(metadata, null, 2)
        );
      }
    } catch {
      // Silently ignore metadata update errors
    }
  }
}
