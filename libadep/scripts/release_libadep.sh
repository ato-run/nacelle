#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: release_libadep.sh <version> [--dry-run]

Examples:
  release_libadep.sh 0.2.0
  release_libadep.sh 0.2.0 --dry-run

The script will:
  1. Validate the working tree is clean.
  2. Create an annotated git tag `libadep-v<version>` at HEAD.
  3. Update the version manifest file at
     gumball-platform/docs/implementation/versions/libadep.toml.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || $# -lt 1 ]]; then
  usage
  exit 1
fi

VERSION="$1"
shift

DRY_RUN=false
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=true
fi

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "error: version must be SemVer compatible (got '$VERSION')" >&2
  exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
SCRIPT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "$REPO_ROOT" != "$SCRIPT_ROOT" ]]; then
  echo "error: script must be executed from the monorepo containing gumball-adep" >&2
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "error: working tree has uncommitted changes; aborting" >&2
  exit 1
fi

TAG="libadep-v${VERSION}"
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "error: tag '$TAG' already exists" >&2
  exit 1
fi

SHA="$(git rev-parse HEAD)"
DATE_UTC="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

versions_file="$REPO_ROOT/gumball-platform/docs/implementation/versions/libadep.toml"

if [[ ! -f "$versions_file" ]]; then
  cat >"$versions_file" <<'EOF'
# libadep release manifest
# このファイルは release_libadep.sh により自動更新されます。

[active]
version = ""
tag = ""
rev = ""
released_at = ""

[[history]]
version = ""
tag = ""
rev = ""
released_at = ""
EOF
fi

update_versions_file() {
  local tmp
  tmp="$(mktemp)"
  awk -v ver="$VERSION" -v tag="$TAG" -v sha="$SHA" -v date="$DATE_UTC" '
    BEGIN {
      active_written = 0;
      history_inserted = 0;
    }
    /^\[active\]/ {
      print;
      getline; print "version = \"" ver "\"";
      getline; print "tag = \"" tag "\"";
      getline; print "rev = \"" sha "\"";
      getline; print "released_at = \"" date "\"";
      active_written = 1;
      next;
    }
    /^\[\[history\]\]/ && history_inserted == 0 {
      print "[[history]]";
      print "version = \"" ver "\"";
      print "tag = \"" tag "\"";
      print "rev = \"" sha "\"";
      print "released_at = \"" date "\"";
      print "";
      history_inserted = 1;
    }
    { print }
    END {
      if (active_written == 0) {
        print "[active]";
        print "version = \"" ver "\"";
        print "tag = \"" tag "\"";
        print "rev = \"" sha "\"";
        print "released_at = \"" date "\"";
        print "";
      }
      if (history_inserted == 0) {
        print "[[history]]";
        print "version = \"" ver "\"";
        print "tag = \"" tag "\"";
        print "rev = \"" sha "\"";
        print "released_at = \"" date "\"";
      }
    }
  ' "$versions_file" >"$tmp"
  mv "$tmp" "$versions_file"
}

if ! $DRY_RUN; then
  git tag -a "$TAG" -m "libadep release $VERSION"
  update_versions_file
else
  echo "[dry-run] would create tag $TAG"
  update_versions_file
  git tag -d "$TAG" >/dev/null 2>&1 || true
  git checkout -- "$versions_file"
fi

echo "Release prepared:"
echo "  version:     $VERSION"
echo "  tag:         $TAG"
echo "  commit:      $SHA"
echo "  versions:    $versions_file"
if $DRY_RUN; then
  echo "Dry-run completed; no tag persisted."
else
  echo "Tag created. Update the versions file and push the tag when ready:"
  echo "  git push origin \"$TAG\""
fi

