# Eimer Scripts

Eimer Scripts automates recurring developer workflows around source control, delivery pipelines, releases, and work tracking.

## Release language

**Release branch**:
The source-control branch whose successful pipeline runs define the candidate and previously released commit range for a changelog.
_Avoid_: Default branch, master branch

**Production stage**:
The pipeline stage whose successful completion identifies a run as released to production.
_Avoid_: Prod job, deployment step
