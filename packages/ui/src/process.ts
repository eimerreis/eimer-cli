type ToolInfo = {
  name: string;
  purpose: string;
  install: string;
};

const knownTools: Record<string, ToolInfo> = {
  az: {
    name: "Azure CLI",
    purpose: "Azure DevOps PRs, pipelines, releases and tasks",
    install: "brew install azure-cli && az extension add --name azure-devops && az login",
  },
  gh: {
    name: "GitHub CLI",
    purpose: "GitHub pull requests",
    install: "brew install gh && gh auth login",
  },
  git: {
    name: "git",
    purpose: "branch and remote detection",
    install: "xcode-select --install (macOS) or your package manager",
  },
};

function missingToolMessage(command: string): string {
  const tool = knownTools[command];
  const install = tool ? ` Install with: ${tool.install}.` : "";
  return `'${command}' was not found on your PATH.${install} Run 'eimer doctor' to check your setup.`;
}

/** Throws a helpful error when `command` is not on PATH. */
function requireTool(command: string): void {
  if (!Bun.which(command)) {
    throw new Error(missingToolMessage(command));
  }
}

type RunResult = {
  stdout: string;
  stderr: string;
  exitCode: number;
};

async function runRaw(command: string[]): Promise<RunResult> {
  const [executable] = command;
  if (executable) {
    requireTool(executable);
  }

  const process = Bun.spawn({
    cmd: command,
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);

  return { stdout, stderr, exitCode };
}

async function runText(command: string[], allowedExitCodes: number[] = [0]): Promise<string> {
  const { stdout, stderr, exitCode } = await runRaw(command);

  if (!allowedExitCodes.includes(exitCode)) {
    throw new Error(`Command failed (${command.join(" ")}): ${stderr.trim() || stdout.trim()}`);
  }

  return stdout;
}

async function runJson<T>(command: string[], allowedExitCodes: number[] = [0]): Promise<T> {
  const raw = await runText(command, allowedExitCodes);

  try {
    return JSON.parse(raw) as T;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`Failed to parse JSON from '${command.join(" ")}': ${message}`);
  }
}

function openUrlCommand(url: string): string[] | null {
  if (process.platform === "darwin") {
    return ["open", url];
  }

  if (process.platform === "win32") {
    return ["cmd", "/c", "start", "", url];
  }

  for (const opener of ["xdg-open", "wslview", "gio"]) {
    if (Bun.which(opener)) {
      return opener === "gio" ? ["gio", "open", url] : [opener, url];
    }
  }

  return null;
}

/** Opens a URL in the default browser; prints it instead when no opener is available. */
async function openUrl(url: string): Promise<void> {
  const command = openUrlCommand(url);
  if (!command || !Bun.which(command[0]!)) {
    console.log(url);
    return;
  }

  try {
    await runText(command);
  } catch {
    console.log(url);
  }
}

export { knownTools, missingToolMessage, openUrl, requireTool, runJson, runRaw, runText };
export type { RunResult, ToolInfo };
