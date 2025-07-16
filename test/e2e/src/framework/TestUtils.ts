import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

/**
 * Utilities for configuring test environments, particularly for log management
 */
export class TestUtils {
  /**
   * Create a temporary log file for testing and return its path
   */
  static async createTempLogFile(testName: string): Promise<string> {
    const tempDir = path.join(os.tmpdir(), 'mcp-cpp-tests');
    await fs.mkdir(tempDir, { recursive: true });

    const logFileName = `${testName}-${Date.now()}-${process.pid}.log`;
    return path.join(tempDir, logFileName);
  }

  /**
   * Clean up temporary log files after test completion
   */
  static async cleanupTempLogFile(logFilePath: string): Promise<void> {
    try {
      await fs.unlink(logFilePath);
    } catch (error: unknown) {
      // Silently ignore if file doesn't exist (ENOENT) - this is expected when no logs were generated
      if ((error as { code?: string })?.code !== 'ENOENT') {
        // eslint-disable-next-line no-console
        console.warn(`Failed to cleanup log file ${logFilePath}:`, error);
      }
    }
  }

  /**
   * Read and parse log file contents for analysis
   */
  static async readLogFile(logFilePath: string): Promise<LogEntry[]> {
    try {
      const content = await fs.readFile(logFilePath, 'utf-8');
      return content
        .split('\n')
        .filter((line) => line.trim().length > 0)
        .map((line) => TestUtils.parseLogLine(line))
        .filter((entry) => entry !== null) as LogEntry[];
    } catch (error: unknown) {
      // Silently return empty array if file doesn't exist (ENOENT) - this means no logs were generated
      if ((error as { code?: string })?.code === 'ENOENT') {
        return [];
      }
      // eslint-disable-next-line no-console
      console.warn(`Failed to read log file ${logFilePath}:`, error);
      return [];
    }
  }

  /**
   * Parse a single log line into structured data
   */
  private static parseLogLine(line: string): LogEntry | null {
    // Match tracing format: TIMESTAMP LEVEL ThreadId(ID) TARGET: MESSAGE
    const match = line.match(
      /^(\S+)\s+(\w+)\s+ThreadId\(\d+\)\s+([^:]+):\s*(.*)$/
    );
    if (!match) {
      return null;
    }

    const [, timestamp, level, target, message] = match;
    return {
      timestamp,
      level: level as LogLevel,
      target,
      message: message.trim(),
      raw: line,
    };
  }

  /**
   * Analyze log entries for test validation
   * Any ERROR or WARN level logs during tests indicate potential issues that should be investigated
   */
  static analyzeLogEntries(entries: LogEntry[]): LogAnalysis {
    const analysis: LogAnalysis = {
      totalEntries: entries.length,
      levels: { ERROR: 0, WARN: 0, INFO: 0, DEBUG: 0, TRACE: 0 },
      errors: [],
      warnings: [],
    };

    for (const entry of entries) {
      analysis.levels[entry.level]++;

      if (entry.level === 'ERROR') {
        analysis.errors.push(entry);
      } else if (entry.level === 'WARN') {
        analysis.warnings.push(entry);
      }
    }

    return analysis;
  }

  /**
   * Create environment variables for test logging configuration
   */
  static createTestLogEnv(
    logFilePath: string,
    logLevel: string = 'warn'
  ): Record<string, string> {
    return {
      // Set log level to reduce noise during testing
      RUST_LOG: logLevel,
      // Direct logs to file instead of stderr
      MCP_LOG_FILE: logFilePath,
      // Add unique identifier to prevent conflicts
      MCP_LOG_UNIQUE: 'true',
      // Use structured format for easier parsing
      MCP_LOG_JSON: 'false', // Keep human-readable for easier debugging
    };
  }

  /**
   * Find the MCP server binary path
   */
  static async findMcpServer(): Promise<string> {
    // Look for the binary in the standard cargo target directory
    const possiblePaths = [
      path.resolve(
        __dirname,
        '..',
        '..',
        '..',
        '..',
        'target',
        'debug',
        'mcp-cpp-server'
      ),
      path.resolve(
        __dirname,
        '..',
        '..',
        '..',
        '..',
        'target',
        'release',
        'mcp-cpp-server'
      ),
      path.join(process.cwd(), '..', '..', 'target', 'debug', 'mcp-cpp-server'),
      path.join(
        process.cwd(),
        '..',
        '..',
        'target',
        'release',
        'mcp-cpp-server'
      ),
      // Try relative to the current working directory
      path.resolve('target', 'debug', 'mcp-cpp-server'),
      path.resolve('target', 'release', 'mcp-cpp-server'),
      // Try from the project root
      path.resolve('..', '..', 'target', 'debug', 'mcp-cpp-server'),
      path.resolve('..', '..', 'target', 'release', 'mcp-cpp-server'),
    ];

    for (const serverPath of possiblePaths) {
      try {
        await fs.access(serverPath);
        return serverPath;
      } catch {
        // Continue checking other paths
      }
    }

    throw new Error(
      'MCP server binary not found. Please run "cargo build" first.'
    );
  }
}

export type LogLevel = 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | 'TRACE';

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  target: string;
  message: string;
  raw: string;
}

export interface LogAnalysis {
  totalEntries: number;
  levels: Record<LogLevel, number>;
  errors: LogEntry[];
  warnings: LogEntry[];
}

// Named export for convenience
export const findMcpServer = TestUtils.findMcpServer;
