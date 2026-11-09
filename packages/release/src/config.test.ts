import { describe, expect, test } from "bun:test";
import { prodStageForPipeline, type ReleaseConfig } from "./config";

describe("prodStageForPipeline", () => {
  const config: ReleaseConfig = {
    release: { prodStageName: "deploy_prod", prodStages: { "tapio.B2C.UI": "deploy-tapiousers" } },
  };

  test("matches pipeline names case-insensitively, first candidate wins", () => {
    expect(prodStageForPipeline(config, "TAPIO.b2c.ui")).toBe("deploy-tapiousers");
    expect(prodStageForPipeline(config, undefined, "tapio.b2c.ui")).toBe("deploy-tapiousers");
  });

  test("returns empty string without a remembered stage", () => {
    expect(prodStageForPipeline(config, "other")).toBe("");
    expect(prodStageForPipeline({}, "tapio.B2C.UI")).toBe("");
  });
});
