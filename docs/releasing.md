# Releasing

## Current state

An unreleased native application target now exists, but no Flatpak manifest or distributable
artifact exists. The vertical slice must not be tagged or published as a product release, and no
packaging claim is valid. Its bundled fake provider is development-only behavior. The optional
OpenAI-compatible endpoint form accepts a one-shot session credential, clears the field
immediately, and never persists the credential or profile. It is tested only with LinguaMesh's
loopback fake provider. Persistent intent and persistent secret references fail closed because no
native Secret Service backend exists. The native workflow pins reviewed Core functional revision
`c9a96da52e10554c8458f4d49600ec9336ea651b` rather than checking out a floating branch. The
alpha.2 Xvfb/GTK gate is pending remote execution.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. the native Secret Service backend, session-only fallback, persistent provider profiles, XDG paths, portals, accessibility, Wayland, practical X11 support, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
