#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import approveCommand from "./commands/approve";
import changelogCommand from "./commands/changelog";

const cli = await createCLI({
  name: "release",
  version: "0.1.0",
  description: "Release CLI",
});

cli.command(changelogCommand);
cli.command(approveCommand);

await cli.run();
