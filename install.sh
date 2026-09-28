#!/usr/bin/env bash
# Bootstrap eimer on a fresh machine: installs Bun if missing, then runs `bun run setup`.
#
#   ./install.sh                # from a checkout
#   ./install.sh --all          # extra args are passed to scripts/setup.ts
#
# Fresh machine without a checkout:
#   git clone https://github.com/eimerreis/eimer-cli.git ~/dev/eimer-cli && ~/dev/eimer-cli/install.sh
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v bun >/dev/null 2>&1; then
  echo "Bun not found; installing from https://bun.sh ..."
  curl -fsSL https://bun.sh/install | bash
  export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
  export PATH="$BUN_INSTALL/bin:$PATH"
fi

exec bun run scripts/setup.ts "$@"
