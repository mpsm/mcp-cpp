#!/usr/bin/env node

import { promises as fs } from 'fs';
import * as path from 'path';
import * as fse from 'fs-extra';

interface TestMetadata {
  testName: string;
  describe?: string;
  timestamp: number;
  testId: string;
  uuid: string;
  projectPath: string;
  createdAt: string;
  status: 'running' | 'completed' | 'failed' | 'preserved';
  lastUpdated?: string;
  completedAt?: string;
  statusReason?: string;
  folderName: string;
}

interface CleanupOptions {
  dryRun?: boolean;
  maxAge?: number; // in minutes
  onlyCompleted?: boolean;
  preserveDebug?: boolean;
  verbose?: boolean;
}

class TestFolderCleanup {
  private tempDir: string;

  constructor(tempDir: string = path.join(process.cwd(), 'temp')) {
    this.tempDir = tempDir;
  }

  async cleanup(options: CleanupOptions = {}): Promise<void> {
    const {
      dryRun = false,
      maxAge = 60, // 1 hour default
      onlyCompleted = true,
      preserveDebug = true,
      verbose = false,
    } = options;

    console.log(`🧹 Cleaning up test folders in: ${this.tempDir}`);
    console.log(`⚙️  Options: dryRun=${dryRun}, maxAge=${maxAge}min, onlyCompleted=${onlyCompleted}, preserveDebug=${preserveDebug}`);

    try {
      const folders = await this.getFolders();
      if (folders.length === 0) {
        console.log('📁 No test folders found');
        return;
      }

      console.log(`📁 Found ${folders.length} test folders`);

      let cleanedCount = 0;
      let preservedCount = 0;
      let errorCount = 0;

      for (const folder of folders) {
        try {
          const folderPath = path.join(this.tempDir, folder);
          const shouldClean = await this.shouldCleanFolder(folderPath, options);
          
          if (shouldClean.clean) {
            if (verbose) {
              console.log(`🗑️  ${dryRun ? 'Would clean' : 'Cleaning'}: ${folder} (${shouldClean.reason})`);
            }
            
            if (!dryRun) {
              await fse.remove(folderPath);
            }
            cleanedCount++;
          } else {
            if (verbose) {
              console.log(`✋ Preserving: ${folder} (${shouldClean.reason})`);
            }
            preservedCount++;
          }
        } catch (error) {
          console.error(`❌ Error processing ${folder}:`, error);
          errorCount++;
        }
      }

      console.log(`\n📊 Summary:`);
      console.log(`   ${dryRun ? 'Would clean' : 'Cleaned'}: ${cleanedCount} folders`);
      console.log(`   Preserved: ${preservedCount} folders`);
      if (errorCount > 0) {
        console.log(`   Errors: ${errorCount} folders`);
      }
      
      if (dryRun && cleanedCount > 0) {
        console.log(`\n💡 Run without --dry-run to actually clean the folders`);
      }
    } catch (error) {
      console.error('❌ Error during cleanup:', error);
    }
  }

  private async getFolders(): Promise<string[]> {
    try {
      const entries = await fs.readdir(this.tempDir, { withFileTypes: true });
      return entries
        .filter(entry => entry.isDirectory())
        .map(entry => entry.name);
    } catch (error) {
      if ((error as any).code === 'ENOENT') {
        return [];
      }
      throw error;
    }
  }

  private async shouldCleanFolder(folderPath: string, options: CleanupOptions): Promise<{clean: boolean, reason: string}> {
    const { maxAge = 60, onlyCompleted = true, preserveDebug = true } = options;

    // Check if folder has debug preservation marker
    const debugPreservedPath = path.join(folderPath, '.debug-preserved.json');
    if (preserveDebug && await this.fileExists(debugPreservedPath)) {
      return { clean: false, reason: 'debug preserved' };
    }

    // Check folder age using filesystem stats
    const folderStats = await fs.stat(folderPath);
    const ageMinutes = (Date.now() - folderStats.mtime.getTime()) / (1000 * 60);
    
    if (ageMinutes < maxAge) {
      return { clean: false, reason: `too recent (${Math.round(ageMinutes)}min old)` };
    }

    // Check test metadata if available
    const metadataPath = path.join(folderPath, '.test-info.json');
    if (await this.fileExists(metadataPath)) {
      try {
        const metadata: TestMetadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
        
        // Never clean preserved tests
        if (metadata.status === 'preserved') {
          return { clean: false, reason: 'test preserved' };
        }
        
        // Only clean completed tests if onlyCompleted is true
        if (onlyCompleted && metadata.status !== 'completed') {
          return { clean: false, reason: `test status: ${metadata.status}` };
        }
        
        return { clean: true, reason: `old ${metadata.status} test (${Math.round(ageMinutes)}min old)` };
      } catch (error) {
        // If metadata is corrupted, fall back to age-based cleanup
        return { clean: true, reason: `corrupted metadata, old folder (${Math.round(ageMinutes)}min old)` };
      }
    }

    // No metadata - could be from old tests or interrupted tests
    // Only clean if very old to be safe
    const safeMaxAge = Math.max(maxAge, 120); // At least 2 hours for folders without metadata
    if (ageMinutes > safeMaxAge) {
      return { clean: true, reason: `very old folder without metadata (${Math.round(ageMinutes)}min old)` };
    }

    return { clean: false, reason: `no metadata, but recent (${Math.round(ageMinutes)}min old)` };
  }

