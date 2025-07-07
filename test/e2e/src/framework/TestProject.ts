import { promises as fs } from 'fs';
import * as path from 'path';
import * as fse from 'fs-extra';
import { spawn } from 'child_process';

export interface CmakeOptions {
  buildType?: 'Debug' | 'Release' | 'RelWithDebInfo' | 'MinSizeRel';
  generator?: string;
  options?: Record<string, string>;
  buildDir?: string;
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

  constructor(public readonly projectPath: string) {}

  static async create(fixtureName?: string): Promise<TestProject> {
    const tempDir = path.join(
      process.cwd(),
      'temp',
      `test-project-${Date.now()}-${++TestProject.tempCounter}`
    );

    await fse.ensureDir(tempDir);

    const project = new TestProject(tempDir);
    project.cleanupCallbacks.push(async () => {
      await fse.remove(tempDir);
    });

    if (fixtureName) {
      await project.loadFixture(fixtureName);
    }

    return project;
  }

  static async createFromExisting(sourcePath: string): Promise<TestProject> {
    const tempDir = path.join(
      process.cwd(),
      'temp',
      `test-project-${Date.now()}-${++TestProject.tempCounter}`
    );

    await fse.copy(sourcePath, tempDir);

    const project = new TestProject(tempDir);
    project.cleanupCallbacks.push(async () => {
      await fse.remove(tempDir);
    });

    return project;
  }

  async loadFixture(fixtureName: string): Promise<void> {
    const fixturesPath = path.join(process.cwd(), 'src', 'fixtures');
    const fixturePath = path.join(fixturesPath, `${fixtureName}.ts`);

    try {
      const fixture = await import(fixturePath);
      if (fixture.default && typeof fixture.default === 'function') {
        await fixture.default(this);
      } else {
        throw new TestProjectError(`Invalid fixture: ${fixtureName}`);
      }
    } catch (error) {
      throw new TestProjectError(
        `Failed to load fixture ${fixtureName}`,
        error as Error
      );
    }
  }

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

  async createDirectory(relativePath: string): Promise<void> {
    const fullPath = path.join(this.projectPath, relativePath);
    await fse.ensureDir(fullPath);
  }

  async removeDirectory(relativePath: string): Promise<void> {
    const fullPath = path.join(this.projectPath, relativePath);
    await fse.remove(fullPath);
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

  async runCmake(options: CmakeOptions = {}): Promise<void> {
    const {
      buildType = 'Debug',
      generator,
      options: cmakeOptions = {},
      buildDir = 'build',
    } = options;

    const buildPath = path.join(this.projectPath, buildDir);
    await fse.ensureDir(buildPath);

    const args = ['-S', this.projectPath, '-B', buildPath];

    if (generator) {
      args.push('-G', generator);
    }

    args.push(`-DCMAKE_BUILD_TYPE=${buildType}`);

    for (const [key, value] of Object.entries(cmakeOptions)) {
      args.push(`-D${key}=${value}`);
    }

    await this.runCommand('cmake', args);
  }

  async buildProject(buildDir = 'build'): Promise<void> {
    const buildPath = path.join(this.projectPath, buildDir);
    await this.runCommand('cmake', ['--build', buildPath]);
  }

  async copyFromBaseProject(): Promise<void> {
    const basePath = path.resolve(process.cwd(), '..', 'test-project');

    try {
      await fse.copy(basePath, this.projectPath);
    } catch (error) {
      throw new TestProjectError(
        'Failed to copy base test project',
        error as Error
      );
    }
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

  async cleanup(): Promise<void> {
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
}
