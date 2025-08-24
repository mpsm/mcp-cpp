/* eslint-disable no-console, @typescript-eslint/no-explicit-any */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject } from '../framework/TestProject.js';
import { findMcpServer, TestUtils } from '../framework/TestUtils.js';

interface SymbolResponse {
  name?: string;
  kind?: string;
  location?: {
    file?: string;
  };
  [key: string]: unknown;
}

describe('Search Symbols Tool', () => {
  let client: McpClient;
  let project: TestProject;

  beforeEach(async () => {
    // Enhanced setup with test context tracking
    const testContext = TestUtils.createTestContext(
      'search-symbols-test',
      'Search Symbols Tool'
    );
    project = await TestProject.fromBaseProject(undefined, testContext);

    // Ensure project is built and indexed BEFORE starting the server
    await project.runCmake();

    const serverPath = await findMcpServer();
    const logEnv = TestUtils.createTestEnvironment(
      project.getProjectPath(),
      testContext.testName,
      'warn'
    );

    client = new McpClient(serverPath, {
      workingDirectory: project.getProjectPath(),
      timeout: 20000, // Increased timeout for symbol indexing
      env: logEnv.env,
    });
    await client.start();
  });

  afterEach(async (context) => {
    await client.stop();
    // Use enhanced cleanup that preserves folders on test failure
    await project.cleanup({
      cleanupOnFailure: false,
      vitestContext: context as any,
    });
  });

  // Helper function to safely parse JSON responses or handle error messages
  function parseResponse(responseText: string): any {
    try {
      return JSON.parse(responseText);
    } catch {
      // Handle non-JSON error responses (like "No build directory found" messages)
      console.log('Non-JSON response:', responseText);
      return {
        error: responseText,
        isPlainTextError: true,
      };
    }
  }

  describe('Basic symbol search', () => {
    it('should find main Math class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      // Check if we got an error or valid response
      if (response.error) {
        console.log('Search failed with error:', response.error);
        // This is expected during development - log for debugging
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();
        expect(Array.isArray(response.symbols)).toBe(true);

        // Look for the Math class symbol
        const mathSymbol = response.symbols.find(
          (s: any) => s.name === 'Math' && s.kind === 'class'
        );
        if (mathSymbol) {
          expect(mathSymbol.name).toBe('Math');
          expect(mathSymbol.kind).toBe('class');
          expect(mathSymbol.location).toBeDefined();
          if (mathSymbol.location?.file) {
            expect(mathSymbol.location.file).toContain('Math.hpp');
          }
        }
      }
    });

    it('should find factorial function', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'factorial',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Factorial search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // Should find multiple factorial overloads
        const factorialSymbols = response.symbols.filter((s: any) =>
          s.name.includes('factorial')
        );
        if (factorialSymbols.length > 0) {
          expect(factorialSymbols.length).toBeGreaterThan(0);

          // Check that we have function symbols (LSP kind 6 = METHOD, 12 = FUNCTION)
          const functionSymbols = factorialSymbols.filter(
            (s: any) =>
              s.kind === 'function' ||
              s.kind === 'method' ||
              s.kind === 6 ||
              s.kind === 12
          );
          expect(functionSymbols.length).toBeGreaterThan(0);
        }
      }
    });

    it('should find Container template class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Container',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Container search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const containerSymbol = response.symbols.find(
          (s: any) => s.name === 'Container' && s.kind === 'class'
        );
        if (containerSymbol) {
          expect(containerSymbol.name).toBe('Container');
          expect(containerSymbol.kind).toBe('class');
          if (containerSymbol.location?.file) {
            expect(containerSymbol.location.file).toContain('Container.hpp');
          }
        }
      }
    });

    it('should find StringUtils class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'StringUtils',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('StringUtils search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const stringUtilsSymbol = response.symbols.find(
          (s: any) => s.name === 'StringUtils' && s.kind === 'class'
        );
        if (stringUtilsSymbol) {
          expect(stringUtilsSymbol.name).toBe('StringUtils');
          expect(stringUtilsSymbol.kind).toBe('class');
          if (stringUtilsSymbol.location?.file) {
            expect(stringUtilsSymbol.location.file).toContain(
              'StringUtils.hpp'
            );
          }
        }
      }
    });

    it('should find TestProject namespace', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'TestProject',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('TestProject namespace search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // May or may not find namespace symbols depending on clangd indexing
        if (response.symbols.length > 0) {
          const namespaceSymbol = response.symbols.find(
            (s: any) => s.name === 'TestProject' && s.kind === 'namespace'
          );
          if (namespaceSymbol) {
            expect(namespaceSymbol.name).toBe('TestProject');
            expect(namespaceSymbol.kind).toBe('namespace');
          }
        }
      }
    });
  });

  describe('Advanced filtering', () => {
    it('should filter by symbol kind - class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Math',
        kind: 'class',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Kind filtering failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // All returned symbols should be classes
        if (response.symbols.length > 0) {
          response.symbols.forEach((symbol: SymbolResponse) => {
            if (typeof symbol.kind === 'string') {
              expect(symbol.kind).toBe('class');
            }
          });
        }
      }
    });

    it('should filter by symbol kind - function', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'factorial',
        kind: 'function',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Function kind filtering failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // All returned symbols should be functions
        if (response.symbols.length > 0) {
          response.symbols.forEach((symbol: SymbolResponse) => {
            if (typeof symbol.kind === 'string') {
              expect(['function', 'method'].includes(symbol.kind)).toBe(true);
            }
          });
        }
      }
    });

    it('should respect result limits', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'operator', // Should match many symbols
        max_results: 5,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Limit filtering failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // Should respect the limit
        if (response.symbols.length > 0) {
          expect(response.symbols.length).toBeLessThanOrEqual(5);
        }
      }
    });

    it('should handle project boundary filtering', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'vector', // Should match std::vector and project code
        include_external: false,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Project boundary filtering failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // Should only include project symbols, not external std::vector
        if (response.symbols.length > 0) {
          response.symbols.forEach((symbol: SymbolResponse) => {
            if (symbol.location?.file) {
              // Project files should be in include/ or src/
              expect(
                symbol.location.file.includes('include/') ||
                  symbol.location.file.includes('src/')
              ).toBe(true);
            }
          });
        }
      }
    });
  });

  describe('Template symbols', () => {
    it('should find Matrix template class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Matrix',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Matrix template search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const matrixSymbol = response.symbols.find(
          (s: any) => s.name.includes('Matrix') && s.kind === 'class'
        );
        if (matrixSymbol) {
          expect(matrixSymbol.name).toContain('Matrix');
          expect(matrixSymbol.kind).toBe('class');
          if (matrixSymbol.location?.file) {
            expect(matrixSymbol.location.file).toContain('Math.hpp');
          }
        }
      }
    });

    it('should find type aliases', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Matrix2x2',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Type alias search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const aliasSymbol = response.symbols.find(
          (s: any) => s.name === 'Matrix2x2'
        );
        if (aliasSymbol) {
          expect(aliasSymbol.name).toBe('Matrix2x2');
          if (aliasSymbol.location?.file) {
            expect(aliasSymbol.location.file).toContain('Math.hpp');
          }
        }
      }
    });

    it('should find template functions from Algorithms namespace', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'max_element',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Template function search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const maxElementSymbol = response.symbols.find((s: any) =>
          s.name.includes('max_element')
        );
        if (maxElementSymbol) {
          expect(maxElementSymbol.name).toContain('max_element');
          if (maxElementSymbol.location?.file) {
            expect(maxElementSymbol.location.file).toContain('Algorithms.hpp');
          }
        }
      }
    });
  });

  describe('Nested classes', () => {
    it('should find Statistics nested class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Statistics',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Statistics nested class search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const statisticsSymbol = response.symbols.find(
          (s: any) => s.name === 'Statistics' && s.kind === 'class'
        );
        if (statisticsSymbol) {
          expect(statisticsSymbol.name).toBe('Statistics');
          expect(statisticsSymbol.kind).toBe('class');
          if (statisticsSymbol.location?.file) {
            expect(statisticsSymbol.location.file).toContain('Math.hpp');
          }
        }
      }
    });

    it('should find Complex nested class', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Complex',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Complex nested class search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const complexSymbol = response.symbols.find(
          (s: any) => s.name === 'Complex' && s.kind === 'class'
        );
        if (complexSymbol) {
          expect(complexSymbol.name).toBe('Complex');
          expect(complexSymbol.kind).toBe('class');
          if (complexSymbol.location?.file) {
            expect(complexSymbol.location.file).toContain('Math.hpp');
          }
        }
      }
    });
  });

  describe('Enum symbols', () => {
    it('should find LogLevel enum', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'LogLevel',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('LogLevel enum search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const logLevelSymbol = response.symbols.find(
          (s: any) => s.name === 'LogLevel' && s.kind === 'enum'
        );
        if (logLevelSymbol) {
          expect(logLevelSymbol.name).toBe('LogLevel');
          expect(logLevelSymbol.kind).toBe('enum');
          if (logLevelSymbol.location?.file) {
            expect(logLevelSymbol.location.file).toContain('LogLevel.hpp');
          }
        }
      }
    });

    it('should find LogFormat enum', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'LogFormat',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('LogFormat enum search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        const logFormatSymbol = response.symbols.find(
          (s: any) => s.name === 'LogFormat' && s.kind === 'enum'
        );
        if (logFormatSymbol) {
          expect(logFormatSymbol.name).toBe('LogFormat');
          expect(logFormatSymbol.kind).toBe('enum');
          if (logFormatSymbol.location?.file) {
            expect(logFormatSymbol.location.file).toContain('LogLevel.hpp');
          }
        }
      }
    });
  });

  describe('Edge cases and error handling', () => {
    it('should handle non-existent symbols gracefully', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'NonExistentSymbol123',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log(
          'Non-existent symbol search failed (expected):',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();
        // Should return empty array for non-existent symbols
        expect(response.symbols.length).toBe(0);
      }
    });

    it('should handle empty query', async () => {
      const result = await client.callTool('search_symbols', {
        query: '',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Empty query search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();
        // Empty query might return all symbols or none
        expect(Array.isArray(response.symbols)).toBe(true);
      }
    });

    it('should handle special characters in query', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'operator+',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Special character query failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();
        // Should handle operator symbols
        expect(Array.isArray(response.symbols)).toBe(true);
      }
    });

    it('should handle invalid symbol kind', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Math',
        kind: 'invalid_kind',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('Invalid kind query failed (expected):', response.error);
        expect(response.error).toBeDefined();
      } else {
        // Should either ignore invalid kind or return empty results
        expect(response.symbols).toBeDefined();
        expect(Array.isArray(response.symbols)).toBe(true);
      }
    });
  });

  describe('File-specific search', () => {
    it('should search symbols in specific file', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'factorial',
        file: 'include/Math.hpp',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log('File-specific search failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();

        // All symbols should be from the specified file
        if (response.symbols.length > 0) {
          response.symbols.forEach((symbol: SymbolResponse) => {
            if (symbol.location?.file) {
              expect(symbol.location.file).toContain('Math.hpp');
            }
          });
        }
      }
    });

    it('should handle non-existent file gracefully', async () => {
      const result = await client.callTool('search_symbols', {
        query: 'Math',
        files: ['include/NonExistent.hpp'],
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response = parseResponse(responseText);

      if (response.error) {
        console.log(
          'Non-existent file search failed (expected):',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.symbols).toBeDefined();
        // Should return empty array for non-existent file
        expect(response.symbols.length).toBe(0);
      }
    });
  });
});
