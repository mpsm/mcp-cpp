import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['node_modules/', 'src/**/__tests__/**'],
    },
    timeout: 30000, // MCP server startup and CMake operations can be slow
    testTimeout: 30000,
    hookTimeout: 30000,
  },
});