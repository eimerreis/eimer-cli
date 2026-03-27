#!/usr/bin/env tsx

import { $, execa } from "execa";
import { WorkItemQueryResponse } from "./recent-tasks.types";
import {
  command,
  boolean,
  run,
  flag,
  option,
  number,
  positional,
} from "cmd-ts";
import { TerminalLink } from "../util";
import { buildWorkItemUrl, GetTaskStateEmoji, getAzureProjectBaseUrl, TaskFields } from "./utils";

const cmd = command({
  name: "update task",
  description: "updates a given task",
  version: "0.0.1",
  args: {
    taskNumber: positional({
      type: number,
      description: "Number of the task to update",
      displayName: "Task number",
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
    close: flag({
      type: boolean,
      description: "closes the task",
      long: "close",
      short: "x",
    }),
  },
  handler: async ({ completed, remaining, close, taskNumber }) => {
    const baseUrl = await getAzureProjectBaseUrl();
    const closedState = "Closed";

    const task = JSON.parse(
      (
        await $`az boards work-item update --id ${taskNumber} --fields ${
          TaskFields.CompletedHours
        }=${completed} ${
          TaskFields.RemainingHours
        }=${remaining} ${TaskFields.State}=${close ? closedState : "Active"}`
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
