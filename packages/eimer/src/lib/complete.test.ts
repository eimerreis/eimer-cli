import { describe, expect, test } from "bun:test";
import { completeWords, type CompletionNode } from "./complete";

const root: CompletionNode = {
  name: "eimer",
  commands: [
    {
      name: "pr",
      description: "Pull request commands",
      commands: [{ name: "list", description: "List open\n pull requests", options: { json: { description: "Print JSON" }, all: {} } }],
    },
    { name: "doctor", description: "Check setup" },
  ],
};

describe("completeWords", () => {
  test("lists top-level commands and global flags", () => {
    expect(completeWords(root, [])).toEqual(["pr:Pull request commands", "doctor:Check setup", "--help:Show help", "--version:Show version"]);
  });

  test("descends into subcommands", () => {
    expect(completeWords(root, ["pr"])).toEqual(["list:List open pull requests"]);
  });

  test("lists options of a leaf command and ignores typed flags", () => {
    expect(completeWords(root, ["pr", "list", "--json"])).toEqual(["--json:Print JSON", "--all"]);
  });

  test("ignores unknown words", () => {
    expect(completeWords(root, ["nope"])).toEqual(completeWords(root, []));
  });
});
