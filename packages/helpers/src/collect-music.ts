#!/usr/bin/env tsx

import * as fs from "fs";
import * as path from "path";

// Configuration
const CONFIG = {
  destination: "/volume1/music/library",
  logFile: "/volume1/music/consolidation.log",
  duplicatesFile: "/volume1/music/duplicates.json",

  // Audio file extensions to search for
  extensions: [
    "mp3",
    "flac",
    "m4a",
    "wav",
    "ogg",
    "aac",
    "wma",
    "opus",
    "ape",
    "alac",
  ],

  // Source folders to scan
  sourceFolders: [
    "/volume1/Externe Festplatte/Contents/",
    "/volume1/Externe Festplatte/DJing/",
    "/volume1/Externe Festplatte/Studio/Backup Mac Mini/# - Tracks/Dezember 2016",
    "/volume1/Externe Festplatte/Kleine Externe/#Backup Macbook/Music/iTunes",
    "/volume1/Externe Festplatte/Kleine Externe/#Backup Macbook/Music/DJ Database",
  ],

  // Patterns to exclude
  excludePatterns: [
    "**/sample_packs/**",
    "**/samples/**",
    "**/Sample Packs/**",
    "**/Samples/**",
    "**/loops/**",
    "**/Loops/**",
    // Add more exclude patterns here
  ],
};

// Types
interface Stats {
  moved: number;
  duplicates: number;
  skipped: number;
  errors: number;
}

interface DuplicateEntry {
  filename: string;
  originalPath: string;
  existingPath: string;
  timestamp: string;
  size: number;
  hash?: string;
}

interface DuplicatesData {
  generatedAt: string;
  totalDuplicates: number;
  duplicates: DuplicateEntry[];
}

// Utilities
const stats: Stats = {
  moved: 0,
  duplicates: 0,
  skipped: 0,
  errors: 0,
};

const duplicates: DuplicateEntry[] = [];

function log(message: string, toConsole = true): void {
  const timestamp = new Date().toISOString();
  const logMessage = `[${timestamp}] ${message}\n`;

  fs.appendFileSync(CONFIG.logFile, logMessage);

  if (toConsole) {
    console.log(message);
  }
}

function matchesExcludePattern(filePath: string): boolean {
  return CONFIG.excludePatterns.some((pattern) => {
    const regex = new RegExp(
      pattern.replace(/\*\*/g, ".*").replace(/\*/g, "[^/]*").replace(/\?/g, ".")
    );
    return regex.test(filePath);
  });
}

function isAudioFile(filename: string): boolean {
  const ext = path.extname(filename).toLowerCase().slice(1);
  return CONFIG.extensions.includes(ext);
}

async function* walkDirectory(dir: string): AsyncGenerator<string> {
  try {
    const entries = await fs.promises.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);

      if (entry.isDirectory()) {
        if (!matchesExcludePattern(fullPath)) {
          yield* walkDirectory(fullPath);
        }
      } else if (entry.isFile() && isAudioFile(entry.name)) {
        if (!matchesExcludePattern(fullPath)) {
          yield fullPath;
        }
      }
    }
  } catch (error) {
    log(`ERROR: Cannot read directory ${dir}: ${error}`, false);
    stats.errors++;
  }
}

async function getFileSize(filePath: string): Promise<number> {
  try {
    const stats = await fs.promises.stat(filePath);
    return stats.size;
  } catch {
    return 0;
  }
}

async function moveFile(sourcePath: string): Promise<void> {
  const filename = path.basename(sourcePath);
  const destPath = path.join(CONFIG.destination, filename);

  try {
    // Check if file already exists
    if (fs.existsSync(destPath)) {
      // File exists - add to duplicates JSON
      const fileSize = await getFileSize(sourcePath);

      const duplicateEntry: DuplicateEntry = {
        filename,
        originalPath: sourcePath,
        existingPath: destPath,
        timestamp: new Date().toISOString(),
        size: fileSize,
      };

      duplicates.push(duplicateEntry);
      stats.duplicates++;

      log(`DUPLICATE: ${filename}`, false);
      log(`  Original: ${sourcePath}`, false);
      log(`  Existing: ${destPath}`, false);

      // Show progress in console
      if (stats.duplicates % 5 === 0) {
        console.log(`Found ${stats.duplicates} duplicates so far...`);
      }
    } else {
      // Move the file
      await fs.promises.rename(sourcePath, destPath);
      stats.moved++;

      log(`MOVED: ${sourcePath} -> ${destPath}`, false);

      // Show progress every 10 files
      if (stats.moved % 10 === 0) {
        console.log(`Progress: ${stats.moved} files moved...`);
      }
    }
  } catch (error) {
    log(`ERROR moving ${sourcePath}: ${error}`);
    stats.errors++;
  }
}

