import * as path from "path";

const EXAMPLE_TRANSLATION_PATHS = [
  "~/Sources/example-app/translations",
  "~/Sources/example-admin/translations",
];

function expandHomeDir(inputPath: string): string {
  if (inputPath === "~") {
    return process.env.HOME || inputPath;
  }

  if (inputPath.startsWith("~/") && process.env.HOME) {
    return path.join(process.env.HOME, inputPath.slice(2));
  }

  return inputPath;
}

function getTranslationSearchPaths(): string[] {
  const raw = (process.env.EIMER_TRANSLATION_PATHS || "").trim();
  if (!raw) {
    throw new Error(
      `Set EIMER_TRANSLATION_PATHS to a ${path.delimiter}-separated list of translation directories. Example: ${EXAMPLE_TRANSLATION_PATHS.join(path.delimiter)}`,
    );
  }

  const paths = raw
    .split(path.delimiter)
    .map((entry) => expandHomeDir(entry.trim()))
    .filter((entry) => entry.length > 0);

  if (paths.length === 0) {
    throw new Error("EIMER_TRANSLATION_PATHS did not contain any usable paths.");
  }

  return paths;
}

export { EXAMPLE_TRANSLATION_PATHS, getTranslationSearchPaths };
