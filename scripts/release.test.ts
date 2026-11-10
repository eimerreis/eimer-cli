import { describe, expect, test } from "bun:test";
import { cargoVersion, isChangesetFile, isExemptTitle, setCargoLockVersion, setCargoTomlVersion, unexpectedPaths } from "./release";

describe("pull request rules", () => {
  test("dependency updates are exempt", () => {
    expect(isExemptTitle("chore(deps): bump x")).toBe(true);
    expect(isExemptTitle("chore(deps-dev): bump y")).toBe(true);
    expect(isExemptTitle("chore: tidy")).toBe(false);
    expect(isExemptTitle("feat(deps): x")).toBe(false);
  });

  test("changeset files exclude the README", () => {
    expect(isChangesetFile(".changeset/brave-owl.md")).toBe(true);
    expect(isChangesetFile(".changeset/README.md")).toBe(false);
    expect(isChangesetFile(".changeset/config.json")).toBe(false);
  });
});

describe("release commit", () => {
  test("only metadata may change", () => {
    const ok = [".changeset/a.md", "crates/eimer/CHANGELOG.md", "packages/release/package.json", "bun.lock", "Cargo.lock", "crates/eimer/Cargo.toml"];
    expect(unexpectedPaths(ok)).toEqual([]);
    expect(unexpectedPaths(["crates/eimer/src/main.rs", ".github/workflows/x.yml"])).toEqual([
      "crates/eimer/src/main.rs",
      ".github/workflows/x.yml",
    ]);
  });

  test("syncs the Cargo version", () => {
    const toml = '[package]\nname = "eimer"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\nanyhow = { version = "1" }\n';
    expect(setCargoTomlVersion(toml, "0.2.0")).toBe(toml.replace('"0.1.0"', '"0.2.0"'));
    const lock = '[[package]]\nname = "dialoguer"\nversion = "0.12.0"\n\n[[package]]\nname = "eimer"\nversion = "0.1.0"\ndependencies = [\n';
    expect(setCargoLockVersion(lock, "eimer", "0.2.0")).toContain('name = "eimer"\nversion = "0.2.0"');
    expect(setCargoLockVersion(lock, "eimer", "0.2.0")).toContain('name = "dialoguer"\nversion = "0.12.0"');
    expect(() => setCargoLockVersion(lock, "missing", "1.0.0")).toThrow();
    expect(cargoVersion(toml)).toBe("0.1.0");
  });
});
