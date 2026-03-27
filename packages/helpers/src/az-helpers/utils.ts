import { execa } from "execa";

export const GetTaskStateEmoji = (state: string) => {
  const stateEmoji =
    state === "Active"
      ? "🔵"
      : state === "Resolved"
      ? "🟠"
      : state === "Not Started"
      ? "⚪"
      : "🟢";

  return stateEmoji;
};

export const TaskFields = {
  CompletedHours: "Microsoft.VSTS.Scheduling.CompletedWork",
  RemainingHours: "Microsoft.VSTS.Scheduling.RemainingWork",
  State: "System.State",
  Title: "System.Title",
};

export const Areas = {
  Default: process.env.AZURE_DEVOPS_AREA_PATH || "Company\\Engineering",
};

export const DefaultTeam = process.env.AZURE_DEVOPS_TEAM || "Default Team";

const parseAzureConfigValue = (configText: string, key: string): string => {
  return (configText.match(new RegExp(`^${key}\\s*=\\s*(.+)$`, "im"))?.[1] || "").trim();
};

export const getAzureProjectBaseUrl = async (): Promise<string> => {
  const { stdout } = await execa("az", ["devops", "configure", "--list"]);
  const organization = parseAzureConfigValue(stdout, "organization").replace(/\/$/, "");
  const project = parseAzureConfigValue(stdout, "project");

  if (!organization || !project) {
    throw new Error(
      "Azure DevOps defaults missing. Configure with 'az devops configure -d organization=<url> project=<name>'.",
    );
  }

  return `${organization}/${project}`;
};

export const getGitUserEmail = async (): Promise<string> => {
  try {
    const { stdout } = await execa("git", ["config", "user.email"]);
    return stdout.trim();
  } catch {
    return "";
  }
};

export const getDefaultReviewer = async (): Promise<string> => {
  return (process.env.AZURE_DEVOPS_REVIEWER || (await getGitUserEmail()) || "").trim();
};

export const getRequestedForIdentity = async (): Promise<string> => {
  return (process.env.AZURE_DEVOPS_REQUESTED_FOR || (await getGitUserEmail()) || "").trim();
};

export const requireConfiguredTeam = (): string => {
  if (!process.env.AZURE_DEVOPS_TEAM) {
    throw new Error("Set AZURE_DEVOPS_TEAM before using this helper.");
  }

  return process.env.AZURE_DEVOPS_TEAM;
};

export const requireConfiguredAreaPath = (): string => {
  if (!process.env.AZURE_DEVOPS_AREA_PATH) {
    throw new Error("Set AZURE_DEVOPS_AREA_PATH before using this helper.");
  }

  return process.env.AZURE_DEVOPS_AREA_PATH;
};

export const buildPullRequestUrl = (baseUrl: string, repoName: string, pullRequestId: number): string => {
  return `${baseUrl}/_git/${repoName}/pullrequest/${pullRequestId}`;
};

export const buildWorkItemUrl = (baseUrl: string, workItemId: number): string => {
  return `${baseUrl}/_workitems/edit/${workItemId}`;
};

export const buildBuildUrl = (baseUrl: string, buildId: number): string => {
  return `${baseUrl}/_build/results?buildId=${buildId}`;
};
