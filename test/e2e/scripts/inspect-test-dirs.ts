#!/usr/bin/env tsx

import { promises as fs } from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Helper script to inspect test directories and their metadata
 * Usage: npx tsx scripts/inspect-test-dirs.ts [options]
 */

interface TestMetadata {
  testName: string;
  describe?: string;
  timestamp: number;
  testId: string;
  uuid: string;
  projectPath: string;
  createdAt: string;
  nodeVersion: string;
  platform: string;
}

interface DebugInfo {
  preservedAt: string;
  reason: string;
  projectPath: string;
  testCase?: {
    testCase: string;
    testFile: string;
    fullName: string;
    errors: string[];
    duration: number;
  };
}

interface LogFileInfo {
  name: string;
  size: number;
  lines: number;
  modified: Date;
}

interface DirectoryInfo {
  name: string;
  path: string;
  size: number;
  created: Date;
  modified: Date;
  metadata?: TestMetadata;
  debugInfo?: DebugInfo;
  logFiles: LogFileInfo[];
  error?: string;
}

interface InspectionOptions {
  verbose: boolean;
  showLogs: boolean;
  clean: boolean;
  dryRun: boolean;
  help: boolean;
}

const COLORS = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  dim: '\x1b[2m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  white: '\x1b[37m',
} as const;

function colorize(text: string, color: keyof typeof COLORS): string {
  return `${COLORS[color] || ''}${text}${COLORS.reset}`;
}

