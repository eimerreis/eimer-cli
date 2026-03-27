#!/usr/bin/env tsx

import { $ } from "execa";
import { WorkItemQueryResponse } from "./recent-tasks.types";
import {
  command,
  boolean,
  run,
  flag,
  option,
  number,
  positional,
  string,
} from "cmd-ts";
import { TerminalLink } from "../util";
import {
  buildWorkItemUrl,
  GetTaskStateEmoji,
  getAzureProjectBaseUrl,
  requireConfiguredAreaPath,
  requireConfiguredTeam,
  TaskFields,
} from "./utils";
import { IterationResponse } from "./iteration.types";

const cmd = command({
  name: "update task",
  description: "updates a given task",
  version: "0.0.1",
  args: {
    title: positional({
      type: string,
      description: "title of the task",
    }),
    completed: option({
      short: "c",
      long: "completed",
      type: number,
      description: "amount of completed hours",
    }),
    remaining: option({
      short: "r",
      long: "remaining",
      type: number,
      description: "amount of remaining hours",
    }),
  },
  handler: async ({ completed, remaining, title }) => {
    const baseUrl = await getAzureProjectBaseUrl();
    const team = requireConfiguredTeam();
    const areaPath = requireConfiguredAreaPath();
    const iterations = JSON.parse(
      (await $`az boards iteration team list --team ${team}`).stdout
    ) as IterationResponse[];
    const currentIteration = iterations.find(
      (x) => x.attributes.timeFrame.toLowerCase() === "current"
    );

    if (!currentIteration) {
      console.error("No current iteration found");
      process.exit(1);
    }

    const task = JSON.parse(
      (
        await $`az boards work-item create --title ${title} --type Task --iteration ${currentIteration.path} --area ${areaPath} --fields --fields ${TaskFields.CompletedHours}=${completed} ${TaskFields.RemainingHours}=${remaining}`
      ).stdout
    ) as WorkItemQueryResponse;

    const state = task.fields["System.State"];
    const stateEmoji = GetTaskStateEmoji(state);
    const logString = `${stateEmoji} ${task.id}: ${task.fields["System.Title"]}`;
    const url = buildWorkItemUrl(baseUrl, task.id);

    console.log(TerminalLink(logString, url));
    console.log("");
  },
});

run(cmd, process.argv.slice(2));
