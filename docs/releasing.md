# Releasing

## Current state

An unreleased native application target now exists, but no Flatpak manifest or distributable
artifact exists. The vertical slice must not be tagged or published as a product release, and no
packaging claim is valid. Its bundled fake provider is development-only behavior. The native
workflow pins reviewed Core revision `873b6da45447f73e4be4e2f1127c3c8d0f188cf2` rather than
checking out a floating branch.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. Secret Service/session-only behavior, XDG paths, portals, accessibility, Wayland, practical X11 support, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
