# Third-Party Notices

The source depends on the following direct third-party components. No binary artifact, image, font,
provider logo, or generated SDK is distributed from this checkpoint.

- GTK 4 (`gtk4` bindings 0.11.4): gtk-rs bindings under MIT; the dynamically linked system GTK
  library is LGPL-2.1-or-later. It provides native widgets and GLib/GIO integration.
- libadwaita (`libadwaita` bindings 0.9.2): Rust bindings under MIT; the dynamically linked system
  library is LGPL-2.1-or-later. It provides the native application shell and appearance manager.
- Tokio 1.x: MIT, used only for the off-main-thread core worker runtime.
- LinguaMesh Core path crates: MIT first-party source, used for domain events, protocol versioning,
  provider adaptation, streaming, cancellation, and the loopback test provider.
- LinguaMesh localization resources: MIT first-party generated PO catalogs. Non-English catalogs
  remain machine-generated, unreviewed development drafts and are not distributed as approved
  translations.

GitHub Actions uses `actions/checkout`, `dtolnay/rust-toolchain`, and `Swatinem/rust-cache` under
their published upstream licenses. They are CI infrastructure and are not distributed as part of
the application.

Before adding a dependency or distributable asset, record its name, version, source, license,
purpose, modification status, linking mode, and distribution obligations here. Review transitive
dependencies and avoid AGPL, SSPL, non-commercial, source-available, or otherwise incompatible
terms.

GTK, GLib, GIO, libadwaita, and other LGPL system libraries require a documented compliance review before packaging. Notices for pinned LinguaMesh Core crates and localization resources must be incorporated before any application artifact is distributed.
