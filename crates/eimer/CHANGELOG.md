# eimer

## 0.10.0

### Minor Changes

- 2599b93: New `eimer doc new|render`: invoices, quotes, letters, reminders, contracts and time reports rendered from YAML with Typst. Templates and brand fonts are embedded; sender details live in `~/.config/eimer/sender.yaml`. `eimer time report --project` uses it instead of the external `doc` CLI.

## 0.9.0

### Minor Changes

- f0627f5: Add `eimer secrets` for runtime secrets as SOPS/age-encrypted dotenv files in the repo: `init`, `recipients add|remove|list` (host keys from 1Password), `edit`, `get` and `decrypt`.

## 0.8.0

### Minor Changes

- a3b4251: `eimer time report --project <id|name>` renders the time report PDF (Zeitbuchungen) for a project via the `doc` CLI from freelance-ops.

## 0.7.0

### Minor Changes

- 3df636c: `eimer time`: the pre-push hook counts only sessions of the pushed repo. Git exports `GIT_DIR` to hooks, which made every agent session on the machine look like part of the repo. New `--include <dir>` on `suggest` and `add --auto` also counts sessions started in another folder, such as a scratch folder whose work belongs to the repo.

## 0.6.1

### Patch Changes

- 5497f62: `eimer product check` accepts plain commit hashes in posts dated before `since`, so a retrospective of features from before the changelogs passes.

## 0.6.0

### Minor Changes

- 43bd4c7: Add `eimer product` to turn Changesets changelogs into public update posts: `status` lists user-facing changes no post covers yet, `update` lets you pick them, writes a draft post and copies a prompt for a coding agent, `check` validates posts in CI, and `init` writes the repo config.

  Clipboard copies (`pr copy`, `release changelog`, `product update`) now fall back to the terminal's clipboard via OSC 52 when no clipboard tool is installed, so they work on headless machines and over SSH.

## 0.5.0

### Minor Changes

- 2ec389b: Add `eimer time report`: hours per project for a date window (default: current week), hours not yet invoiced with the oldest unbilled day, and budget progress from codeunity time. `--json` for dashboards and scripts.

## 0.4.0

### Minor Changes

- ba2cec8: `eimer time hook install` adds a git `pre-push` hook that records your focus time automatically: the first push of a branch writes and commits its `.time/*.md` entry, later pushes add new focus time to it. `eimer time init` installs the hook, and `eimer time add --auto` does the same by hand. `eimer time preview` shows what the next push would record, without writing anything.

## 0.3.0

### Minor Changes

- 151d095: `eimer time` now records work as pending entries in `.time/` (`eimer time add`) and books them in one go with `eimer time book`, which appends the bookings to `TIME.md`. `eimer time suggest` measures your focus time from the prompts in local Claude Code and Codex transcripts (prompt to prompt when close together; time the agent works alone only counts once you return) instead of commit times, and shows agent active time alongside. `eimer time pending` lists open entries and the last booking. Breaking: `eimer time book <duration>` (per-PR booking) is replaced by `eimer time add <duration>`; the PR check accepts an added `.time/*.md` entry.

## 0.2.0

### Minor Changes

- 790710e: First release of the Rust `eimer` CLI. Adds `eimer time` to book pull request work to codeunity time: `login`, `projects`, `init`, `suggest`, `book`, `none` and `status`.
