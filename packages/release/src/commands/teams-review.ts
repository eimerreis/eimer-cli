import { randomUUID } from "node:crypto";
import { readFile, rm, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";

type ReviewAction = "post" | "edit" | "save-post" | "cancel";

type ReviewResult = {
  markdown: string;
  shouldPost: boolean;
};

type ReviewPrompt = {
  select(
    message: string,
    options: {
      options: Array<{
        value: ReviewAction;
        label: string;
        hint?: string;
      }>;
      default?: ReviewAction;
    },
  ): Promise<ReviewAction>;
};

async function reviewTeamsMarkdown(markdown: string, prompt: ReviewPrompt): Promise<ReviewResult> {
  let currentMarkdown = markdown;
  printPreview(currentMarkdown, "Changelog to post to Teams:");

  while (true) {
    const editorChoices: Array<{ value: ReviewAction; label: string; hint?: string }> = [
      { value: "post", label: "Post now" },
      { value: "edit", label: "Edit changelog", hint: `Open in ${describeEditor()}` },
    ];

    if (canUseVsCodeSaveFlow()) {
      editorChoices.push({
        value: "save-post",
        label: "Edit and post on save",
        hint: "Open in VS Code; next save posts automatically",
      });
    }

    editorChoices.push({ value: "cancel", label: "Skip post" });

    const action = await prompt.select("Teams webhook review", {
      options: editorChoices,
      default: "post",
    });

    if (action === "post") {
      return { markdown: currentMarkdown, shouldPost: true };
    }

    if (action === "cancel") {
      return { markdown: currentMarkdown, shouldPost: false };
    }

    if (action === "save-post") {
      currentMarkdown = await editMarkdownAndWaitForSave(currentMarkdown);
      return { markdown: currentMarkdown, shouldPost: true };
    }

    currentMarkdown = await editMarkdownInEditor(currentMarkdown);
    printPreview(currentMarkdown, "Updated changelog to post:");
  }
}

async function editMarkdownInEditor(markdown: string): Promise<string> {
  return withEditableMarkdownFile(markdown, { cleanup: true }, async (tempFile) => {
    const process = Bun.spawn({
      cmd: resolveEditorCommand(tempFile),
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
    });

    const exitCode = await process.exited;
    if (exitCode !== 0) {
      throw new Error(`Editor exited with code ${exitCode}.`);
    }

    const updatedMarkdown = await readEditedMarkdown(tempFile);
    if (!updatedMarkdown.trim()) {
      throw new Error("Edited changelog is empty.");
    }

    return updatedMarkdown;
  });
}

async function editMarkdownAndWaitForSave(markdown: string): Promise<string> {
  return withEditableMarkdownFile(markdown, { cleanup: false }, async (tempFile) => {
    Bun.spawn({
      cmd: ["code", "--reuse-window", tempFile],
      stdin: "ignore",
      stdout: "ignore",
      stderr: "inherit",
    });

    console.log(`Opened ${tempFile} in VS Code.`);
    console.log("Waiting for next save. Save the file in VS Code to post automatically.");
    console.log(`The review file stays on disk: ${tempFile}`);

    const initialMtime = (await stat(tempFile)).mtimeMs;
    return waitForNextSave(tempFile, initialMtime);
  });
}

async function withEditableMarkdownFile<T>(
  markdown: string,
  options: { cleanup: boolean },
  run: (tempFile: string) => Promise<T>,
): Promise<T> {
  const tempFile = await resolveEditableFilePath();

  try {
    await writeFile(tempFile, `${markdown}\n`, "utf8");
    const writtenMarkdown = await readFile(tempFile, "utf8");
    if (!writtenMarkdown.trim()) {
      throw new Error(`Editable changelog file was empty after writing: ${tempFile}`);
    }

    return run(tempFile);
  } finally {
    if (options.cleanup) {
      await rm(tempFile, { force: true });
    }
  }
}

async function resolveEditableFilePath(): Promise<string> {
  const baseDir = process.cwd();
  const uniqueSuffix = `${Date.now()}-${randomUUID().slice(0, 8)}`;
  const prefix = `eimer-release-changelog-${uniqueSuffix}`;

  for (let index = 0; index < 100; index += 1) {
    const suffix = index === 0 ? "" : `-${index}`;
    const candidate = join(baseDir, `${prefix}${suffix}.md`);
    if (!(await Bun.file(candidate).exists())) {
      return candidate;
    }
  }

  throw new Error("Could not allocate editable changelog file path.");
}

async function waitForNextSave(tempFile: string, initialMtime: number): Promise<string> {
  let lastMtime = initialMtime;

  await Bun.sleep(1200);

  while (true) {
    await Bun.sleep(400);

    const currentStat = await stat(tempFile);
    if (currentStat.mtimeMs <= lastMtime) {
      continue;
    }

    lastMtime = currentStat.mtimeMs;
    const updatedMarkdown = await readEditedMarkdown(tempFile);
    if (updatedMarkdown.trim()) {
      return updatedMarkdown;
    }
  }
}

async function readEditedMarkdown(tempFile: string): Promise<string> {
  return (await readFile(tempFile, "utf8")).trimEnd();
}

function resolveEditorCommand(filePath: string): string[] {
  const configuredEditor = (process.env.GIT_EDITOR || process.env.VISUAL || process.env.EDITOR || "").trim();
  if (configuredEditor) {
    return wrapShellCommand(`${configuredEditor} ${quoteShellArg(filePath)}`);
  }

  if (Bun.which("code")) {
    return ["code", "--wait", filePath];
  }

  if (process.platform === "darwin" && Bun.which("open")) {
    return ["open", "-W", "-t", filePath];
  }

  if (Bun.which("nano")) {
    return ["nano", filePath];
  }

  if (Bun.which("vi")) {
    return ["vi", filePath];
  }

  throw new Error("No editor found. Set GIT_EDITOR, VISUAL, or EDITOR.");
}

function wrapShellCommand(command: string): string[] {
  if (process.platform === "win32") {
    return ["cmd", "/d", "/s", "/c", command];
  }

  return ["sh", "-lc", command];
}

function quoteShellArg(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function printPreview(markdown: string, title: string): void {
  console.log(`\n${title}\n`);
  console.log(markdown);
  console.log("");
}

function describeEditor(): string {
  const configuredEditor = (process.env.GIT_EDITOR || process.env.VISUAL || process.env.EDITOR || "").trim();
  if (configuredEditor) {
    return configuredEditor;
  }

  if (Bun.which("code")) {
    return "VS Code";
  }

  if (process.platform === "darwin" && Bun.which("open")) {
    return "default text editor";
  }

  if (Bun.which("nano")) {
    return "nano";
  }

  if (Bun.which("vi")) {
    return "vi";
  }

  return "configured editor";
}

function canUseVsCodeSaveFlow(): boolean {
  return Boolean(Bun.which("code"));
}

export { reviewTeamsMarkdown };
