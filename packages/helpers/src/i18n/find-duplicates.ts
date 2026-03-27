// find-duplicates.ts
import * as fs from "fs";
import * as path from "path";
import * as glob from "glob";
import { getTranslationSearchPaths } from "./config";

interface TranslationEntry {
  key: string;
  value: string;
  filePath: string;
  locale: string;
}

interface DuplicateGroup {
  translation: string;
  locale: string;
  occurrences: Array<{
    key: string;
    filePath: string;
    namespace: string;
  }>;
  suggestedSharedKey: string;
  wordCount: number; // Number of words in the translation
  phraseWordsSaved: number; // How many phrase words saved if consolidated
}

interface Config {
  amountOfTotalLocales: number;
  searchPaths: string[];
  locales: string[];
  outputFile: string;
  minOccurrences: number;
  ignoreKeys?: string[];
  ignorePatterns?: RegExp[];
}

interface Report {
  generatedAt: string;
  summary: {
    totalDuplicateGroups: number;
    byLocale: Record<
      string,
      { groups: number; phraseWordsSaved: number; actualWordsSaved: number }
    >;
    potentialPhraseSavings: number; // Total phrase entries saved
    potentialWordSavings: number; // Total actual words saved (based on English)
    totalWordSavingsAcrossAllLocales: number; // Word savings × total locales
  };
  duplicates: DuplicateGroup[];
}

/**
 * Count words in a translation string
 */
const countWords = (text: string): number => {
  // Remove extra whitespace and split by spaces
  // This is a simple word count - you can make it more sophisticated if needed
  return text
    .trim()
    .split(/\s+/)
    .filter((word) => word.length > 0).length;
};

/**
 * Find all translation files matching the pattern
 */
const findTranslationFiles = (config: Config): string[] => {
  const files: string[] = [];

  for (const searchPath of config.searchPaths) {
    const pattern = path.join(searchPath, "**/*.json");
    const matches = glob.sync(pattern, { absolute: true });
    files.push(...matches);
  }

  return files;
};

/**
 * Extract locale from file path
 */
const extractLocale = (filePath: string, locales: string[]): string | null => {
  // locale is the folder name
  const fileName = path.basename(filePath, path.extname(filePath));
  const dirname = path.basename(path.dirname(filePath));
  return locales.includes(fileName)
    ? fileName
    : locales.includes(dirname)
      ? dirname
      : null;
};

/**
 * Check if key should be ignored
 */
const shouldIgnoreKey = (
  key: string,
  ignoreKeys?: string[],
  ignorePatterns?: RegExp[],
): boolean => {
  if (ignoreKeys?.includes(key)) {
    return true;
  }

  if (ignorePatterns) {
    return ignorePatterns.some((pattern) => pattern.test(key));
  }

  return false;
};

/**
 * Recursively index translations with their full key paths
 */
const indexTranslations = (
  obj: any,
  filePath: string,
  locale: string,
  translations: Map<string, TranslationEntry[]>,
  config: Config,
  prefix: string = "",
): void => {
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;

    // Skip if key matches ignore patterns
    if (shouldIgnoreKey(fullKey, config.ignoreKeys, config.ignorePatterns)) {
      continue;
    }

    if (typeof value === "string") {
      // Skip empty strings and already referenced translations
      if (!value.trim() || value.startsWith("$t(")) {
        continue;
      }

      const mapKey = `${locale}:${value}`;

      if (!translations.has(mapKey)) {
        translations.set(mapKey, []);
      }

      translations.get(mapKey)!.push({
        key: fullKey,
        value,
        filePath,
        locale,
      });
    } else if (typeof value === "object" && value !== null) {
      // Recursively process nested objects
      indexTranslations(
        obj[key],
        filePath,
        locale,
        translations,
        config,
        fullKey,
      );
    }
  }
};

/**
 * Load all translations from files
 */
const loadTranslations = (
  files: string[],
  config: Config,
): Map<string, TranslationEntry[]> => {
  const translations = new Map<string, TranslationEntry[]>();

  for (const filePath of files) {
    const locale = extractLocale(filePath, config.locales);
    if (!locale) continue;

    try {
      const content = fs.readFileSync(filePath, "utf-8");
      const translationsObj = JSON.parse(content);

      indexTranslations(
        translationsObj,
        filePath,
        locale,
        translations,
        config,
      );
      console.log(`✓ Loaded: ${filePath}`);
    } catch (error) {
      console.error(`✗ Error loading ${filePath}:`, error);
    }
  }

  return translations;
};

/**
 * Extract namespace from file path
 */
const extractNamespace = (filePath: string, locales: string[]): string => {
  const parts = filePath.split(path.sep);
  const localeFileIndex = parts.findIndex((part) =>
    locales.some((locale) => part === `${locale}.json`),
  );

  if (localeFileIndex > 0) {
    return parts[localeFileIndex - 1];
  }

  return "unknown";
};

/**
 * Generate a suggested shared key name
 */
