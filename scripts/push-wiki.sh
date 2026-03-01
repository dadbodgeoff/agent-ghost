#!/usr/bin/env bash
set -euo pipefail

# Push wiki/ directory contents to the GitHub wiki repository.
#
# Prerequisites:
#   1. You must have created at least one wiki page via the GitHub UI first.
#      Go to: https://github.com/dadbodgeoff/agent-ghost/wiki
#      Click "Create the first page" and save with any content.
#   2. SSH key must have push access to the repo.
#
# Usage:
#   ./scripts/push-wiki.sh

REPO_ROOT="$(git rev-parse --show-toplevel)"
WIKI_DIR="${REPO_ROOT}/wiki"
WIKI_REPO_URL="git@github-personal:dadbodgeoff/agent-ghost.wiki.git"
TEMP_DIR=$(mktemp -d)

echo "==> Cloning wiki repo into ${TEMP_DIR}..."
git clone "${WIKI_REPO_URL}" "${TEMP_DIR}"

echo "==> Removing old wiki content..."
# Remove all .md files from the wiki repo (preserve .git)
find "${TEMP_DIR}" -maxdepth 1 -name '*.md' -delete

echo "==> Copying ${WIKI_DIR}/ into wiki repo..."
cp "${WIKI_DIR}"/*.md "${TEMP_DIR}/"

echo "==> Committing and pushing..."
cd "${TEMP_DIR}"
git add -A
if git diff --cached --quiet; then
    echo "==> No changes to push. Wiki is already up to date."
else
    git commit -m "Update wiki: all 37 crates documented across 10 layers

- 38 pages total (Home + 37 crate deep-dives)
- 10,692 lines of documentation
- Covers architecture decisions, security properties, test strategies
- Layer 0-10 complete coverage"
    git push origin master
    echo "==> Wiki pushed successfully!"
fi

echo "==> Cleaning up temp directory..."
rm -rf "${TEMP_DIR}"
echo "==> Done."
