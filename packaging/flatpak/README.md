# Flatpak packaging

`dev.linguamesh.LinguaMesh.yml` is the Linux packaging scaffold. It pins the reviewed Core
revision and the Linux source revision, includes the generated Cargo source set, and installs the
binary, desktop entry, AppStream metadata, and icon. The sandbox permissions are intentionally
limited to Wayland, X11 fallback, D-Bus Secret Service and notifications, provider network access,
GPU access, and the application data directory.

Validate metadata without a Flatpak SDK:

```sh
bash tools/validate-flatpak-metadata.sh
```

Build and install locally when `flatpak-builder`, the GNOME 49 SDK, and the Rust SDK extension are
available:

```sh
flatpak-builder --user --install-deps-from=flathub --force-clean \
  build-dir packaging/flatpak/dev.linguamesh.LinguaMesh.yml
flatpak-builder --user --run build-dir packaging/flatpak/dev.linguamesh.LinguaMesh.yml
```

The manifest is still an unreleased scaffold. The `Flatpak Linux` workflow builds a CI-only bundle
with the GNOME 49 SDK and runs a bounded Xvfb/private-D-Bus startup smoke; metadata/build/smoke do
not prove portal lease, notification delivery, or a distributable release artifact. It also emits
SHA-256 and SPDX 2.3 dependency evidence as unreleased CI sidecars. Regenerate
`cargo-sources.json` from the checked-in `Cargo.lock` whenever dependencies change, then update the
Linux source commit in the manifest after packaging files are committed. The metadata validator
requires that commit to equal the current checkout or an ancestor with no build-input changes, so
a Flatpak gate cannot silently build an older application revision.
