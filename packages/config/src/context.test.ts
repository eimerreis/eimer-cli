import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { findRcFile, resolveContext, selectContext, toOrgUrl, writeRcFile } from "./context";
import type { EimerConfig } from "./schema";

const config: EimerConfig = {
  defaultContext: "tapio",
  contexts: {
    tapio: { org: "tapioone", project: "tapio", team: "Core" },
    homag: { org: "homag-group", project: "onehomag" },
  },
};

describe("selectContext", () => {
  const rc = { path: "/x/.eimerrc", context: "rc-ctx" };

  test("flag beats env, rc and default", () => {
    expect(selectContext({ flag: "f", env: "e", rc, defaultContext: "d" })).toEqual({ name: "f", source: "flag" });
  });

  test("env beats rc and default", () => {
    expect(selectContext({ env: "e", rc, defaultContext: "d" })).toEqual({ name: "e", source: "env" });
  });

  test("rc beats default", () => {
    expect(selectContext({ rc, defaultContext: "d" })).toEqual({ name: "rc-ctx", source: "rc", rcPath: "/x/.eimerrc" });
  });

  test("falls back to default", () => {
    expect(selectContext({ defaultContext: "d" })).toEqual({ name: "d", source: "global" });
  });

  test("returns null when nothing is set", () => {
    expect(selectContext({ flag: "  ", env: "" })).toBeNull();
  });
});

describe("toOrgUrl", () => {
  test("expands bare org names", () => {
    expect(toOrgUrl("tapioone")).toBe("https://dev.azure.com/tapioone");
  });

  test("keeps full URLs", () => {
    expect(toOrgUrl("https://dev.azure.com/homag-group/")).toBe("https://dev.azure.com/homag-group");
  });
});

describe("rc files and resolveContext", () => {
  let root: string;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), "eimer-ctx-"));
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  test("findRcFile walks up to the nearest .eimerrc", async () => {
    const nested = join(root, "a", "b", "c");
    await mkdir(nested, { recursive: true });
    await writeRcFile(root, "homag");
    await writeRcFile(join(root, "a"), "tapio");

    const found = await findRcFile(nested);
    expect(found).toEqual({ path: join(root, "a", ".eimerrc"), context: "tapio" });
  });

  test("findRcFile rejects malformed files", async () => {
    await Bun.write(join(root, ".eimerrc"), "{ \"ctx\": 1 }");
    await expect(findRcFile(root)).rejects.toThrow("Invalid");
  });

  test("rc file overrides the global default", async () => {
    await writeRcFile(root, "homag");
    const ctx = await resolveContext({ cwd: root, env: {}, config });
    expect(ctx).toMatchObject({
      name: "homag",
      source: "rc",
      org: "homag-group",
      project: "onehomag",
      orgUrl: "https://dev.azure.com/homag-group",
    });
  });

  test("env var overrides the rc file", async () => {
    await writeRcFile(root, "homag");
    const ctx = await resolveContext({ cwd: root, env: { EIMER_CONTEXT: "tapio" }, config });
    expect(ctx).toMatchObject({ name: "tapio", source: "env", team: "Core" });
  });

  test("flag overrides everything", async () => {
    await writeRcFile(root, "tapio");
    const ctx = await resolveContext({ cwd: root, flag: "homag", env: { EIMER_CONTEXT: "tapio" }, config });
    expect(ctx).toMatchObject({ name: "homag", source: "flag" });
  });

  test("returns null without any context configured", async () => {
    const ctx = await resolveContext({ cwd: root, env: {}, config: {} });
    expect(ctx).toBeNull();
  });

  test("throws a helpful error for unknown contexts", async () => {
    await expect(resolveContext({ cwd: root, flag: "nope", env: {}, config })).rejects.toThrow(
      "Unknown context 'nope' (from --context flag). Known contexts: tapio, homag.",
    );
  });
});
