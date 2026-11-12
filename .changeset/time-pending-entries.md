---
"eimer": minor
---

`eimer time` now records work as pending entries in `.time/` (`eimer time add`) and books them in one go with `eimer time book`, which appends the bookings to `TIME.md`. `eimer time suggest` measures agent activity from local Claude Code and Codex transcripts instead of commit times. `eimer time pending` lists open entries and the last booking. Breaking: `eimer time book <duration>` (per-PR booking) is replaced by `eimer time add <duration>`; the PR check accepts an added `.time/*.md` entry.
