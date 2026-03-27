#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import commentsCommand from "./commands/comments";
import copyCommand from "./commands/copy";
import createCommand from "./commands/create";
import listCommand from "./commands/list";
import openCommand from "./commands/open";
import showCommand from "./commands/show";

const cli = await createCLI({
  name: "pr",

  version: "0.1.0",
  description: "A CLI built with Bunli",
});

cli.command(createCommand);
cli.command(commentsCommand);
cli.command(copyCommand);
cli.command(listCommand);
cli.command(openCommand);
cli.command(showCommand);

await cli.run();
