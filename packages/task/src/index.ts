#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import closeCommand from "./commands/close";
import createCommand from "./commands/create";
import listCommand from "./commands/list";
import recentCommand from "./commands/recent";
import showCommand from "./commands/show";
import startCommand from "./commands/start";

const cli = await createCLI({
  name: "task",
  version: "0.1.0",
  description: "Task CLI",
});

cli.command(createCommand);
cli.command(listCommand);
cli.command(recentCommand);
cli.command(startCommand);
cli.command(closeCommand);
cli.command(showCommand);

await cli.run();
