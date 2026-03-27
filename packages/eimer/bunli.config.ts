import { defineConfig } from "@bunli/core";

export default defineConfig({
  name: "eimer",
  version: "0.1.0",
  description: "Meta CLI for all scripts packages",
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
