#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import configureCommand from "./commands/configure";
import { listCommand as pipelineListCommand, openCommand as pipelineOpenCommand, runsCommand, showCommand as pipelineShowCommand, triggerCommand, watchCommand } from "@scripts/pipeline/commands";
import { commentsCommand, copyCommand, createCommand as createPrCommand, listCommand as listPrCommand, openCommand as openPrCommand, showCommand as showPrCommand } from "@scripts/pr/commands";
import { approveCommand, changelogCommand } from "@scripts/release/commands";
import { closeCommand, createCommand as createTaskCommand, listCommand as listTaskCommand, recentCommand, showCommand as showTaskCommand, startCommand } from "@scripts/task/commands";

const cli = await createCLI({
  name: "eimer",
  version: "0.1.0",
  description: "Meta CLI for all scripts packages",
});

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
  commands: [changelogCommand, approveCommand],
} as any;

const taskCommand = {
  name: "task",
  description: "Task commands",
  commands: [createTaskCommand, listTaskCommand, recentCommand, startCommand, closeCommand, showTaskCommand],
} as any;

cli.command(prCommand);
cli.command(pipelineCommand);
cli.command(releaseCommand);
cli.command(taskCommand);
cli.command(configureCommand);

await cli.run();