const generateSharedKey = (
  originalKey: string,
  translation: string,
): string => {
  console.log("🚀 ~ generateSharedKey ~ originalKey:", originalKey)
  // Remove common prefixes and clean up
  const cleanKey =
    originalKey
      .replace(/^(common|shared|global)\./, "")
      .split(".")
      .pop() || originalKey;

  // Create a camelCase version
  const words = translation
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, "")
    .split(/\s+/)
    .filter((w) => w.length > 0)
    .slice(0, 3); // Take first 3 words max

  if (words.length > 0) {
    return words
      .map((word, index) =>
        index === 0 ? word : word.charAt(0).toUpperCase() + word.slice(1),
      )
      .join("");
  }

  return cleanKey;
};

/**
 * Find all duplicate translations
 */
const findDuplicates = (
  translations: Map<string, TranslationEntry[]>,
  config: Config,
): DuplicateGroup[] => {
  const duplicates: DuplicateGroup[] = [];

  for (const [mapKey, entries] of translations.entries()) {
    // Only consider entries that appear multiple times
    if (entries.length < config.minOccurrences) {
      continue;
    }

    const [locale, translation] = mapKey.split(":", 2);
    const wordCount = countWords(translation);

    // If a translation appears N times, consolidating saves (N-1) phrase entries
    const phraseWordsSaved = entries.length - 1;

    duplicates.push({
      translation,
      locale,
      occurrences: entries.map((entry) => ({
        key: entry.key,
        filePath: entry.filePath,
        namespace: extractNamespace(entry.filePath, config.locales),
      })),
      suggestedSharedKey: generateSharedKey(entries[0].key, translation),
      wordCount,
      phraseWordsSaved,
    });
  }

  // Sort by phrase words saved (descending) - most impactful first
  return duplicates.sort((a, b) => b.phraseWordsSaved - a.phraseWordsSaved);
};

/**
 * Get summary by locale
 */
const getSummaryByLocale = (
  duplicates: DuplicateGroup[],
): Record<
  string,
  { groups: number; phraseWordsSaved: number; actualWordsSaved: number }
> => {
  return duplicates.reduce(
    (summary, dup) => {
      if (!summary[dup.locale]) {
        summary[dup.locale] = {
          groups: 0,
          phraseWordsSaved: 0,
          actualWordsSaved: 0,
        };
      }
      summary[dup.locale].groups += 1;
      summary[dup.locale].phraseWordsSaved += dup.phraseWordsSaved;
      // Actual words saved = word count × number of duplicates removed
      summary[dup.locale].actualWordsSaved +=
        dup.wordCount * dup.phraseWordsSaved;
      return summary;
    },
    {} as Record<
      string,
      { groups: number; phraseWordsSaved: number; actualWordsSaved: number }
    >,
  );
};

/**
 * Calculate potential savings
 */
const calculateSavings = (
  duplicates: DuplicateGroup[],
  config: Config,
): {
  potentialPhraseSavings: number;
  potentialWordSavings: number;
  totalWordSavingsAcrossAllLocales: number;
} => {
  // Get English duplicates as baseline for word count
  const englishDuplicates = duplicates.filter((dup) => dup.locale === "en");

  // Calculate phrase savings (number of duplicate entries)
  const potentialPhraseSavings = duplicates.reduce(
    (sum, dup) => sum + dup.phraseWordsSaved,
    0,
  );

  // Calculate word savings based on English
  // For each English duplicate, count: word_count × times_duplicated
  const potentialWordSavings = englishDuplicates.reduce(
    (sum, dup) => sum + dup.wordCount * dup.phraseWordsSaved,
    0,
  );

  // Total word savings across all locales
  // This is the English word savings × total number of locales
  const totalWordSavingsAcrossAllLocales =
    potentialWordSavings * config.amountOfTotalLocales;

  return {
    potentialPhraseSavings,
    potentialWordSavings,
    totalWordSavingsAcrossAllLocales,
  };
};

/**
 * Generate markdown report
 */
