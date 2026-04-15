#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import approveCommand from "./commands/approve";
import changelogCommand from "./commands/changelog";
import configureCommand from "./commands/configure";

const cli = await createCLI({
  name: "release",
  version: "0.1.0",
  description: "Release CLI",
});

cli.command(changelogCommand);
cli.command(approveCommand);
cli.command(configureCommand);

await cli.run();
