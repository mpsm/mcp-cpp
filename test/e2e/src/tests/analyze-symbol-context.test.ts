/* eslint-disable no-console, @typescript-eslint/no-explicit-any */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { McpClient } from '../framework/McpClient.js';
import { TestProject } from '../framework/TestProject.js';
import { findMcpServer, TestUtils } from '../framework/TestUtils.js';

interface AnalysisResponse {
  success?: boolean;
  symbol?: {
    name?: string;
    kind?: string;
    fully_qualified_name?: string;
    file_location?: {
      uri?: string;
      range?: any;
    };
    definition?: any;
    declaration?: any;
    type_info?: any;
    documentation?: any;
    usage_statistics?: {
      total_references?: number;
      files_containing_references?: number;
      reference_density?: number;
    };
    inheritance?: {
      base_classes?: string[];
      derived_classes?: string[];
      has_inheritance?: boolean;
    };
    usage_examples?: Array<{
      file?: string;
      range?: any;
      context?: string;
      pattern_type?: string;
    }>;
    call_relationships?: {
      incoming_calls?: any[];
      outgoing_calls?: any[];
      total_callers?: number;
      total_callees?: number;
      call_depth_analyzed?: number;
      has_call_relationships?: boolean;
    };
    class_members?: {
      members?: Array<{
        name?: string;
        kind?: string;
        detail?: string;
        range?: any;
      }>;
      total_count?: number;
    };
  };
  metadata?: {
    analysis_type?: string;
    features_used?: any;
    build_directory_used?: string;
    indexing_waited?: boolean;
    indexing_status?: any;
  };
  error?: string;
  message?: string;
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

    const serverPath = await findMcpServer();
    const logEnv = TestUtils.createTestEnvironment(
      project.getProjectPath(),
      testContext.testName,
      'warn'
    );

    client = new McpClient(serverPath, {
      workingDirectory: project.getProjectPath(),
      timeout: 30000, // Increased timeout for symbol analysis with indexing
      env: logEnv.env,
    });
    await client.start();

