import { beforeEach, afterEach } from 'vitest';

// Extend global namespace for test state tracking
declare global {
  var __testStates: Map<string, 'running' | 'passed' | 'failed'>;
  var __currentTestName: string | undefined;
}

// Initialize global test states if not already done
if (!globalThis.__testStates) {
  globalThis.__testStates = new Map<string, 'running' | 'passed' | 'failed'>();
}

beforeEach((context) => {
  // Store current test name for global access
  const testName = context.task?.name || 'unknown-test';
  globalThis.__currentTestName = testName;
  globalThis.__testStates.set(testName, 'running');
});

afterEach((context) => {
  const testName =
    context.task?.name ?? globalThis.__currentTestName ?? 'unknown-test';

  // Update test status based on whether any errors occurred
  if (context.task?.result?.state === 'fail') {
    globalThis.__testStates.set(testName, 'failed');
  } else {
    globalThis.__testStates.set(testName, 'passed');
  }

  // Clear current test name
  globalThis.__currentTestName = undefined;
});

/**
 * Get the current test status
 */
export function getCurrentTestStatus():
  | 'running'
  | 'passed'
  | 'failed'
  | 'unknown' {
  if (!globalThis.__currentTestName) {
    return 'unknown';
  }
  return globalThis.__testStates.get(globalThis.__currentTestName) ?? 'unknown';
}

/**
 * Check if the current test has failed
 */
export function hasCurrentTestFailed(): boolean {
  return getCurrentTestStatus() === 'failed';
}

/**
 * Get test status by name
 */
export function getTestStatus(
  testName: string
): 'running' | 'passed' | 'failed' | 'unknown' {
  return globalThis.__testStates.get(testName) ?? 'unknown';
}
