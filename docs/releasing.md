# Releasing

## Current state

An unreleased native application target now exists, but no Flatpak manifest or distributable
artifact exists. The vertical slice must not be tagged or published as a product release, and no
packaging claim is valid. Its bundled fake provider is development-only behavior. The optional
OpenAI-compatible endpoint form accepts a one-shot session credential, clears the field
immediately, and never persists the credential or its reference. A saved-profile dropdown and
explicit remember checkbox can create, update, activate, switch, and remove multiple rows containing
only provider names, endpoints, and model preferences in the XDG user data SQLite database. The
private application directory is `0700` and the database is `0600`. Removing the connected row
leaves its already validated runtime session active but no longer persistent. Core's no-follow
SQLite open behavior on Linux's default Unix VFS remains required. Startup prefills the last
persistently activated row but never auto-connects, so a credential must be entered again when
required. A derived Provider setup card guides configuration, explicit connection, and deliberate
model selection without storing a completion flag, distinguishes worker failure from startup, and
shows the confirmed next-request stable ID/model identity.
The external-provider path is tested only with LinguaMesh's loopback fake provider. Persistent
secret references fail closed because no native Secret Service backend exists. The native workflow
pins reviewed Core functional revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, whose storage delta adds
`SQLITE_OPEN_NOFOLLOW`, rather than checking out a floating branch. Functional revision
`9729b23ce1a4280ebb434339e880010103b4859d` passed Native Linux run `29580444723` (job
`87884607879`) with 65 library tests, the real GTK test, strict Clippy, and the all-feature build;
that evidence used the X11/Xvfb gate. Wayland-gate revision
`10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed Native Linux run `29582513061` (job
`87891382469`) with the same GTK binary test succeeding under both X11/Xvfb and forced
Wayland/headless Weston. Neither validation creates a distributable artifact or satisfies the
future release gate below.

The current native gate also includes a real post-startup `ENOSPC` regression for persistent model
updates, profile deletion, and provider switching. Local evidence proves exact rejection, continued
use of the prior session, and restart recovery of only pre-fault state; its first remote run is
pending. This is not evidence for corruption, power-loss recovery, read-only media, or every
storage-failure path.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. the native Secret Service backend, session-only fallback, complete SecretRef-backed profile lifecycle, multi-profile management, XDG paths, portals, accessibility, Wayland, practical X11 support, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
