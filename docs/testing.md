# Testing and Validation

## Current foundation checks

Setup requires only Git and standard POSIX shell tools. From the repository root, run this exact validation:

```sh
set -euo pipefail
required_files="README.md LICENSE AGENTS.md REPOSITORY_ROLE.md GLOBAL_GOAL.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md THIRD_PARTY_NOTICES.md IMPLEMENTATION_STATUS.md docs/architecture.md docs/testing.md docs/releasing.md .gitignore .github/workflows/foundation.yml"
for file in $required_files; do
  test -s "$file" || {
    printf 'Missing required file: %s\n' "$file"
    exit 1
  }
done
grep -Fqx 'Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`' GLOBAL_GOAL.md
if find . -type f \( -name '*.md' -o -name '*.yml' \) -not -path './.git/*' -exec awk '/[[:blank:]]$/ { printf "%s:%d: trailing whitespace\n", FILENAME, FNR; bad=1 } END { exit bad }' {} +; then
  printf '%s\n' 'Foundation validation passed.'
else
  exit 1
fi
git diff --check
```

This validates required non-empty files, the pinned goal digest, trailing whitespace, and tracked-file patch whitespace. It does not build or test an application.

## Command availability

| Activity | Current command | Status |
| --- | --- | --- |
| Setup | No dependency command | Foundation has no dependencies |
| Format | No formatter command | Unavailable until Rust sources and policy exist |
| Lint | Foundation shell block above | Available for documentation only |
| Test | No product test command | Unavailable until test targets exist |
| Build | No product build command | Unavailable until a Cargo package exists |

When implementation begins, replace unavailable entries with exact pinned toolchain and package commands. Product CI must eventually cover `cargo fmt`, strict Clippy, Rust tests, GTK component and accessibility tests, virtual-display UI tests, portal abstractions, Wayland/X11 smoke tests, release builds, Flatpak smoke tests, and dependency/license review. Do not record these future checks as passing before they run.

