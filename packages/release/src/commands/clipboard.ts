async function copyToClipboard(text: string): Promise<void> {
  if (process.platform === "darwin") {
    await writeToClipboardCommand(["pbcopy"], text);
    return;
  }

  if (process.platform === "win32") {
    await writeToClipboardCommand(["clip"], text);
    return;
  }

  try {
    await writeToClipboardCommand(["wl-copy"], text);
    return;
  } catch {}

  await writeToClipboardCommand(["xclip", "-selection", "clipboard"], text);
}

async function writeToClipboardCommand(command: string[], text: string): Promise<void> {
  const process = Bun.spawn({
    cmd: command,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });

  if (!process.stdin) {
    throw new Error(`Could not open stdin for clipboard command '${command.join(" ")}'.`);
  }

  const writeResult = process.stdin.write(text);
  if (writeResult instanceof Promise) {
    await writeResult;
  }
  process.stdin.end();

  const [stderr, exitCode] = await Promise.all([new Response(process.stderr).text(), process.exited]);
  if (exitCode !== 0) {
    throw new Error(`Clipboard command failed (${command.join(" ")}): ${stderr.trim() || `exit code ${exitCode}`}`);
  }
}

export { copyToClipboard };
