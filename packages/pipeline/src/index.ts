#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import listCommand from "./commands/list";
import openCommand from "./commands/open";
import runsCommand from "./commands/runs";
import showCommand from "./commands/show";
import triggerCommand from "./commands/trigger";
import watchCommand from "./commands/watch";

const cli = await createCLI({
  name: "pipeline",
  version: "0.1.0",
  description: "Pipeline CLI",
});

cli.command(runsCommand);
cli.command(listCommand);
cli.command(openCommand);
cli.command(showCommand);
cli.command(triggerCommand);
cli.command(watchCommand);

await cli.run();