const generateMarkdownReport = (report: Report, config: Config): string => {
  let md = "# Translation Duplicates Report\n\n";
  md += `**Generated:** ${new Date(report.generatedAt).toLocaleString()}\n\n`;

  md += "## 💰 Savings Summary\n\n";
  md += `### Overall Impact\n\n`;
  md += `- **Total Duplicate Groups Found:** ${report.summary.totalDuplicateGroups}\n`;
  md += `- **Phrase Entries Saved:** ${report.summary.potentialPhraseSavings}\n`;
  md += `- **Words Saved (English baseline):** ${report.summary.potentialWordSavings}\n`;
  md += `- **Total Words Saved Across All ${config.amountOfTotalLocales} Locales:** ${report.summary.totalWordSavingsAcrossAllLocales}\n\n`;

  md += `### Cost Calculation Example\n\n`;
  md += `If translation costs €0.10 per word:\n`;
  md += `- Savings per consolidation: €${(report.summary.potentialWordSavings * 0.1).toFixed(2)}\n`;
  md += `- Total savings across all locales: €${(report.summary.totalWordSavingsAcrossAllLocales * 0.1).toFixed(2)}\n\n`;

  md += "### By Locale\n\n";
  md += `| Locale | Duplicate Groups | Phrase Entries Saved | Words Saved |\n`;
  md += `|--------|------------------|----------------------|-------------|\n`;

  for (const [locale, stats] of Object.entries(report.summary.byLocale)) {
    md += `| ${locale} | ${stats.groups} | ${stats.phraseWordsSaved} | ${stats.actualWordsSaved} |\n`;
  }

  md += "\n## 🎯 Top Duplicates (by impact)\n\n";

  for (const dup of report.duplicates.slice(0, 50)) {
    md += `### [${dup.locale}] "${dup.translation}"\n\n`;
    md += `- **Used:** ${dup.occurrences.length} times\n`;
    md += `- **Word Count:** ${dup.wordCount} words\n`;
    md += `- **Phrase Entries Saved:** ${dup.phraseWordsSaved}\n`;
    md += `- **Words Saved:** ${dup.wordCount * dup.phraseWordsSaved}\n`;
    md += `- **Suggested Shared Key:** \`${dup.suggestedSharedKey}\`\n\n`;
    md += "**Found in:**\n\n";

    for (const occ of dup.occurrences) {
      md += `- \`${occ.key}\` in \`${occ.filePath}\`\n`;
    }

    md += "\n---\n\n";
  }

  return md;
};

/**
 * Generate CSV report
 */
const generateCsvReport = (duplicates: DuplicateGroup[]): string => {
  const header =
    "Locale,Translation,Times Used,Word Count,Phrase Entries Saved,Words Saved,Suggested Key,Original Key,File Path,Namespace\n";

  const rows = duplicates.flatMap((dup) =>
    dup.occurrences.map(
      (occ) =>
        `"${dup.locale}","${dup.translation.replace(/"/g, '""')}",${dup.occurrences.length},${dup.wordCount},${dup.phraseWordsSaved},${dup.wordCount * dup.phraseWordsSaved},"${dup.suggestedSharedKey}","${occ.key}","${occ.filePath}","${occ.namespace}"`,
    ),
  );

  return header + rows.join("\n");
};

/**
 * Generate report files
 */
const generateReport = (duplicates: DuplicateGroup[], config: Config): void => {
  const savings = calculateSavings(duplicates, config);

  // Generate JSON report
  const jsonReport: Report = {
    generatedAt: new Date().toISOString(),
    summary: {
      totalDuplicateGroups: duplicates.length,
      byLocale: getSummaryByLocale(duplicates),
      potentialPhraseSavings: savings.potentialPhraseSavings,
      potentialWordSavings: savings.potentialWordSavings,
      totalWordSavingsAcrossAllLocales:
        savings.totalWordSavingsAcrossAllLocales,
    },
    duplicates,
  };

  fs.writeFileSync(
    config.outputFile,
    JSON.stringify(jsonReport, null, 2),
    "utf-8",
  );

  // Generate human-readable markdown report
  const mdReport = generateMarkdownReport(jsonReport, config);
  const mdFile = config.outputFile.replace(".json", ".md");
  fs.writeFileSync(mdFile, mdReport, "utf-8");

  console.log(`\n${"=".repeat(70)}`);
  console.log("📈 CONSOLIDATION SAVINGS SUMMARY");
  console.log("=".repeat(70));
  console.log(`\n📊 Duplicate Analysis:`);
  console.log(`   Total duplicate groups found: ${duplicates.length}`);
  console.log(
    `   Phrase entries that can be consolidated: ${savings.potentialPhraseSavings}`,
  );
  console.log(
    `Word savings across all languages: ${savings.potentialWordSavings * config.amountOfTotalLocales}`,
  );

  console.log(`\n📋 By Locale:`);
  for (const [locale, stats] of Object.entries(jsonReport.summary.byLocale)) {
    console.log(
      `   ${locale}: ${stats.groups} groups, ${stats.phraseWordsSaved} phrases, ${stats.actualWordsSaved} words`,
    );
  }

  console.log(`\n${"=".repeat(70)}`);
};

/**
 * Main execution function
 */
const run = async (config: Config): Promise<void> => {
  console.log("🔍 Starting duplicate translation search...\n");

  // Find all translation files
  const translationFiles = findTranslationFiles(config);
  console.log(`📁 Found ${translationFiles.length} translation files\n`);

  // Load and index all translations
  const translations = loadTranslations(translationFiles, config);

  // Find duplicates for each locale
  const duplicates = findDuplicates(translations, config);

  // Generate report
  generateReport(duplicates, config);

  console.log("\n✅ Analysis complete!");
  console.log(`📊 Report saved to: ${config.outputFile}`);
};

const config: Config = {
  searchPaths: getTranslationSearchPaths(),
  locales: ["en", "de"],
  amountOfTotalLocales: 14,
  outputFile: "./translation-duplicates.json",
  minOccurrences: 2,
  ignoreKeys: [],
  ignorePatterns: [/^shared.json/, /^links.json/],
};

// Run the script
run(config).catch(console.error);
