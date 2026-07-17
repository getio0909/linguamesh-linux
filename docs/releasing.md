# Releasing

## Current state

An unreleased native application target and a pinned Flatpak packaging scaffold now exist. The
GNOME 49 SDK workflow builds a prerelease CI bundle from pinned sources and runs the bounded
Xvfb/private-D-Bus sandbox smoke. The native gate verifies the real document-portal lease lifecycle,
but no interactive file-chooser portal, desktop notification delivery, signed artifact, or
distributable release has been verified. The
vertical slice must not be tagged or published as a product release, and no packaging claim beyond
the recorded CI build is valid. Its bundled fake provider is development-only behavior. The optional
OpenAI-compatible endpoint form accepts a one-shot session credential, clears the field
immediately, and never persists the credential value. A saved-profile dropdown and explicit remember
checkbox can create, update, activate, switch, and remove multiple rows containing provider names,
endpoints, model preferences, and persistent Secret Service references in the XDG user data SQLite database. The
private application directory is `0700` and the database is `0600`. Removing the connected row
leaves its already validated runtime session active but no longer persistent. Core's no-follow
SQLite open behavior on Linux's default Unix VFS remains required. Startup prefills the last
persistently activated row but never auto-connects, so a credential must be entered again when
required. A derived Provider setup card guides configuration, explicit connection, and deliberate
model selection without storing a completion flag, distinguishes worker failure from startup, and
shows the confirmed next-request stable ID/model identity.
The external-provider path is tested only with LinguaMesh's loopback fake provider. Persistent
secret references use the Linux GIO Secret Service adapter and fail closed when the desktop keyring
is unavailable or requires an interactive prompt. The native workflow
pins reviewed Core functional revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, whose storage delta adds
`SQLITE_OPEN_NOFOLLOW`, rather than checking out a floating branch. Functional revision
`7d7eba9960b657f0460fb0daaaaebaaa609f39b1` passed Native Linux run `29604269568` (job
`87963611054`) and repository-foundation run `29604269516`; it includes the no-credential
OpenAI-compatible loopback regression, secure onboarding fixtures, strict Clippy, both display
gates, and the all-feature build. Earlier functional revision
`9729b23ce1a4280ebb434339e880010103b4859d` passed Native Linux run `29580444723` (job
`87884607879`) with 65 library tests and the first real GTK flow. Wayland-gate revision
`10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed Native Linux run `29582513061` (job
`87891382469`) with the same GTK binary test succeeding under both X11/Xvfb and forced
Wayland/headless Weston. Neither validation creates a distributable artifact or satisfies the
future release gate below.

The current native gate also includes a real post-startup `ENOSPC` regression for persistent model
updates, profile deletion, and provider switching. Runtime-storage functional revision
`c37702c76c3b1a2f9cec805cf9e219721ef7b5ce` passed Native Linux run `29586532049` (job
`87904787338`) and repository-foundation run `29586531915` (job `87904787120`). Ubuntu exercised
the controlled mount fallback and proved exact rejection, continued use of the prior session,
session-only post-fault model selection, and restart recovery of only pre-fault state. This is not
evidence for corruption, power-loss recovery, read-only media, or every storage-failure path.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. real desktop Secret Service CRUD/cleanup, the session-only fallback, complete SecretRef-backed profile lifecycle, multi-profile management, bounded native text import (including source-editor drag-and-drop), XDG paths, document-portal leases and interactive file workflows, accessibility, Wayland, practical X11 support, desktop notification delivery, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
