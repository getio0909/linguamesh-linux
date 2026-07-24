# Linux Repository Instructions

## Required reading

Before changing this repository, read `GLOBAL_GOAL.md`, `REPOSITORY_ROLE.md`, `IMPLEMENTATION_STATUS.md`, and the relevant local documentation. Read the authoritative `PROJECT_GOAL.md` from the sibling `linguamesh-project` repository when it is available, and verify its SHA-256 against `GLOBAL_GOAL.md`.

## Scope

This repository owns only the native Linux client, Linux platform services, application packaging, and Linux-specific tests. Shared translation, provider, persistence, document, and routing behavior belongs in `linguamesh-core`. Canonical UI strings belong in `linguamesh-l10n`.

Use stable Rust, GTK 4 through gtk-rs, GLib/GIO, and libadwaita where useful without making core behavior depend on GNOME-only services. Prefer direct typed calls into the shared Rust application layer. Use XDG paths and portals, Secret Service with explicit session-only fallback, and native Wayland behavior with practical X11 coverage.

## Workflow

1. Inspect `git status --short --branch` and preserve unrelated changes.
2. State uncertain decisions with `Assumption:`.
3. Implement the smallest complete native behavior with tests.
4. Run every available check documented in `docs/testing.md`.
5. Update `IMPLEMENTATION_STATUS.md` with commands and results.
6. Update architecture, testing, and release documentation when behavior changes.

All code comments must be in Simplified Chinese on separate lines immediately above the code they describe. All console, log, diagnostic, and command-line output strings must be in English.

## Current commands

Use the pinned Rust toolchain and exact commands in `docs/testing.md`. Hosts without GTK 4 and
libadwaita development headers can format, lint, and test the toolkit-independent state and the
real shared-core worker with `--features demo-provider`. Run `--all-features` only when native
headers are present. Native CI pins the approved Core revision
`f5b818c3598d78e7cac30604577fa8057d380737`; changing it requires a new compatibility review. This
adds the bundled `unix-none` fail-closed storage regression on top of ABI 1 and `unix-excl`
hardening while preserving the Linux runtime contract. Do not
invent successful GTK, Flatpak,
packaging, Wayland, or X11 results.

## Safety

Never commit credentials, signing keys, translated user content, or sensitive diagnostics. Never silently fall back from Secret Service to plaintext storage. Do not weaken TLS, portal, sandbox, file-permission, or core compatibility protections to make a check pass. Never publish or label a release stable without compatible pinned core and localization versions plus reproducible evidence.
