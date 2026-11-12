# eimer

## 0.4.0

### Minor Changes

- ba2cec8: `eimer time hook install` adds a git `pre-push` hook that records your focus time automatically: the first push of a branch writes and commits its `.time/*.md` entry, later pushes add new focus time to it. `eimer time init` installs the hook, and `eimer time add --auto` does the same by hand. `eimer time preview` shows what the next push would record, without writing anything.

## 0.3.0

### Minor Changes

- 151d095: `eimer time` now records work as pending entries in `.time/` (`eimer time add`) and books them in one go with `eimer time book`, which appends the bookings to `TIME.md`. `eimer time suggest` measures your focus time from the prompts in local Claude Code and Codex transcripts (prompt to prompt when close together; time the agent works alone only counts once you return) instead of commit times, and shows agent active time alongside. `eimer time pending` lists open entries and the last booking. Breaking: `eimer time book <duration>` (per-PR booking) is replaced by `eimer time add <duration>`; the PR check accepts an added `.time/*.md` entry.

## 0.2.0

### Minor Changes

- 790710e: First release of the Rust `eimer` CLI. Adds `eimer time` to book pull request work to codeunity time: `login`, `projects`, `init`, `suggest`, `book`, `none` and `status`.
