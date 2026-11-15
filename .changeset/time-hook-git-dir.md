---
"eimer": minor
---

`eimer time`: the pre-push hook counts only sessions of the pushed repo. Git exports `GIT_DIR` to hooks, which made every agent session on the machine look like part of the repo. New `--include <dir>` on `suggest` and `add --auto` also counts sessions started in another folder, such as a scratch folder whose work belongs to the repo.