  private async fileExists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  async inspect(verbose: boolean = false): Promise<void> {
    console.log(`🔍 Inspecting test folders in: ${this.tempDir}`);
    
    const folders = await this.getFolders();
    if (folders.length === 0) {
      console.log('📁 No test folders found');
      return;
    }

    console.log(`📁 Found ${folders.length} test folders:\n`);

    for (const folder of folders) {
      const folderPath = path.join(this.tempDir, folder);
      const metadataPath = path.join(folderPath, '.test-info.json');
      const debugPreservedPath = path.join(folderPath, '.debug-preserved.json');
      
      console.log(`📂 ${folder}`);
      
      if (await this.fileExists(metadataPath)) {
        try {
          const metadata: TestMetadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
          console.log(`   Test: ${metadata.testName}`);
          console.log(`   Status: ${metadata.status}`);
          console.log(`   Created: ${metadata.createdAt}`);
          if (metadata.lastUpdated) {
            console.log(`   Updated: ${metadata.lastUpdated}`);
          }
          if (verbose) {
            console.log(`   Describe: ${metadata.describe || 'none'}`);
            console.log(`   UUID: ${metadata.uuid}`);
            console.log(`   Path: ${metadata.projectPath}`);
          }
        } catch (error) {
          console.log(`   ❌ Corrupted metadata: ${error}`);
        }
      } else {
        console.log(`   ❌ No metadata found`);
      }

      if (await this.fileExists(debugPreservedPath)) {
        try {
          const debugInfo = JSON.parse(await fs.readFile(debugPreservedPath, 'utf-8'));
          console.log(`   🔍 Debug preserved: ${debugInfo.reason}`);
        } catch (error) {
          console.log(`   🔍 Debug preserved (corrupted info)`);
        }
      }

      const stats = await fs.stat(folderPath);
      const ageMinutes = (Date.now() - stats.mtime.getTime()) / (1000 * 60);
      console.log(`   Age: ${Math.round(ageMinutes)} minutes`);
      
      console.log('');
    }
  }
}

// CLI interface
async function main() {
  const args = process.argv.slice(2);
  const command = args[0] || 'help';

  const cleanup = new TestFolderCleanup();

  switch (command) {
    case 'inspect':
      await cleanup.inspect(args.includes('--verbose'));
      break;
    
    case 'cleanup':
      await cleanup.cleanup({
        dryRun: args.includes('--dry-run'),
        maxAge: args.includes('--max-age') ? parseInt(args[args.indexOf('--max-age') + 1]) : 60,
        onlyCompleted: !args.includes('--include-failed'),
        preserveDebug: !args.includes('--no-preserve-debug'),
        verbose: args.includes('--verbose'),
      });
      break;
    
    case 'help':
    default:
      console.log(`
🧹 Test Folder Cleanup Tool

Usage:
  npm run cleanup:inspect [--verbose]     # Inspect test folders
  npm run cleanup:dry [options]          # Preview cleanup (dry run)
  npm run cleanup [options]              # Actually clean up folders

Options:
  --dry-run                  # Preview what would be cleaned
  --max-age <minutes>        # Maximum age in minutes (default: 60)
  --include-failed          # Include failed tests in cleanup
  --no-preserve-debug       # Don't preserve debug-marked folders
  --verbose                 # Show detailed output

Examples:
  npm run cleanup:inspect --verbose
  npm run cleanup:dry --max-age 30 --verbose
  npm run cleanup --max-age 120 --include-failed
      `);
      break;
  }
}

// ES module version of require.main check
if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch(console.error);
}

export { TestFolderCleanup };