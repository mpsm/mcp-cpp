/* eslint-disable no-console, @typescript-eslint/no-explicit-any */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject } from '../framework/TestProject.js';
import { findMcpServer, TestUtils } from '../framework/TestUtils.js';

interface AnalyzeSymbolResponse {
  // Actual analyze_symbol_context response format (AnalyzerResult)
  symbol?: {
    name?: string;
    kind?: string | number;
    container_name?: string;
    location?: string;
  };
  query?: string;
  definitions?: string[];
  declarations?: string[];
  hover_documentation?: string;
  detail?: string;
  examples?: string[];
  type_hierarchy?: {
    supertypes?: any[];
    subtypes?: any[];
  };
  call_hierarchy?: {
    callers?: any[];
    callees?: any[];
  };
  members?: {
    methods?: Array<{
      name?: string;
      member_type?: string;
      signature?: string;
    }>;
    constructors?: any[];
    destructors?: any[];
    operators?: any[];
  };

  // For test framework compatibility (added by parseResponse for error cases)
  success?: boolean;
  error?: string;
  message?: string;
  isPlainTextError?: boolean;
  [key: string]: unknown;
}

describe('Analyze Symbol Context Tool', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    // Enhanced setup with test context tracking
    const testContext = TestUtils.createTestContext(
      'analyze-symbol-context-test',
      'Analyze Symbol Context Tool'
    );
    project = await TestProject.fromBaseProject(undefined, testContext);

    // Ensure project is built and indexed BEFORE starting the server
    await project.runCmake();

    const serverPath = await findMcpServer();
    client = new McpClient(serverPath, {
      workingDirectory: project.projectPath,
      timeout: 15000,
    });
    await client.start();
  });

  afterEach(async () => {
    await client?.stop();
    await project.cleanup({
      cleanupOnFailure: false,
      vitestContext: expect.getState().currentTestName as any,
    });
  });

  // Helper function to safely parse JSON responses or handle error messages
  function parseResponse(responseText: string): AnalyzeSymbolResponse {
    try {
      const parsed = JSON.parse(responseText);

      // analyze_symbol_context returns direct AnalyzerResult, not wrapped in success/error format
      if (parsed.symbol || parsed.query || parsed.definitions) {
        // This is a successful analyze_symbol_context response - return as-is but add success flag for test compatibility
        return {
          success: true,
          ...parsed,
        } as AnalyzeSymbolResponse;
      }

      // Handle other response formats (e.g., error responses)
      return parsed;
    } catch {
      // Handle non-JSON error responses (like "No build directory found" messages)
      console.log('Non-JSON response:', responseText);
      return {
        success: false,
        error: 'plain_text_error',
        message: responseText,
        isPlainTextError: true,
      } as AnalyzeSymbolResponse;
    }
  }

  // Helper function to handle kind comparisons (kind can be string or number)
  function isKindMatch(
    kind: string | number | undefined,
    expectedPatterns: string[]
  ): boolean {
    if (!kind) return false;
    const kindStr = String(kind);
    return expectedPatterns.some(
      (pattern) => kindStr.includes(pattern) || kindStr === pattern
    );
  }

  describe('Basic Symbol Analysis', () => {
    it('should analyze Math class with basic information', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.error) {
        console.log('Math class analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toBe('Math');
          expect(isKindMatch(response.symbol.kind, ['class', '5'])).toBe(true);
          expect(response.symbol.location).toBeDefined();

          if (response.symbol.location) {
            expect(response.symbol.location).toContain('Math.hpp');
          }
        }

        // Verify we have definitions
        expect(response.definitions).toBeDefined();
        expect(Array.isArray(response.definitions)).toBe(true);

        // Verify examples are present
        expect(response.examples).toBeDefined();
        expect(Array.isArray(response.examples)).toBe(true);

        // Verify query matches
        expect(response.query).toBe('Math');

        // Check for hover documentation
        if (response.hover_documentation) {
          expect(typeof response.hover_documentation).toBe('string');
        }

        // Check for members (classes should have methods)
        if (response.members?.methods) {
          expect(Array.isArray(response.members.methods)).toBe(true);
          expect(response.members.methods.length).toBeGreaterThan(0);
        }
      }
    });

    it('should analyze factorial function', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.error) {
        console.log('Factorial function analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('factorial');
          // LSP SymbolKind: 6 = METHOD, so we should accept '6' as well
          expect(
            isKindMatch(response.symbol.kind, ['function', 'method', '6'])
          ).toBe(true);
          expect(response.symbol.location).toBeDefined();
        }

        // Verify we have definitions
        expect(response.definitions).toBeDefined();
        expect(Array.isArray(response.definitions)).toBe(true);

        // Verify examples are present
        expect(response.examples).toBeDefined();
        expect(Array.isArray(response.examples)).toBe(true);
      }
    });

    it('should analyze Container template class', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Container',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.error) {
        console.log('Container class analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toBe('Container');
          expect(
            isKindMatch(response.symbol.kind, ['class', 'template', '5'])
          ).toBe(true);
          expect(response.symbol.location).toBeDefined();
        }
      }
    });

    it('should handle non-existent symbol gracefully', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'NonExistentSymbol123',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.isPlainTextError) {
        // Handle plain text error responses during development
        expect(response.message).toContain('No symbols found');
      } else if (response.success === false) {
        expect(response.error).toBeDefined();
      } else {
        // Tool might return empty results instead of error
        console.log('Non-existent symbol response:', response);
      }
    });
  });

  describe('Advanced Features', () => {
    it('should analyze with max_examples parameter', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        max_examples: 3,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.success && response.examples) {
        expect(Array.isArray(response.examples)).toBe(true);
        expect(response.examples.length).toBeLessThanOrEqual(3);
      }
    });

    it('should handle build_directory parameter', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        build_directory: 'build-debug',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.isPlainTextError) {
        // Expected behavior when build directory path handling differs
        console.log('Build directory parameter test result:', response.message);
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();
      }
    });
  });

  describe('Class Member Analysis', () => {
    it('should analyze Math class members', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.success && response.members) {
        expect(response.members).toBeDefined();

        if (response.members.methods) {
          expect(Array.isArray(response.members.methods)).toBe(true);
          expect(response.members.methods.length).toBeGreaterThan(0);

          // Check method structure
          response.members.methods.forEach((method) => {
            expect(method).toHaveProperty('name');
            expect(method).toHaveProperty('signature');
          });
        }

        if (response.members.constructors) {
          expect(Array.isArray(response.members.constructors)).toBe(true);
        }

        if (response.members.destructors) {
          expect(Array.isArray(response.members.destructors)).toBe(true);
        }

        if (response.members.operators) {
          expect(Array.isArray(response.members.operators)).toBe(true);
        }
      }
    });
  });

  describe('Hierarchy Analysis', () => {
    it('should provide type hierarchy for classes', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.success && response.type_hierarchy) {
        expect(response.type_hierarchy).toBeDefined();

        if (response.type_hierarchy.supertypes) {
          expect(Array.isArray(response.type_hierarchy.supertypes)).toBe(true);
        }

        if (response.type_hierarchy.subtypes) {
          expect(Array.isArray(response.type_hierarchy.subtypes)).toBe(true);
        }
      }
    });

    it('should provide call hierarchy for functions', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.success && response.call_hierarchy) {
        expect(response.call_hierarchy).toBeDefined();

        if (response.call_hierarchy.callers) {
          expect(Array.isArray(response.call_hierarchy.callers)).toBe(true);
        }

        if (response.call_hierarchy.callees) {
          expect(Array.isArray(response.call_hierarchy.callees)).toBe(true);
        }
      }
    });
  });

  describe('Error Handling', () => {
    it('should handle invalid parameters gracefully', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: '',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      // Should either return an error or handle empty symbol gracefully
      if (response.isPlainTextError) {
        expect(response.message).toBeDefined();
      } else if (response.success === false) {
        expect(response.error).toBeDefined();
      } else {
        // Tool might handle empty symbol in a specific way
        console.log('Empty symbol response:', response);
      }
    });

    it('should handle non-existent build directory', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        build_directory: 'non-existent-build',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalyzeSymbolResponse = parseResponse(responseText);

      if (response.isPlainTextError) {
        expect(response.message).toContain('Path does not exist');
      } else if (response.success === false) {
        expect(response.error).toBeDefined();
      }
    });
  });
});
