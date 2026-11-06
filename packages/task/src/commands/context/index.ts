import { defineGroup } from "@bunli/core";
import listCommand from "./list";
import showCommand from "./show";
import useCommand from "./use";

const contextCommand = defineGroup({
  name: "context",
  description: "Manage the Azure DevOps context (org/project) task commands act in",
  commands: [showCommand, listCommand, useCommand],
});

export default contextCommand;
