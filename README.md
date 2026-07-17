# LinguaMesh for Linux

LinguaMesh for Linux is the native Rust and GTK 4 client for the LinguaMesh translation suite. This repository currently contains only its verified repository foundation. It does not yet contain a Cargo package, application source, tests, Flatpak manifest, or release artifacts.

## Project authority

- [`GLOBAL_GOAL.md`](GLOBAL_GOAL.md) pins the global specification revision.
- [`REPOSITORY_ROLE.md`](REPOSITORY_ROLE.md) defines this repository's ownership boundaries.
- [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) records what is actually implemented and verified.

The authoritative specification lives in the sibling `linguamesh-project` repository. Product work must remain compatible with pinned LinguaMesh Core crates and the central release train.

## Intended native stack

The client will use stable Rust, GTK 4 through gtk-rs, GLib/GIO, and libadwaita where it does not make core behavior depend on GNOME-only services. It will integrate XDG directories and portals, Secret Service with session-only fallback, Wayland, and practical X11 support. These are requirements, not claims of current implementation.

## Current validation

The foundation requires only Git and standard POSIX shell tools:

```sh
cd linguamesh-linux
git status --short --branch
git diff --check
```

The complete documentation check is in [`docs/testing.md`](docs/testing.md) and runs in [`.github/workflows/foundation.yml`](.github/workflows/foundation.yml). Product format, lint, test, and build commands are unavailable until the native project is implemented.

## Documentation

- [Architecture](docs/architecture.md)
- [Testing](docs/testing.md)
- [Releasing](docs/releasing.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

