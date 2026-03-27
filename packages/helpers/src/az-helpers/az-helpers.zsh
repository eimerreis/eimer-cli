
# Set git alias for branch-name
git config --global alias.branch-name "rev-parse --abbrev-ref HEAD"

: "${EIMER_AZ_HELPERS_DIR:=${${(%):-%N}:A:h}}"

# Function to create PR for current branch and open in browser
pr:create() {
  pr create "$@"
}

pr:list() {
  tsx "$EIMER_AZ_HELPERS_DIR/prs-for-me.ts"
}

pipeline:list() {
  tsx "$EIMER_AZ_HELPERS_DIR/pipeline-runs.ts"
}

task:list() {
  tsx "$EIMER_AZ_HELPERS_DIR/recent-tasks.ts"
}

task:update() {
  tsx "$EIMER_AZ_HELPERS_DIR/update-task.ts" "$@"
}

task:create() {
  tsx "$EIMER_AZ_HELPERS_DIR/create-task.ts" "$@"
}
