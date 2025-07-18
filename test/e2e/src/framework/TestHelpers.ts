import { TestProject, TestContext } from './TestProject.js';
import { TestUtils } from './TestUtils.js';
import { McpClient } from './McpClient.js';

/**
 * Helper functions to simplify test setup with proper context tracking
 */
export class TestHelpers {
  /**
   * Create a test project with proper context from the current test
   */
  static async createTestProject(options?: {
    template?: 'base' | 'empty' | 'minimal-cmake';
    projectOptions?: Record<string, unknown>;
    testName?: string;
    describe?: string;
  }): Promise<TestProject> {
    const testContext = TestUtils.createTestContext(
      options?.testName,
      options?.describe
    );

    switch (options?.template) {
      case 'empty':
        return TestProject.empty(testContext);
      case 'minimal-cmake':
        return TestProject.fromTemplate('minimal-cmake', testContext);
      default:
        return TestProject.fromBaseProject(
          options?.projectOptions,
          testContext
        );
    }
  }

  /**
   * Create an MCP client with test-aware logging
   */
  static async createMcpClient(
    project: TestProject,
    options?: {
      timeout?: number;
      logLevel?: string;
      testName?: string;
    }
  ): Promise<McpClient> {
    const serverPath = await TestUtils.findMcpServer();
    const testContext = TestUtils.createTestContext(options?.testName);
    const logEnv = TestUtils.createTestEnvironment(
      project.getProjectPath(),
      testContext.testName,
      options?.logLevel ?? 'warn'
    );

    return new McpClient(serverPath, {
      workingDirectory: project.getProjectPath(),
      timeout: options?.timeout ?? 15000,
      env: logEnv.env,
    });
  }

  /**
   * Enhanced test setup that automatically handles context and logging
   */
  static async setupTest(options?: {
    template?: 'base' | 'empty' | 'minimal-cmake';
    projectOptions?: Record<string, unknown>;
    timeout?: number;
    logLevel?: string;
    testName?: string;
    describe?: string;
  }): Promise<{ project: TestProject; client: McpClient }> {
    const project = await TestHelpers.createTestProject(options);
    const client = await TestHelpers.createMcpClient(project, options);

    await client.start();

    return { project, client };
  }

  /**
   * Enhanced cleanup that handles both client and project cleanup
   */
  static async cleanup(client: McpClient, project: TestProject): Promise<void> {
    await client.stop();
    await project.cleanup();
  }

  /**
   * Preserve test artifacts for debugging (useful for failed tests)
   */
  static async preserveForDebugging(
    project: TestProject,
    reason?: string
  ): Promise<void> {
    await project.preserveForDebugging(reason);
  }

  /**
   * Get the current test name from the environment or stack trace
   */
  static getCurrentTestName(): string {
    // Try to get from environment first (set by test runner)
    const envTestName = process.env.VITEST_TEST_NAME;
    if (envTestName) {
      return envTestName;
    }

    // Try to get from vitest context if available
    const vitestContext = process.env.VITEST_POOL_ID;
    if (vitestContext) {
      return `vitest-${vitestContext}`;
    }

    // Fallback: extract from stack trace with better parsing
    const stack = new Error().stack;
    if (stack) {
      const lines = stack.split('\n');
      for (const line of lines) {
        // Look for test files in the stack
        const testMatch = line.match(
          /at\s+(?:.*\s+\()?([^\s\(]+\.(test|spec)\.[jt]s)/
        );
        if (testMatch) {
          const testFile = testMatch[1];
          const basename =
            testFile
              .split('/')
              .pop()
              ?.replace(/\.(test|spec)\.[jt]s$/, '') ?? 'unknown';
          return basename;
        }

        // Look for test function names
        const functionMatch = line.match(/at\s+(test|it|describe)\s/);
        if (functionMatch) {
          const contextMatch = line.match(/\(([^)]+)\)/);
          if (contextMatch) {
            const file =
              contextMatch[1]
                .split('/')
                .pop()
                ?.replace(/\.[jt]s$/, '') ?? 'unknown';
            return file;
          }
        }
      }
    }

    // Last resort: use timestamp
    return `test-${Date.now()}`;
  }

  /**
   * Get the current describe block name
   */
  static getCurrentDescribeName(): string | undefined {
    // Check various vitest environment variables
    const describe = process.env.VITEST_DESCRIBE ?? process.env.VITEST_SUITE;
    if (describe) {
      return describe;
    }

    // Try to extract from stack trace
    const stack = new Error().stack;
    if (stack) {
      const describeMatch = stack.match(/describe\s*\(["']([^"']+)["']/);
      if (describeMatch) {
        return describeMatch[1];
      }
    }

    return undefined;
  }

  /**
   * Create a test context with automatic detection of test information
   */
  static createAutoTestContext(): TestContext {
    return {
      testName: TestHelpers.getCurrentTestName(),
      describe: TestHelpers.getCurrentDescribeName(),
      timestamp: Date.now(),
      testId: `auto-${Date.now()}`,
    };
  }
}
