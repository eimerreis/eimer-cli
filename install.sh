#!/bin/sh
# eimer installer for Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/eimerreis/eimer-cli/main/install.sh | sh
#
# Environment variables:
#   EIMER_VERSION      install a specific version (e.g. 0.2.0) instead of the latest
#   EIMER_INSTALL_DIR  install directory (default: ~/.local/bin)
#   EIMER_NO_LINKS=1   do not create the `task`, `pr`, `pipeline`, `release` symlinks
#   GITHUB_TOKEN       optional, avoids GitHub API rate limits
#   EIMER_GITHUB_URL / EIMER_GITHUB_API  override https://github.com / https://api.github.com (mirrors, tests)

set -eu

REPO="eimerreis/eimer-cli"
TAG_PREFIX="eimer/v"
INSTALL_DIR="${EIMER_INSTALL_DIR:-$HOME/.local/bin}"
GITHUB_URL="${EIMER_GITHUB_URL:-https://github.com}"
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

github_get() {
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    curl -fsSL -H "Authorization: Bearer ${GITHUB_TOKEN}" -H "Accept: application/vnd.github+json" "$1"
  else
    curl -fsSL -H "Accept: application/vnd.github+json" "$1"
  fi
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
  version="${EIMER_VERSION:-}"
  version="${version#v}"
  if [ -z "$version" ]; then
    version=$(latest_version)
    [ -n "$version" ] || die "Could not determine the latest eimer release of ${REPO}."
  fi

  asset="eimer-${target}"
  base="${GITHUB_URL}/${REPO}/releases/download/${TAG_PREFIX}${version}"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM

  info "Downloading eimer ${version} (${target})"
  curl -fsSL -o "$tmp/eimer" "${base}/${asset}" || die "Download failed: ${base}/${asset}"
  curl -fsSL -o "$tmp/eimer.sha256" "${base}/${asset}.sha256" || die "Checksum download failed."

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
