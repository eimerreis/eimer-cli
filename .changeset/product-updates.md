---
"eimer": minor
---

Add `eimer product` to turn Changesets changelogs into public update posts: `status` lists user-facing changes no post covers yet, `update` lets you pick them, writes a draft post and copies a prompt for a coding agent, `check` validates posts in CI, and `init` writes the repo config.

Clipboard copies (`pr copy`, `release changelog`, `product update`) now fall back to the terminal's clipboard via OSC 52 when no clipboard tool is installed, so they work on headless machines and over SSH.
