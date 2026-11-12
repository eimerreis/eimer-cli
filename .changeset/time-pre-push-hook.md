---
"eimer": minor
---

`eimer time hook install` adds a git `pre-push` hook that records your focus time automatically: the first push of a branch writes and commits its `.time/*.md` entry, later pushes add new focus time to it. `eimer time init` installs the hook, and `eimer time add --auto` does the same by hand. `eimer time preview` shows what the next push would record, without writing anything.
