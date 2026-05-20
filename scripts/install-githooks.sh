#!/usr/bin/env bash
set -euo pipefail

# ColdVox Git Hook Installer
# Following MasterDocumentationPlaybook 1.0.1 requirements for pre-push validation.

repo_root="$({
  cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd
})"

if [[ ! -d "$repo_root/.git" ]]; then
  echo "error: not a git checkout (missing $repo_root/.git)" >&2
  exit 1
fi

hooks_path="$repo_root/.githooks"

if [[ ! -d "$hooks_path" ]]; then
  echo "error: missing $hooks_path (expected repo-tracked hooks)" >&2
  exit 1
fi

# 1) Configure git to use the tracked hooks directory
git -C "$repo_root" config --local core.hooksPath .githooks

# 2) Ensure all hooks are executable
echo "Setting permissions for hooks..."
chmod +x "$hooks_path"/*

# 3) Ensure critical scripts called by hooks are executable
chmod +x "$repo_root/scripts/validate_domain_docs_naming.py"

echo "✓ Git hooks installed successfully from .githooks/"