async function saveDuplicatesJson(): Promise<void> {
  const data: DuplicatesData = {
    generatedAt: new Date().toISOString(),
    totalDuplicates: duplicates.length,
    duplicates: duplicates,
  };

  await fs.promises.writeFile(
    CONFIG.duplicatesFile,
    JSON.stringify(data, null, 2),
    "utf-8"
  );

  console.log(`\nDuplicates saved to: ${CONFIG.duplicatesFile}`);
}

async function processFolder(folderPath: string): Promise<void> {
  console.log(`\nProcessing: ${folderPath}`);
  log(`Processing directory: ${folderPath}`);
  log("----------------------------------------");

  if (!fs.existsSync(folderPath)) {
    log(`WARNING: Directory does not exist: ${folderPath}`);
    return;
  }

  let fileCount = 0;

  for await (const filePath of walkDirectory(folderPath)) {
    await moveFile(filePath);
    fileCount++;
  }

  console.log(`  Processed ${fileCount} files from this folder`);
}

async function main(): Promise<void> {
  console.log("=========================================");
  console.log("Music File Consolidation Script");
  console.log("=========================================");
  console.log("");
  console.log(`Destination: ${CONFIG.destination}`);
  console.log(`Log file: ${CONFIG.logFile}`);
  console.log(`Duplicates file: ${CONFIG.duplicatesFile}`);
  console.log("");
  console.log("Source folders:");
  CONFIG.sourceFolders.forEach((folder) => console.log(`  - ${folder}`));
  console.log("");
  console.log("Excluded patterns:");
  CONFIG.excludePatterns.forEach((pattern) => console.log(`  - ${pattern}`));
  console.log("");

  // Create destination directory
  await fs.promises.mkdir(CONFIG.destination, { recursive: true });

  // Initialize log file
  const startTime = new Date().toISOString();
  fs.writeFileSync(CONFIG.logFile, "");
  log("=========================================");
  log(`Music consolidation started at ${startTime}`);
  log(`Destination: ${CONFIG.destination}`);
  log("=========================================");
  log("");
  log("Source folders:");
  CONFIG.sourceFolders.forEach((folder) => log(`  - ${folder}`));
  log("");
  log("Excluded patterns:");
  CONFIG.excludePatterns.forEach((pattern) => log(`  - ${pattern}`));
  log("=========================================");
  log("");

  // Process each source folder
  for (const folder of CONFIG.sourceFolders) {
    await processFolder(folder);
  }

  // Save duplicates JSON
  if (duplicates.length > 0) {
    await saveDuplicatesJson();
  }

  // Final summary
  console.log("");
  console.log("=========================================");
  console.log("Consolidation Complete!");
  console.log("=========================================");
  console.log(`Files moved: ${stats.moved}`);
  console.log(`Duplicates found: ${stats.duplicates}`);
  console.log(`Files skipped (excluded): ${stats.skipped}`);
  console.log(`Errors: ${stats.errors}`);
  console.log("=========================================");
  console.log("");
  console.log(`Log file: ${CONFIG.logFile}`);
  if (duplicates.length > 0) {
    console.log(`Duplicates file: ${CONFIG.duplicatesFile}`);
  }

  log("");
  log("=========================================");
  log(`Consolidation completed at ${new Date().toISOString()}`);
  log("=========================================");
  log(`Files moved: ${stats.moved}`);
  log(`Duplicates found: ${stats.duplicates}`);
  log(`Files skipped (excluded): ${stats.skipped}`);
  log(`Errors: ${stats.errors}`);
  log("=========================================");
}

// Run the script
main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
