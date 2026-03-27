import * as fs from "fs";
import * as path from "path";
import * as glob from "glob";
import { getTranslationSearchPaths } from "./config";

const findTranslationFiles = (): string[] => {
  const files: string[] = [];

  for (const searchPath of config.searchPaths) {
    const pattern = path.join(searchPath, "**/*.json");
    const matches = glob.sync(pattern, { absolute: true });
    files.push(...matches);
  }

  return files;
};

const config = {
  searchPaths: getTranslationSearchPaths(),
  amountOfTotalLocales: 14,
  minOccurrences: 2,
  ignoreKeys: [],
  ignorePatterns: [/^shared.json/, /^links.json/],
};

const WordReducer = (acc: string[], val: unknown): string[] => {
  if (!val) return acc;

  if (typeof val === "string") {
    acc.push(val);
  }

  if (typeof val === "object") {
    return Object.values(val).reduce<string[]>(WordReducer, acc);
  }

  return acc;
};

const countCharacters = () => {
  const translationFiles = findTranslationFiles();

  const translationValues = translationFiles.reduce<string[]>(
    (acc, filePath) => {
      const content = fs.readFileSync(filePath, "utf-8");
      const translationsObj = JSON.parse(content);

      return acc.concat(
        Object.values(translationsObj).reduce<string[]>(WordReducer, []),
      );
    },
    [],
  );

  const characterCount = translationValues.reduce(
    (acc, val) => acc + val.length,
    0,
  );
  console.log("🔢 characterCount:", characterCount);
};

countCharacters();
