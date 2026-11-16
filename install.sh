#!/bin/sh
# eimer installer for Linux and macOS.
#
#   gh api repos/eimerreis/eimer-cli/contents/install.sh -H 'Accept: application/vnd.github.raw' | sh
#
# The repo is private: the installer authenticates with GITHUB_TOKEN, or the GitHub CLI login.
#
# Environment variables:
#   EIMER_VERSION      install a specific version (e.g. 0.2.0) instead of the latest
#   EIMER_INSTALL_DIR  install directory (default: ~/.local/bin)
#   EIMER_NO_LINKS=1   do not create the `task`, `pr`, `pipeline`, `release` symlinks
#   GITHUB_TOKEN       token with access to the repo (default: `gh auth token`)
#   EIMER_GITHUB_API   override https://api.github.com (mirrors, tests)

set -eu

REPO="eimerreis/eimer-cli"
TAG_PREFIX="eimer/v"
INSTALL_DIR="${EIMER_INSTALL_DIR:-$HOME/.local/bin}"
GITHUB_API="${EIMER_GITHUB_API:-https://api.github.com}"

info() { printf 'i %s\n' "$*"; }
ok() { printf '+ %s\n' "$*"; }
warn() { printf '! %s\n' "$*" >&2; }
die() {
  printf 'X %s\n' "$*" >&2
  exit 1
}

need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed."; }

need curl
need uname

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) die "Unsupported architecture: $arch" ;;
  esac
  case "$os" in
    Linux) echo "${arch}-unknown-linux-musl" ;;
    Darwin) echo "${arch}-apple-darwin" ;;
    *) die "Unsupported OS: $os (on Windows use install.ps1)" ;;
  esac
}

github_token() {
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    printf '%s' "$GITHUB_TOKEN"
  elif command -v gh >/dev/null 2>&1; then
    gh auth token 2>/dev/null || true
  fi
}

# github_get <url> [accept] [curl args...]
github_get() {
  url=$1
  accept=${2:-application/vnd.github+json}
  shift
  [ "$#" -gt 0 ] && shift
  curl -fsSL -H "Authorization: Bearer ${TOKEN}" -H "Accept: ${accept}" "$@" "$url"
}

# URL of the release asset named $2 in the release JSON on stdin (pretty-printed by the API).
asset_url() {
  awk -v name="$2" '
    /"url": *".*\/releases\/assets\/[0-9]+"/ { sub(/.*"url": *"/, ""); sub(/".*/, ""); url = $0 }
    /"name": *"/ { n = $0; sub(/.*"name": *"/, "", n); sub(/".*/, "", n); if (n == name && url != "") { print url; exit } }
  ' "$1"
}

latest_version() {
  # Newest stable eimer release; `release/v*` tags belong to @tapio/release and are skipped.
  github_get "${GITHUB_API}/repos/${REPO}/releases?per_page=50" |
    sed -n 's/.*"tag_name": *"eimer\/v\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)".*/\1/p' |
    sort -t. -k1,1n -k2,2n -k3,3n |
    tail -n 1
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    die "Neither sha256sum nor shasum is available to verify the download."
  fi
}

check_prerequisites() {
  if ! command -v az >/dev/null 2>&1; then
    warn "Azure CLI (az) not found. eimer needs it for Azure DevOps: https://learn.microsoft.com/cli/azure/install-azure-cli"
  elif ! az extension show --name azure-devops >/dev/null 2>&1; then
    warn "The azure-devops extension is missing. Install it with: az extension add --name azure-devops"
  fi
  if ! command -v gh >/dev/null 2>&1; then
    info "GitHub CLI (gh) not found. It is only needed for pull requests in GitHub repos."
  fi
}

main() {
  target=$(detect_target)
  TOKEN=$(github_token)
  [ -n "$TOKEN" ] || die "eimer-cli is a private repo: run 'gh auth login' or set GITHUB_TOKEN."
  version="${EIMER_VERSION:-}"
  version="${version#v}"
  if [ -z "$version" ]; then
    version=$(latest_version)
    [ -n "$version" ] || die "Could not determine the latest eimer release of ${REPO}."
  fi

  asset="eimer-${target}"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM

  tag=$(printf '%s%s' "$TAG_PREFIX" "$version" | sed 's|/|%2F|g')
  github_get "${GITHUB_API}/repos/${REPO}/releases/tags/${tag}" >"$tmp/release.json" ||
    die "Release ${TAG_PREFIX}${version} not found in ${REPO}."
  binary_url=$(asset_url "$tmp/release.json" "$asset")
  checksum_url=$(asset_url "$tmp/release.json" "${asset}.sha256")
  [ -n "$binary_url" ] && [ -n "$checksum_url" ] || die "Release ${TAG_PREFIX}${version} has no asset ${asset}."

  info "Downloading eimer ${version} (${target})"
  github_get "$binary_url" application/octet-stream -o "$tmp/eimer" || die "Download failed: ${asset}"
  github_get "$checksum_url" application/octet-stream -o "$tmp/eimer.sha256" || die "Checksum download failed."

  expected=$(cut -d' ' -f1 <"$tmp/eimer.sha256")
  actual=$(sha256_of "$tmp/eimer")
  [ "$expected" = "$actual" ] || die "Checksum mismatch (expected $expected, got $actual)."

  mkdir -p "$INSTALL_DIR"
  chmod +x "$tmp/eimer"
  mv "$tmp/eimer" "$INSTALL_DIR/eimer"
  ok "Installed eimer ${version} to ${INSTALL_DIR}/eimer"

  if [ "${EIMER_NO_LINKS:-}" != "1" ]; then
    "$INSTALL_DIR/eimer" link --dir "$INSTALL_DIR" || warn "Could not create command links; run 'eimer link' later."
  fi

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      warn "${INSTALL_DIR} is not on your PATH. Add this to your shell profile:"
      printf '    export PATH="%s:$PATH"\n' "$INSTALL_DIR" >&2
      ;;
  esac

  check_prerequisites
  info "Update later with: eimer upgrade"
}

main "$@"
