import { describe, expect, test } from "bun:test";
import { missingToolMessage, requireTool, runJson, runText } from "./process";

describe("process helpers", () => {
  test("runText returns stdout", async () => {
    expect(await runText(["echo", "hello"])).toBe("hello\n");
  });

  test("runJson parses stdout", async () => {
    expect(await runJson<{ ok: boolean }>(["echo", '{"ok":true}'])).toEqual({ ok: true });
  });

  test("runText throws with stderr on failure", async () => {
    await expect(runText(["sh", "-c", "echo boom >&2; exit 3"])).rejects.toThrow("boom");
  });

  test("runText accepts allowed non-zero exit codes", async () => {
    expect(await runText(["sh", "-c", "echo partial; exit 8"], [0, 8])).toBe("partial\n");
  });

  test("missing tools get an install hint", () => {
    expect(() => requireTool("definitely-not-a-real-tool")).toThrow("was not found on your PATH");
    expect(missingToolMessage("az")).toContain("az extension add --name azure-devops");
  });
});
