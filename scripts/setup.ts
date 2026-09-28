#!/usr/bin/env bun
/**
 * One-command setup: install deps, build eimer, link it onto PATH, run `eimer doctor`.
 *
 *   bun run setup                 # build eimer, link into ~/.local/bin
 *   bun run setup --all           # also build pr/pipeline/release/task binaries
 *   bun run setup --bin-dir DIR   # link somewhere else
 *   bun run setup --no-doctor     # skip the final health check
 */
import { existsSync, lstatSync, mkdirSync, readlinkSync, symlinkSync, unlinkSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join, resolve } from "node:path";

const repoDir = resolve(import.meta.dir, "..");
const args = Bun.argv.slice(2);
const buildAll = args.includes("--all");
const skipDoctor = args.includes("--no-doctor");
const binDirIndex = args.indexOf("--bin-dir");
const binDir = resolve(binDirIndex >= 0 && args[binDirIndex + 1] ? args[binDirIndex + 1]! : process.env.EIMER_BIN_DIR || join(homedir(), ".local", "bin"));
const binaries = buildAll ? ["eimer", "pr", "pipeline", "release", "task"] : ["eimer"];

const bold = (value: string) => `\u001b[1m${value}\u001b[22m`;
const dim = (value: string) => `\u001b[2m${value}\u001b[22m`;
const tilde = (value: string) => value.replace(homedir(), "~");

async function run(command: string[]): Promise<void> {
  console.log(dim(`$ ${command.join(" ")}`));
  const exitCode = await Bun.spawn({ cmd: command, cwd: repoDir, stdout: "inherit", stderr: "inherit" }).exited;
  if (exitCode !== 0) {
    console.error(`\n'${command.join(" ")}' failed with exit code ${exitCode}.`);
    process.exit(exitCode);
  }
}

function link(name: string): void {
  const source = join(repoDir, "bin", name);
  const target = join(binDir, name);

  if (existsSync(target) || isSymlink(target)) {
    if (!isSymlink(target)) {
      console.log(`! ${tilde(target)} exists and is not a symlink; leaving it alone.`);
      return;
    }

    if (readlinkSync(target) === source) {
      console.log(`+ ${tilde(target)} already links to ${tilde(source)}`);
      return;
    }

    unlinkSync(target);
  }

  symlinkSync(source, target);
  console.log(`+ linked ${tilde(target)} -> ${tilde(source)}`);
}

function isSymlink(path: string): boolean {
  try {
    return lstatSync(path).isSymbolicLink();
  } catch {
    return false;
  }
}

function shellRcFile(): string {
  const shell = process.env.SHELL || "";
  if (shell.endsWith("zsh")) return "~/.zshrc";
  if (shell.endsWith("fish")) return "~/.config/fish/config.fish";
  return "~/.bashrc";
}

console.log(bold("1/4 Installing dependencies"));
await run(["bun", "install"]);

console.log(bold(`\n2/4 Building ${binaries.join(", ")}`));
await run(["bun", "run", buildAll ? "build:all" : "build"]);

console.log(bold(`\n3/4 Linking into ${tilde(binDir)}`));
mkdirSync(binDir, { recursive: true });
for (const name of binaries) {
  link(name);
}

const pathDirs = (process.env.PATH || "").split(delimiter).map((dir) => resolve(dir));
if (!pathDirs.includes(binDir)) {
  const line = shellRcFile().includes("fish") ? `fish_add_path ${tilde(binDir)}` : `export PATH="${tilde(binDir).replace("~", "$HOME")}:$PATH"`;
  console.log(`\n! ${tilde(binDir)} is not on your PATH. Add this to ${shellRcFile()} and open a new shell:\n    ${line}`);
}

const shellName = (process.env.SHELL || "zsh").split("/").pop() || "zsh";
const completionShell = ["zsh", "bash", "fish"].includes(shellName) ? shellName : "zsh";
console.log(
  dim(
    completionShell === "fish"
      ? `\nTab completion: eimer completions fish > ~/.config/fish/completions/eimer.fish`
      : `\nTab completion: add 'source <(eimer completions ${completionShell})' to ${shellRcFile()}`,
  ),
);

if (!skipDoctor) {
  console.log(bold("\n4/4 Checking your setup (eimer doctor)"));
  const exitCode = await Bun.spawn({ cmd: [join(repoDir, "bin", "eimer"), "doctor"], cwd: repoDir, stdout: "inherit", stderr: "inherit" }).exited;
  if (exitCode !== 0) {
    console.log("\nSetup finished, but doctor found problems; follow the hints above.");
    process.exit(0);
  }
}

console.log(`\n${bold("Done.")} Try: eimer --help`);
