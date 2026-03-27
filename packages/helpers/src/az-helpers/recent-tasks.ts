#!/usr/bin/env tsx

import { $, execa } from "execa";
import { WorkItemQueryResponse } from "./recent-tasks.types";
import { command, boolean, run, flag } from "cmd-ts";
import { TerminalLink } from "../util";
import { buildWorkItemUrl, GetTaskStateEmoji, getAzureProjectBaseUrl } from "./utils";

const cmd = command({
  name: "tasks",
  description: "List currently active tasks",
  version: "0.0.1",
  args: {
    all: flag({
      type: boolean,
      description: "Show also closed & not started tasks",
      long: "all",
      short: "a",
    }),
  },
  handler: async ({ all }) => {
    const baseUrl = await getAzureProjectBaseUrl();
    const wiql = `SELECT [System.Id], [System.Title], [System.State], [System.AssignedTo] FROM WorkItems WHERE [System.AssignedTo] = @Me ORDER BY [System.Id] DESC`;
    const tasksString = await $`az boards query --wiql ${wiql}`;

    const tasks: WorkItemQueryResponse[] = JSON.parse(tasksString.stdout);
    const tasksToShow = all
      ? tasks
      : tasks.filter((task) => task.fields["System.State"] === "Active");

    const topTasks = tasksToShow.slice(0, 10);

    for (const task of topTasks) {
      const state = task.fields["System.State"];
      const stateEmoji = GetTaskStateEmoji(state);
      const logString = `${stateEmoji} ${task.fields["System.Id"]}: ${task.fields["System.Title"]}`;
      const url = buildWorkItemUrl(baseUrl, task.id);

      console.log(TerminalLink(logString, url));
      console.log("");
    }

    if (topTasks.length === 1) {
      await execa("pbcopy", {
        input: String(topTasks[0].fields["System.Id"]),
      });
    }
  },
});

run(cmd, process.argv.slice(2));
