# Releasing

## Current state

An unreleased native application target now exists, but no Flatpak manifest or distributable
artifact exists. The vertical slice must not be tagged or published as a product release, and no
packaging claim is valid. Its bundled fake provider is development-only behavior. The optional
OpenAI-compatible endpoint form accepts a one-shot session credential, clears the field
immediately, and never persists the credential or its reference. An explicit remember checkbox can
store only the provider name, endpoint, and model preference in the XDG user data SQLite database;
the private application directory is `0700`, the database is `0600`, and Core's no-follow SQLite
open behavior on Linux's default Unix VFS remains required. Startup prefills that saved profile but
never auto-connects, so a credential must be entered again when required. The external-provider
path is tested only with LinguaMesh's loopback fake provider. Persistent secret references fail
closed because no native Secret Service backend exists. The native workflow pins reviewed Core
functional revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, whose storage delta adds
`SQLITE_OPEN_NOFOLLOW`, rather than checking out a floating branch. Functional revision
`c58a54c2479045773358bd9c456b45a958e98e1e` passed Native Linux run `29574265570`; this validation
does not create a distributable artifact or satisfy the future release gate below.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. the native Secret Service backend, session-only fallback, complete SecretRef-backed profile lifecycle, multi-profile management, XDG paths, portals, accessibility, Wayland, practical X11 support, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
