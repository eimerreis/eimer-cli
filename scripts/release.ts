// Release helpers for CI (run with bun).
//   bun scripts/release.ts check-pr <base-ref>   require a changeset on the pull request
//   bun scripts/release.ts prepare               version pending changesets, sync Cargo, guard the diff
// Follows eimerreis/devops ADR-0009 and ADR-0018.
import { $ } from "bun";
import { appendFileSync, readdirSync, readFileSync, writeFileSync } from "node:fs";

/** Release units: Changesets package name -> its package.json. */
export const UNITS = {
  "@scripts/release": "packages/release/package.json",
  eimer: "crates/eimer/package.json",
} as const;

type Unit = keyof typeof UNITS;

const EXEMPT_TITLE = /^chore\(deps(-dev)?\)/;

/** Only these paths may change in a release commit. */
const METADATA = [
  /^\.changeset\/[^/]+\.md$/,
  /^(.+\/)?CHANGELOG\.md$/,
  /^(.+\/)?package\.json$/,
  /^bun\.lock$/,
  /^crates\/eimer\/Cargo\.toml$/,
  /^Cargo\.lock$/,
];

export const isExemptTitle = (title: string) => EXEMPT_TITLE.test(title);

export const isChangesetFile = (path: string) =>
  /^\.changeset\/[^/]+\.md$/.test(path) && path !== ".changeset/README.md";

export const unexpectedPaths = (paths: string[]) => paths.filter((p) => !METADATA.some((re) => re.test(p)));

/** Sets `version` in the [package] table of a Cargo.toml. */
export function setCargoTomlVersion(toml: string, version: string): string {
  const updated = toml.replace(/(\[package\][^[]*?\nversion\s*=\s*)"[^"]*"/, `$1"${version}"`);
  if (updated === toml && !toml.includes(`version = "${version}"`)) throw new Error("No [package] version in Cargo.toml");
  return updated;
}

/** Sets the version of package `name` in Cargo.lock. */
export function setCargoLockVersion(lock: string, name: string, version: string): string {
  const pattern = new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = )"[^"]*"`);
  if (!pattern.test(lock)) throw new Error(`No ${name} entry in Cargo.lock`);
  return lock.replace(pattern, `$1"${version}"`);
}

const readVersion = (unit: Unit): string => JSON.parse(readFileSync(UNITS[unit], "utf8")).version;

export function cargoVersion(toml: string): string | undefined {
  return /\[package\][^[]*?\nversion\s*=\s*"([^"]*)"/.exec(toml)?.[1];
}

function output(values: Record<string, string>) {
  const lines = Object.entries(values).map(([k, v]) => `${k}=${v}`);
  console.log(lines.join("\n"));
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${lines.join("\n")}\n`);
}

async function checkPr(baseRef: string) {
  const title = process.env.PR_TITLE ?? "";
  if (isExemptTitle(title)) {
    console.log(`Exempt: "${title}" is a dependency update.`);
    return;
  }
  const diff = await $`git diff --name-only --diff-filter=A ${baseRef}...HEAD -- .changeset`.text();
  const added = diff.split("\n").filter(isChangesetFile);
  if (added.length > 0) {
    console.log(`Changeset: ${added.join(", ")}`);
    return;
  }
  console.error(
    "::error title=Changeset missing::Add a changeset: `bunx changeset` (or `bunx changeset --empty` when nothing ships). " +
      "Pull requests titled chore(deps) or chore(deps-dev) are exempt.",
  );
  process.exit(1);
}

async function prepare() {
  const pending = readdirSync(".changeset").filter((f) => isChangesetFile(`.changeset/${f}`));
  if (pending.length === 0) {
    console.log("No pending changesets; nothing to release.");
    output({ released: "false" });
    return;
  }

  const cargo = cargoVersion(readFileSync("crates/eimer/Cargo.toml", "utf8"));
  if (cargo !== readVersion("eimer")) {
    throw new Error(`crates/eimer/package.json (${readVersion("eimer")}) and Cargo.toml (${cargo}) disagree`);
  }

  const before = Object.fromEntries(Object.keys(UNITS).map((u) => [u, readVersion(u as Unit)]));
  await $`bunx changeset version`;
  const changed = (Object.keys(UNITS) as Unit[]).filter((u) => readVersion(u) !== before[u]);

  if (changed.includes("eimer")) {
    const version = readVersion("eimer");
    const toml = "crates/eimer/Cargo.toml";
    writeFileSync(toml, setCargoTomlVersion(readFileSync(toml, "utf8"), version));
    writeFileSync("Cargo.lock", setCargoLockVersion(readFileSync("Cargo.lock", "utf8"), "eimer", version));
  }
  await $`bun install --lockfile-only --ignore-scripts`.quiet();

  const status = await $`git status --porcelain`.text();
  const paths = status
    .split("\n")
    .filter(Boolean)
    .map((line) => line.slice(3).replace(/^"|"$/g, ""));
  const unexpected = unexpectedPaths(paths);
  if (unexpected.length > 0) throw new Error(`Release would commit non-metadata files: ${unexpected.join(", ")}`);

  const values: Record<string, string> = { released: String(paths.length > 0) };
  for (const unit of changed) values[unit === "eimer" ? "eimer_version" : "release_version"] = readVersion(unit);
  output(values);
}

if (import.meta.main) {
  const [command, arg] = process.argv.slice(2);
  if (command === "check-pr" && arg) await checkPr(arg);
  else if (command === "prepare") await prepare();
  else {
    console.error("Usage: bun scripts/release.ts check-pr <base-ref> | prepare");
    process.exit(2);
  }
}

