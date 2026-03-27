import { defineConfig } from "@bunli/core";

export default defineConfig({
  name: "release",
  version: "0.1.0",
  description: "CLI for release workflows",

  commands: {
    directory: "./src/commands",
  },

  build: {
    entry: "./src/index.ts",
    outdir: "./dist",
    targets: ["native"],
    minify: true,
    sourcemap: true,
    compress: false,
  },

  dev: {
    watch: true,
    inspect: true,
  },

  test: {
    pattern: ["**/*.test.ts", "**/*.spec.ts"],
    coverage: true,
    watch: false,
  },

  plugins: [],
});