function formatDate(timestamp: number | Date | string): string {
  if (!timestamp) return 'N/A';
  return new Date(timestamp).toLocaleString();
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

async function getDirectorySize(dirPath: string): Promise<number> {
  let totalSize = 0;
  
  try {
    const entries = await fs.readdir(dirPath, { withFileTypes: true });
    
    for (const entry of entries) {
      const fullPath = path.join(dirPath, entry.name);
      
      if (entry.isDirectory()) {
        totalSize += await getDirectorySize(fullPath);
      } else if (entry.isFile()) {
        const stats = await fs.stat(fullPath);
        totalSize += stats.size;
      }
    }
  } catch (error) {
    // Ignore errors (e.g., permission denied)
  }
  
  return totalSize;
}

async function getLogFileInfo(dirPath: string): Promise<LogFileInfo[]> {
  const logFiles: LogFileInfo[] = [];
  
  try {
    const entries = await fs.readdir(dirPath);
    
    for (const entry of entries) {
      if (entry.endsWith('.log')) {
        const logPath = path.join(dirPath, entry);
        const stats = await fs.stat(logPath);
        const size = stats.size;
        
        // Try to count lines
        let lineCount = 0;
        try {
          const content = await fs.readFile(logPath, 'utf-8');
          lineCount = content.split('\n').filter(line => line.trim()).length;
        } catch {
          // If we can't read the file, skip line counting
        }
        
        logFiles.push({
          name: entry,
          size,
          lines: lineCount,
          modified: stats.mtime,
        });
      }
    }
  } catch (error) {
    // Ignore errors
  }
  
  return logFiles;
}

async function inspectTestDirectory(dirPath: string, dirName: string): Promise<DirectoryInfo> {
  const fullPath = path.resolve(dirPath);
  
  try {
    const stats = await fs.stat(fullPath);
    const dirSize = await getDirectorySize(fullPath);
    
    // Try to read test metadata
    let metadata: TestMetadata | undefined;
    try {
      const metadataPath = path.join(fullPath, '.test-info.json');
      const metadataContent = await fs.readFile(metadataPath, 'utf-8');
      metadata = JSON.parse(metadataContent) as TestMetadata;
    } catch {
      // No metadata file or invalid JSON
    }
    
    // Check for debug preservation
    let debugInfo: DebugInfo | undefined;
    try {
      const debugPath = path.join(fullPath, '.debug-preserved.json');
      const debugContent = await fs.readFile(debugPath, 'utf-8');
      debugInfo = JSON.parse(debugContent) as DebugInfo;
    } catch {
      // No debug file
    }
    
    // Get log file information
    const logFiles = await getLogFileInfo(fullPath);
    
    return {
      name: dirName,
      path: fullPath,
      size: dirSize,
      created: stats.birthtime,
      modified: stats.mtime,
      metadata,
      debugInfo,
      logFiles,
    };
  } catch (error) {
    return {
      name: dirName,
      path: fullPath,
      size: 0,
      created: new Date(0),
      modified: new Date(0),
      logFiles: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function findTestDirectories(baseDir: string): Promise<DirectoryInfo[]> {
  const tempDir = path.join(baseDir, 'temp');
  const directories: DirectoryInfo[] = [];
  
  try {
    const entries = await fs.readdir(tempDir, { withFileTypes: true });
    
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const dirPath = path.join(tempDir, entry.name);
        const info = await inspectTestDirectory(dirPath, entry.name);
        directories.push(info);
      }
    }
  } catch (error) {
    throw new Error(`Failed to read temp directory: ${error instanceof Error ? error.message : String(error)}`);
  }
  
  return directories;
}

function printDirectoryInfo(info: DirectoryInfo, options: InspectionOptions): void {
  const { verbose, showLogs } = options;
  
  if (info.error) {
    console.log(colorize(`❌ ${info.name}`, 'red'));
    console.log(colorize(`   Error: ${info.error}`, 'red'));
    return;
  }
  
  // Header
  const debugIcon = info.debugInfo ? '🔍' : '';
  const metadataIcon = info.metadata ? '📝' : '❓';
  console.log(colorize(`${debugIcon}${metadataIcon} ${info.name}`, 'cyan'));
  
  // Basic info
  console.log(colorize(`   Path: ${info.path}`, 'dim'));
  console.log(colorize(`   Size: ${formatSize(info.size)}`, 'dim'));
  console.log(colorize(`   Created: ${formatDate(info.created)}`, 'dim'));
  console.log(colorize(`   Modified: ${formatDate(info.modified)}`, 'dim'));
  
  // Metadata
  if (info.metadata) {
    console.log(colorize(`   Test: ${info.metadata.testName}`, 'green'));
    if (info.metadata.describe) {
      console.log(colorize(`   Suite: ${info.metadata.describe}`, 'green'));
    }
    if (info.metadata.testId) {
      console.log(colorize(`   ID: ${info.metadata.testId}`, 'green'));
    }
    if (verbose) {
      console.log(colorize(`   UUID: ${info.metadata.uuid}`, 'dim'));
      console.log(colorize(`   Platform: ${info.metadata.platform}`, 'dim'));
      console.log(colorize(`   Node: ${info.metadata.nodeVersion}`, 'dim'));
      console.log(colorize(`   Created: ${formatDate(info.metadata.createdAt)}`, 'dim'));
    }
  } else {
    console.log(colorize(`   ⚠️  No metadata found (.test-info.json missing)`, 'yellow'));
  }
  
  // Debug info
  if (info.debugInfo) {
    console.log(colorize(`   🔍 PRESERVED FOR DEBUGGING`, 'yellow'));
    console.log(colorize(`   Reason: ${info.debugInfo.reason}`, 'yellow'));
    console.log(colorize(`   Preserved: ${formatDate(info.debugInfo.preservedAt)}`, 'yellow'));
    
    // Show specific test case information if available
    if (info.debugInfo.testCase) {
      const tc = info.debugInfo.testCase;
      console.log(colorize(`   🎯 Failed Test Case: ${tc.testCase}`, 'red'));
      console.log(colorize(`   📄 Test File: ${tc.testFile}`, 'dim'));
      console.log(colorize(`   📍 Full Path: ${tc.fullName}`, 'dim'));
      if (tc.errors.length > 0) {
        console.log(colorize(`   ❌ Error: ${tc.errors[0]}`, 'red'));
      }
      if (tc.duration > 0) {
        console.log(colorize(`   ⏱️  Duration: ${tc.duration}ms`, 'dim'));
      }
    }
  }
  
  // Log files
  if (info.logFiles.length > 0) {
    console.log(colorize(`   📋 Log files:`, 'blue'));
    for (const log of info.logFiles) {
      const sizeStr = formatSize(log.size);
      const linesStr = log.lines > 0 ? ` (${log.lines} lines)` : '';
      console.log(colorize(`     • ${log.name}: ${sizeStr}${linesStr}`, 'blue'));
      
      if (showLogs && verbose) {
        console.log(colorize(`       Modified: ${formatDate(log.modified)}`, 'dim'));
      }
    }
  }
  
  console.log(); // Empty line for spacing
}

function printSummary(directories: DirectoryInfo[]): void {
  const total = directories.length;
  const withMetadata = directories.filter(d => d.metadata).length;
  const withDebugInfo = directories.filter(d => d.debugInfo).length;
  const withErrors = directories.filter(d => d.error).length;
  const totalSize = directories.reduce((sum, d) => sum + (d.size || 0), 0);
  
  console.log(colorize('📊 SUMMARY', 'bright'));
  console.log(colorize(`   Total directories: ${total}`, 'white'));
  console.log(colorize(`   With metadata: ${withMetadata}`, 'green'));
  console.log(colorize(`   Preserved for debugging: ${withDebugInfo}`, 'yellow'));
  console.log(colorize(`   With errors: ${withErrors}`, 'red'));
  console.log(colorize(`   Total size: ${formatSize(totalSize)}`, 'white'));
  
  if (withDebugInfo > 0) {
    console.log(colorize(`\n   💡 Tip: Use --clean to remove preserved directories`, 'dim'));
  }
}

async function cleanupDirectory(dirPath: string, dryRun: boolean): Promise<void> {
  if (dryRun) {
    console.log(colorize(`Would remove: ${dirPath}`, 'yellow'));
    return;
  }
  
  try {
    await fs.rm(dirPath, { recursive: true, force: true });
    console.log(colorize(`✅ Removed: ${dirPath}`, 'green'));
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.log(colorize(`❌ Failed to remove ${dirPath}: ${errorMessage}`, 'red'));
  }
}

function parseArgs(args: string[]): InspectionOptions {
  return {
    verbose: args.includes('--verbose') || args.includes('-v'),
    showLogs: args.includes('--logs') || args.includes('-l'),
    clean: args.includes('--clean'),
    dryRun: args.includes('--dry-run'),
    help: args.includes('--help') || args.includes('-h'),
  };
}

function printHelp(): void {
  console.log(`
${colorize('MCP C++ Test Directory Inspector', 'bright')}

Usage: npx tsx scripts/inspect-test-dirs.ts [options]

Options:
  -v, --verbose     Show detailed information
  -l, --logs        Show log file details
  --clean           Remove all test directories (use with caution!)
  --dry-run         Show what would be cleaned without actually removing
  -h, --help        Show this help message

Examples:
  npx tsx scripts/inspect-test-dirs.ts                    # Basic inspection
  npx tsx scripts/inspect-test-dirs.ts --verbose          # Detailed view
  npx tsx scripts/inspect-test-dirs.ts --logs             # Include log file info
  npx tsx scripts/inspect-test-dirs.ts --clean --dry-run  # Preview cleanup
  npx tsx scripts/inspect-test-dirs.ts --clean            # Actually cleanup

Icons:
  📝 = Directory with test metadata
  ❓ = Directory without metadata
  🔍 = Directory preserved for debugging
  ❌ = Directory with errors
`);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const options = parseArgs(args);
  
  if (options.help) {
    printHelp();
    return;
  }
  
  const baseDir = path.resolve(__dirname, '..');
  
  try {
    console.log(colorize('🔍 Inspecting test directories...', 'bright'));
    console.log(colorize(`Base directory: ${baseDir}`, 'dim'));
    console.log();
    
    const directories = await findTestDirectories(baseDir);
    
    if (directories.length === 0) {
      console.log(colorize('No test directories found in temp/ folder', 'yellow'));
      console.log(colorize('Run some tests first to generate test directories', 'dim'));
      return;
    }
    
    // Sort by creation time (newest first)
    directories.sort((a, b) => b.created.getTime() - a.created.getTime());
    
    if (options.clean) {
      console.log(colorize('🧹 Cleaning up test directories...', 'bright'));
      
      if (options.dryRun) {
        console.log(colorize('DRY RUN - No files will be actually removed', 'yellow'));
        console.log();
      }
      
      for (const dir of directories) {
        if (!dir.error) {
          await cleanupDirectory(dir.path, options.dryRun);
        }
      }
      
      if (!options.dryRun) {
        console.log(colorize('\n✅ Cleanup complete', 'green'));
      }
    } else {
      // Display directory information
      for (const dir of directories) {
        printDirectoryInfo(dir, options);
      }
      
      printSummary(directories);
    }
    
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.error(colorize(`❌ Error: ${errorMessage}`, 'red'));
    process.exit(1);
  }
}

// Handle unhandled promise rejections
process.on('unhandledRejection', (reason, promise) => {
  console.error(colorize('❌ Unhandled Rejection at:', 'red'), promise, colorize('reason:', 'red'), reason);
  process.exit(1);
});

main().catch(console.error);