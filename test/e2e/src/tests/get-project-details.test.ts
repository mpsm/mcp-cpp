import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject } from '../framework/TestProject.js';
import { findMcpServer, TestUtils } from '../framework/TestUtils.js';

interface VitestTaskContext {
  task?: {
    name?: string;
    file?: { name?: string };
    suite?: { name?: string };
  };
}

describe('Get Project Details Tool', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    const testContext = TestUtils.createTestContext(
      'get-project-details-test',
      'Get Project Details Tool'
    );
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

  afterEach(async (context: VitestTaskContext) => {
    await client.stop();
    await project.cleanup({
      cleanupOnFailure: false,
      vitestContext: context,
    });
  });

  it('should get project details with valid CMake configuration', async () => {
    // Run CMake configuration first to create build directory
    await project.runCmake();

    // Start MCP server AFTER CMake to ensure build directory is discovered
    const serverPath = await findMcpServer();
    const logEnv = TestUtils.createTestEnvironment(
      project.getProjectPath(),
      'get-project-details-cmake',
      'warn'
    );

    const tempClient = new McpClient(serverPath, {
      workingDirectory: project.getProjectPath(),
      timeout: 10000,
      env: logEnv.env,
    });
    await tempClient.start();

    const result = await tempClient.callTool('get_project_details');

    expect(result.content).toBeDefined();
    const responseText = (result.content?.[0]?.text ?? '{}') as string;

    const response = JSON.parse(responseText);
    expect(response.components).toBeDefined();
    expect(response.components.length).toBeGreaterThan(0);

    const component = response.components[0];
    expect(component.build_dir_path).toBeDefined(); // Updated to match actual field name
    expect(component.provider_type).toBe('cmake');
    expect(component.build_type).toBeDefined();

    await tempClient.stop();
  });

  it('should handle project without CMake configuration', async () => {
    const result = await client.callTool('get_project_details');

    expect(result.content).toBeDefined();
    const responseText = (result.content?.[0]?.text ?? '{}') as string;

    const response = JSON.parse(responseText);
    expect(response.components).toBeDefined();
    expect(response.components.length).toBe(0);
  });
});
