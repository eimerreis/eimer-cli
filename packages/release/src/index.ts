#!/usr/bin/env bun
import { createCLI } from "@bunli/core";
import pkg from "../package.json" with { type: "json" };
import approveCommand from "./commands/approve";
import changelogCommand from "./commands/changelog";
import configureCommand from "./commands/configure";

const cli = await createCLI({
  name: "release",
  version: pkg.version,
  description: "Release CLI",
});

cli.command(changelogCommand);
cli.command(approveCommand);
cli.command(configureCommand);

await cli.run();
