# Eimer Scripts

Eimer Scripts automates recurring developer workflows around source control, delivery pipelines, releases, and work tracking.

## Release language

**Release branch**:
The source-control branch whose successful pipeline runs define the candidate and previously released commit range for a changelog.
_Avoid_: Default branch, master branch

**Production stage**:
The pipeline stage whose successful completion identifies a run as released to production.
_Avoid_: Prod job, deployment step

## Work tracking language

**Context**:
A named Azure DevOps org/project pair (plus optional default team and area path) defined in `~/.config/eimer/config.json` that task commands act in, e.g. `tapio` or `homag`.
_Avoid_: Profile, environment, account

**Not-started task**:
A task whose state is New, To Do, Not Started or Proposed, depending on the project's process template.
_Avoid_: Open task, backlog task
