#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import completionsCommand from "./commands/completions";
import configureCommand from "./commands/configure";
import doctorCommand from "./commands/doctor";
import updateCommand from "./commands/update";
import { completeWords, type CompletionNode } from "./lib/complete";
import { version } from "./lib/version";
import { listCommand as pipelineListCommand, openCommand as pipelineOpenCommand, runsCommand, showCommand as pipelineShowCommand, triggerCommand, watchCommand } from "@scripts/pipeline/commands";
import { commentsCommand, copyCommand, createCommand as createPrCommand, listCommand as listPrCommand, openCommand as openPrCommand, showCommand as showPrCommand } from "@scripts/pr/commands";
import { approveCommand, changelogCommand, configureCommand as releaseConfigureCommand } from "@scripts/release/commands";
import { closeCommand, createCommand as createTaskCommand, listCommand as listTaskCommand, recentCommand, showCommand as showTaskCommand, startCommand } from "@scripts/task/commands";

const prCommand = {
  name: "pr",
  description: "Pull request commands",
  commands: [createPrCommand, commentsCommand, copyCommand, listPrCommand, openPrCommand, showPrCommand],
} as any;

const pipelineCommand = {
  name: "pipeline",
  description: "Pipeline commands",
  commands: [runsCommand, pipelineListCommand, pipelineOpenCommand, pipelineShowCommand, triggerCommand, watchCommand],
} as any;

const releaseCommand = {
  name: "release",
  description: "Release commands",
  commands: [changelogCommand, approveCommand, releaseConfigureCommand],
} as any;

const taskCommand = {
  name: "task",
  description: "Task commands",
  commands: [createTaskCommand, listTaskCommand, recentCommand, startCommand, closeCommand, showTaskCommand],
} as any;

const topLevelCommands = [prCommand, pipelineCommand, releaseCommand, taskCommand, configureCommand, doctorCommand, updateCommand, completionsCommand];

// Shell completion callback used by `eimer completions <shell>`; handled before Bunli parses args.
const [firstArg, ...restArgs] = Bun.argv.slice(2);
if (firstArg === "__complete") {
  const root: CompletionNode = { name: "eimer", commands: topLevelCommands as CompletionNode[] };
  console.log(completeWords(root, restArgs).join("\n"));
  process.exit(0);
}

const cli = await createCLI({
  name: "eimer",
  version,
  description: "Personal dev CLI for Azure DevOps & GitHub workflows (run `eimer doctor` to check your setup)",
});

for (const command of topLevelCommands) {
  cli.command(command);
}

await cli.run();
