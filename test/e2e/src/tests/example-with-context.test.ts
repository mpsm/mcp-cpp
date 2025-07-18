import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { TestHelpers } from '../framework/TestHelpers.js';
import { TestProject } from '../framework/TestProject.js';
import { McpClient } from '../framework/McpClient.js';

describe('Example Test with Context Tracking', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    // Using the enhanced setup that automatically handles context
    const setup = await TestHelpers.setupTest({
      template: 'base',
      testName: 'example-context-test',
      describe: 'Example Test with Context Tracking',
      logLevel: 'info',
    });

    project = setup.project;
    client = setup.client;
  });

  afterEach(async () => {
    await TestHelpers.cleanup(client, project);
  });

  it('should create identifiable temp folders and logs', async () => {
    // The temp folder will now have a descriptive name like:
    // "example-context-test-a1b2c3d4" instead of "test-project-uuid"

    // Check that metadata was created
    const metadata = await project.getTestMetadata();
    expect(metadata).toBeDefined();
    expect(metadata.testName).toBe('example-context-test');
    expect(metadata.describe).toBe('Example Test with Context Tracking');

    // Run a test operation
    await project.runCmake();
    const result = await client.callTool('list_build_dirs');

    expect(result.content).toBeDefined();

    // Logs will be named like:
    // - mcp-cpp-server-example-context-test.log
    // - mcp-cpp-clangd-example-context-test.log
    // instead of just mcp-cpp-server.log
  });

  it('should handle test failure preservation', async () => {
    // Simulate a test that might fail and need debugging
    try {
      await project.runCmake();
      const result = await client.callTool('list_build_dirs');

      // If this test were to fail, we could preserve the folder:
      // await TestHelpers.preserveForDebugging(project, 'Test failed - investigating build dirs');

      expect(result.content).toBeDefined();
    } catch (error) {
      // In a real failing test, uncomment this to preserve the folder
      // await TestHelpers.preserveForDebugging(project, `Test failed: ${error.message}`);
      throw error;
    }
  });
});

describe('Manual Context Example', () => {
  it('should allow manual context creation', async () => {
    // Manual approach for more control
    const project = await TestHelpers.createTestProject({
      template: 'base',
      testName: 'manual-context-test',
      describe: 'Manual Context Example',
    });

    const client = await TestHelpers.createMcpClient(project, {
      testName: 'manual-context-test',
      logLevel: 'debug',
    });

    await client.start();

    try {
      const metadata = await project.getTestMetadata();
      expect(metadata.testName).toBe('manual-context-test');

      await project.runCmake();
      const result = await client.callTool('list_build_dirs');
      expect(result.content).toBeDefined();
    } finally {
      await TestHelpers.cleanup(client, project);
    }
  });
});
