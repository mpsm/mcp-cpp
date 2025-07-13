import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { CallToolResult, Tool } from '@modelcontextprotocol/sdk/types.js';

export class McpClientError extends Error {
  constructor(
    message: string,
    public code?: number,
    public data?: unknown
  ) {
    super(message);
    this.name = 'McpClientError';
  }
}

export interface McpClientOptions {
  timeout?: number;
  workingDirectory?: string;
  logFilePath?: string;
  logLevel?: string;
  env?: Record<string, string>;
}

export class McpClient {
  private client?: Client;
  private transport?: StdioClientTransport;
  private options: McpClientOptions;

  constructor(
    private serverPath: string,
    options: McpClientOptions = {}
  ) {
    this.options = {
      timeout: 10000,
      logLevel: 'warn',
      ...options,
    };
  }

  async start(): Promise<void> {
    if (this.client) {
      throw new McpClientError('MCP client already started');
    }

    // Prepare environment variables for logging configuration
    const env: Record<string, string> = {};

    // Copy existing environment
    for (const [key, value] of Object.entries(process.env)) {
      if (value !== undefined) {
        env[key] = value;
      }
    }

    // Apply custom environment variables
    if (this.options.env) {
      Object.assign(env, this.options.env);
    }

    if (this.options.logFilePath) {
      env.MCP_LOG_FILE = this.options.logFilePath;
      env.MCP_LOG_UNIQUE = 'true';
    }

    if (this.options.logLevel) {
      env.RUST_LOG = this.options.logLevel;
    }

    // Create transport that will spawn the process
    this.transport = new StdioClientTransport({
      command: this.serverPath,
      args: [],
      ...(this.options.workingDirectory && {
        cwd: this.options.workingDirectory,
      }),
      env,
    });

    this.client = new Client(
      {
        name: 'mcp-cpp-e2e-test',
        version: '1.0.0',
      },
      {
        capabilities: {
          tools: {},
        },
      }
    );

    // Connect client to transport (this will spawn the process)
    await this.client.connect(this.transport);
  }

  async listTools(): Promise<Tool[]> {
    if (!this.client) {
      throw new McpClientError('Client not initialized');
    }

    try {
      const response = await this.client.listTools();
      return response.tools;
    } catch (error) {
      throw new McpClientError(
        `List tools failed: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  async callTool(
    name: string,
    args: Record<string, unknown> = {}
  ): Promise<CallToolResult> {
    if (!this.client) {
      throw new McpClientError('Client not initialized');
    }

    try {
      const response = await this.client.callTool({
        name,
        arguments: args,
      });

      return response as CallToolResult;
    } catch (error) {
      throw new McpClientError(
        `Tool call failed: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  async stop(): Promise<void> {
    if (this.client) {
      await this.client.close();
      this.client = undefined;
    }

    if (this.transport) {
      await this.transport.close();
      this.transport = undefined;
    }
  }
}