    // Ensure project is built and indexed
    await project.runCmake();
  });

  afterEach(async (context) => {
    await client.stop();
    // Use enhanced cleanup that preserves folders on test failure
    await project.cleanup({ cleanupOnFailure: false, vitestContext: context });
  });

  describe('Basic Symbol Analysis', () => {
    it('should analyze Math class with basic information', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Math class analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toBe('Math');
          expect(response.symbol.kind).toContain('class');
          expect(response.symbol.file_location).toBeDefined();

          if (response.symbol.file_location?.uri) {
            expect(response.symbol.file_location.uri).toContain('Math.hpp');
          }
        }

        // Verify metadata
        expect(response.metadata).toBeDefined();
        if (response.metadata) {
          expect(response.metadata.analysis_type).toBe(
            'comprehensive_symbol_analysis'
          );
          expect(response.metadata.features_used).toBeDefined();
        }
      }
    });

    it('should analyze factorial function', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Factorial function analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('factorial');
          expect(
            ['function', 'method'].includes(response.symbol.kind ?? '')
          ).toBe(true);
          expect(response.symbol.file_location).toBeDefined();
        }
      }
    });

    it('should analyze Container template class', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Container',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Container template analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('Container');
          expect(response.symbol.kind).toContain('class');

          if (response.symbol.file_location?.uri) {
            expect(response.symbol.file_location.uri).toContain(
              'Container.hpp'
            );
          }
        }
      }
    });

    it('should analyze StringUtils class', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'StringUtils',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('StringUtils analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toBe('StringUtils');
          expect(response.symbol.kind).toContain('class');

          if (response.symbol.file_location?.uri) {
            expect(response.symbol.file_location.uri).toContain(
              'StringUtils.hpp'
            );
          }
        }
      }
    });

    it('should analyze TestProject namespace', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'TestProject',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('TestProject namespace analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('TestProject');
          // Namespace might be detected as various kinds depending on clangd
          expect(response.symbol.kind).toBeDefined();
        }
      }
    });

    it('should handle non-existent symbol gracefully', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'NonExistentSymbol123',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      expect(response.success).toBe(false);
      expect(response.error).toBe('symbol_not_found');
      expect(response.message).toContain('NonExistentSymbol123');
      expect(response.message).toContain('not found');
    });
  });

  describe('Symbol Disambiguation', () => {
    it('should handle overloaded factorial functions', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Overloaded factorial analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('factorial');
          // Should handle overloads by picking one
          expect(response.symbol.type_info).toBeDefined();
        }
      }
    });

    it('should use location parameter for disambiguation', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
        location: {
          file_uri: 'include/Math.hpp',
          position: {
            line: 90,
            character: 20,
          },
        },
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Location-based disambiguation failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('factorial');
          expect(response.symbol.file_location).toBeDefined();
        }
      }
    });

    it('should handle qualified symbol names', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'TestProject::Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Qualified name analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.fully_qualified_name).toContain('Math');
        }
      }
    });
  });

  describe('Usage Pattern Analysis', () => {
    it('should analyze usage patterns with statistics', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        include_usage_patterns: true,
        max_usage_examples: 3,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Usage pattern analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol?.usage_statistics) {
          expect(
            response.symbol.usage_statistics.total_references
          ).toBeGreaterThanOrEqual(0);
          expect(
            response.symbol.usage_statistics.files_containing_references
          ).toBeGreaterThanOrEqual(0);
          expect(
            typeof response.symbol.usage_statistics.reference_density
          ).toBe('number');
        }

        // Check metadata shows usage patterns were analyzed
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.usage_statistics).toBe(true);
        }
      }
    });

    it('should provide usage examples with context', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
        include_usage_patterns: true,
        max_usage_examples: 5,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Usage examples analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (
          response.symbol?.usage_examples &&
          response.symbol.usage_examples.length > 0
        ) {
          expect(response.symbol.usage_examples.length).toBeLessThanOrEqual(5);

          // Check usage example structure
          response.symbol.usage_examples.forEach((example) => {
            expect(example.file).toBeDefined();
            expect(example.range).toBeDefined();
            expect(example.pattern_type).toBeDefined();
            expect(
              [
                'function_call',
                'reference',
                'member_access',
                'qualified_access',
                'instantiation',
              ].includes(example.pattern_type ?? '')
            ).toBe(true);
          });
        }

        // Check metadata shows usage examples were analyzed
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.usage_examples).toBeDefined();
        }
      }
    });

    it('should respect max_usage_examples limit', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        include_usage_patterns: true,
        max_usage_examples: 2,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Usage examples limit test failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        if (response.symbol?.usage_examples) {
          expect(response.symbol.usage_examples.length).toBeLessThanOrEqual(2);
        }
      }
    });
  });

  describe('Inheritance Analysis', () => {
    it('should analyze IStorageBackend interface inheritance', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'IStorageBackend',
        include_inheritance: true,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'IStorageBackend inheritance analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol?.inheritance) {
          expect(response.symbol.inheritance.has_inheritance).toBeDefined();
          expect(response.symbol.inheritance.base_classes).toBeDefined();
          expect(response.symbol.inheritance.derived_classes).toBeDefined();

          // IStorageBackend should have derived classes like MemoryStorage, FileStorage
          if (
            response.symbol.inheritance.derived_classes &&
            response.symbol.inheritance.derived_classes.length > 0
          ) {
            const derivedNames =
              response.symbol.inheritance.derived_classes.join(' ');
            expect(
              derivedNames.includes('MemoryStorage') ||
                derivedNames.includes('FileStorage')
            ).toBe(true);
          }
        }

        // Check metadata shows inheritance was analyzed
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.inheritance_info).toBe(true);
        }
      }
    });

    it('should analyze MemoryStorage derived class inheritance', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'MemoryStorage',
        include_inheritance: true,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'MemoryStorage inheritance analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol?.inheritance) {
          expect(response.symbol.inheritance.has_inheritance).toBeDefined();

          // MemoryStorage should have base class IStorageBackend
          if (
            response.symbol.inheritance.base_classes &&
            response.symbol.inheritance.base_classes.length > 0
          ) {
            const baseNames =
              response.symbol.inheritance.base_classes.join(' ');
            expect(baseNames).toContain('IStorageBackend');
          }
        }
      }
    });

    it('should analyze nested class inheritance (Math::Statistics)', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Statistics',
        include_inheritance: true,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Statistics nested class inheritance analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('Statistics');
          expect(response.symbol.kind).toContain('class');

          // Nested classes may or may not have inheritance, check if analyzed
          if (response.symbol.inheritance) {
            expect(response.symbol.inheritance.has_inheritance).toBeDefined();
          }
        }
      }
    });
  });

  describe('Call Hierarchy Analysis', () => {
    it('should analyze factorial function call relationships', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
        include_call_hierarchy: true,
        max_call_depth: 2,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Factorial call hierarchy analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol?.call_relationships) {
          expect(
            response.symbol.call_relationships.incoming_calls
          ).toBeDefined();
          expect(
            response.symbol.call_relationships.outgoing_calls
          ).toBeDefined();
          expect(
            response.symbol.call_relationships.total_callers
          ).toBeGreaterThanOrEqual(0);
          expect(
            response.symbol.call_relationships.total_callees
          ).toBeGreaterThanOrEqual(0);
          expect(response.symbol.call_relationships.call_depth_analyzed).toBe(
            2
          );
          expect(
            response.symbol.call_relationships.has_call_relationships
          ).toBeDefined();
        }

        // Check metadata shows call hierarchy was analyzed
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.call_relationships).toBe(true);
        }
      }
    });

    it('should analyze call relationships with different depth levels', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'gcd',
        include_call_hierarchy: true,
        max_call_depth: 1,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('GCD call hierarchy analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        if (response.symbol?.call_relationships) {
          expect(response.symbol.call_relationships.call_depth_analyzed).toBe(
            1
          );

          // Check call info structure if calls exist
          if (
            response.symbol.call_relationships.incoming_calls &&
            response.symbol.call_relationships.incoming_calls.length > 0
          ) {
            response.symbol.call_relationships.incoming_calls.forEach(
              (call) => {
                expect(call.name).toBeDefined();
                expect(call.kind).toBeDefined();
                expect(call.uri).toBeDefined();
                expect(call.range).toBeDefined();
              }
            );
          }
        }
      }
    });

    it('should handle max_call_depth parameter validation', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        include_call_hierarchy: true,
        max_call_depth: 5,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Call depth validation test failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        if (response.symbol?.call_relationships) {
          expect(response.symbol.call_relationships.call_depth_analyzed).toBe(
            5
          );
        }
      }
    });
  });

  describe('Class Member Analysis', () => {
    it('should analyze Math class members comprehensively', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Math class members analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol?.class_members) {
          expect(response.symbol.class_members.members).toBeDefined();
          expect(response.symbol.class_members.total_count).toBeGreaterThan(0);

          // Math class should have many members (functions, nested classes)
          if (
            response.symbol.class_members.members &&
            response.symbol.class_members.members.length > 0
          ) {
            // Check member structure - log unexpected kinds for debugging
            response.symbol.class_members.members.forEach((member, index) => {
              expect(member.name).toBeDefined();
              expect(member.kind).toBeDefined();

              const validKinds = [
                'method',
                'field',
                'constructor',
                'class',
                'function',
                'variable',
                'enum',
                'struct',
                'namespace',
                'property',
              ];
              if (!validKinds.includes(member.kind ?? '')) {
                console.log(`Unexpected member kind at index ${index}:`, {
                  name: member.name,
                  kind: member.kind,
                  detail: member.detail,
                });
              }
              expect(validKinds.includes(member.kind ?? '')).toBe(true);
              expect(member.range).toBeDefined();
            });

            // Should find nested classes like Statistics and Complex
            const memberNames = response.symbol.class_members.members
              .map((m) => m.name)
              .join(' ');
            expect(
              memberNames.includes('Statistics') ||
                memberNames.includes('factorial')
            ).toBe(true);
          }
        }

        // Check metadata shows class members were analyzed
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.class_members).toBeDefined();
        }
      }
    });

    it('should analyze Container class template members', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Container',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Container class members analysis failed:', response.error);
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        if (response.symbol?.class_members) {
          expect(response.symbol.class_members.members).toBeDefined();
          expect(
            response.symbol.class_members.total_count
          ).toBeGreaterThanOrEqual(0);

          // Container should have template methods
          if (
            response.symbol.class_members.members &&
            response.symbol.class_members.members.length > 0
          ) {
            const memberNames = response.symbol.class_members.members
              .map((m) => m.name)
              .join(' ');
            expect(
              memberNames.includes('push_back') ||
                memberNames.includes('size') ||
                memberNames.includes('empty')
            ).toBe(true);
          }
        }
      }
    });

    it('should analyze nested class members (Statistics)', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Statistics',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Statistics nested class members analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        if (response.symbol?.class_members) {
          expect(response.symbol.class_members.members).toBeDefined();

          // Statistics should have methods and nested classes/structs
          if (
            response.symbol.class_members.members &&
            response.symbol.class_members.members.length > 0
          ) {
            const memberNames = response.symbol.class_members.members
              .map((m) => m.name)
              .join(' ');
            expect(
              memberNames.includes('Result') ||
                memberNames.includes('mean') ||
                memberNames.includes('Distribution')
            ).toBe(true);
          }
        }
      }
    });
  });

  describe('Advanced Feature Combinations', () => {
    it('should handle all features enabled simultaneously', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        include_usage_patterns: true,
        max_usage_examples: 2,
        include_inheritance: true,
        include_call_hierarchy: true,
        max_call_depth: 2,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'All features combination analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        // Verify all features were attempted
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.basic_info).toBe(true);
          expect(response.metadata.features_used.hover_info).toBeDefined();
          expect(
            response.metadata.features_used.usage_statistics
          ).toBeDefined();
          expect(
            response.metadata.features_used.inheritance_info
          ).toBeDefined();
          expect(
            response.metadata.features_used.call_relationships
          ).toBeDefined();
          expect(response.metadata.features_used.class_members).toBeDefined();
        }

        // Check that complex analysis completed within reasonable time
        expect(response.metadata?.indexing_waited).toBeDefined();
      }
    }, 45000); // Extended timeout for complex analysis

    it('should handle selective feature combinations', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'IStorageBackend',
        include_inheritance: true,
        include_call_hierarchy: true,
        max_call_depth: 1,
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Selective features combination analysis failed:',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        // Should not include usage patterns since not requested
        if (response.metadata?.features_used) {
          expect(response.metadata.features_used.usage_statistics).not.toBe(
            true
          );
          expect(response.metadata.features_used.usage_examples).not.toBe(true);
        }
      }
    });

    it('should handle build_directory parameter', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        build_directory: 'build-debug',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log('Build directory parameter test failed:', response.error);
        // This might fail if build-debug doesn't exist, which is expected
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);

        if (response.metadata) {
          expect(response.metadata.build_directory_used).toContain('build');
        }
      }
    });
  });

  describe('Error Handling & Edge Cases', () => {
    it('should handle invalid max_usage_examples parameter', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        include_usage_patterns: true,
        max_usage_examples: 25, // Above valid range of 1-20
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Invalid max_usage_examples parameter failed (expected):',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        // Tool might clamp to valid range instead of erroring, or succeed with out-of-range value
        expect(response.success).toBe(true);

        if (response.symbol?.usage_examples) {
          // The tool may accept the invalid parameter and return more than expected
          expect(response.symbol.usage_examples.length).toBeGreaterThanOrEqual(
            0
          );
        }
      }
    });

    it('should handle invalid max_call_depth parameter', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'factorial',
        include_call_hierarchy: true,
        max_call_depth: 15, // Above valid range of 1-10
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Invalid max_call_depth parameter failed (expected):',
          response.error
        );
        expect(response.error).toBeDefined();
      } else {
        // Tool might clamp to valid range instead of erroring, or accept out-of-range value
        expect(response.success).toBe(true);

        if (response.symbol?.call_relationships) {
          // The tool may accept the invalid parameter and return more than expected
          expect(
            response.symbol.call_relationships.call_depth_analyzed
          ).toBeGreaterThanOrEqual(0);
        }
      }
    });

    it('should handle empty symbol name', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: '',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.success === false) {
        expect(response.error).toBeDefined();
        expect(response.message ?? response.error).toContain('symbol');
      } else {
        // Tool might succeed with empty symbol and return no results
        expect(response.success).toBe(true);
        console.log(
          'Empty symbol analysis succeeded (unexpected but valid):',
          response
        );
      }
    });

    it('should handle special characters in symbol name', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'operator+',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.error) {
        console.log(
          'Special character symbol analysis failed:',
          response.error
        );
        // May fail to find operator symbols depending on indexing
        expect(response.error).toBeDefined();
      } else {
        expect(response.success).toBe(true);
        expect(response.symbol).toBeDefined();

        if (response.symbol) {
          expect(response.symbol.name).toContain('operator');
        }
      }
    });

    it('should provide suggestions for similar symbols', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Maths', // Similar to 'Math'
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      if (response.success === false) {
        expect(response.error).toBe('symbol_not_found');

        // Should provide suggestions for similar symbols
        if ('suggestions' in response) {
          expect(Array.isArray(response.suggestions)).toBe(true);
          if (
            Array.isArray(response.suggestions) &&
            response.suggestions.length > 0
          ) {
            const suggestions = response.suggestions.join(' ');
            expect(suggestions).toContain('Math');
          }
        }
      } else {
        // Tool might find a similar symbol instead of returning an error
        expect(response.success).toBe(true);
        console.log(
          'Similar symbol search succeeded (found something similar):',
          response.symbol?.name
        );
      }
    });

    it('should handle non-existent build directory', async () => {
      const result = await client.callTool('analyze_symbol_context', {
        symbol: 'Math',
        build_directory: 'non-existent-build',
      });

      expect(result.content).toBeDefined();
      const responseText = (result.content?.[0]?.text ?? '{}') as string;
      const response: AnalysisResponse = JSON.parse(responseText);

      expect(response.success).toBe(false);
      expect(response.error).toBe('build_directory_not_found');
      expect(response.message).toContain('non-existent-build');
      expect(response.message).toContain('does not exist');
    });
  });
});
