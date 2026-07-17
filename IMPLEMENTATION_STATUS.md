# Implementation Status

Status: Native text-translation development slice and compatibility pins implemented; native GUI validation pending

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO resources are synchronized and format-validated, but selecting
`zh-CN` continues to present an explicit English fallback until the GTK gettext adapter is
implemented and tested.

## Implemented

- Rust 1.93.0 Cargo package with locked dependencies and optional `demo-provider`/`gui` features.
- Toolkit-independent state for provider/model and language selection, source and streamed output,
  strictly ordered typed events, repeated-start rejection, cancellation, partial output, safe
  errors, theme, locale, and redacted diagnostics.
- Bounded background worker using direct sibling `linguamesh-core` types. It runs the shared
  loopback fake provider, model discovery, real HTTP/SSE translation, and Core cancellation away
  from the GTK main context.
- Cancellation uses a cross-thread Core handle so command-queue backpressure cannot prevent the
  request. Control commands receive priority, and cancellation racing with completion is idempotent.
- GTK 4/libadwaita application source with native selectors, source/output text views,
  translate/stop actions, typed error and partial-result display, appearance switching, locale
  fallback notice, keyboard mnemonics, and redacted diagnostics. Event processing is capped per
  main-context tick, and model updates avoid `RefCell` reentrancy across GTK notifications.
- Fourteen canonical official/pseudo PO catalogs pinned to l10n revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `873b6da45447f73e4be4e2f1127c3c8d0f188cf2` and localization revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`.

## Local validation evidence

Validated on 2026-07-17 with Rust 1.93.0:

- The pinned global-goal SHA-256 matched the sibling authoritative file.
- The sibling Core checkout was clean at pinned revision
  `873b6da45447f73e4be4e2f1127c3c8d0f188cf2`.
- `cargo fmt --all --check` passed.
- `cargo check --all-targets --features demo-provider --locked` passed.
- `cargo clippy --all-targets --features demo-provider --locked -- -D warnings` passed.
- `cargo test --no-default-features --locked` passed: 9 tests, 0 failed.
- `cargo test --features demo-provider --locked` passed: 12 tests, 0 failed. The worker tests used
  real loopback streaming, active cancellation, and cancellation-after-terminal behavior.
- `cargo build --features demo-provider --locked` passed.
- `DOCS_RS=1 cargo check --all-targets --all-features --locked` and the equivalent strict Clippy
  command passed only as source-level diagnostics of the GTK code.
- The exact foundation block in `docs/testing.md`, `git diff --check`, shell syntax validation, and
  code-comment/secret-pattern scans passed.
- `bash tools/sync-l10n.sh --check` passed against the exact clean l10n checkout, and all 14 PO
  catalogs passed `msgfmt --check --check-format`.
- The checkout, Rust-toolchain, and Rust-cache action SHAs resolved through the GitHub commits API;
  their action metadata uses Node 24 or a composite action.

## Not locally validated

No GitHub Actions run was triggered from this worktree, so the pinned native workflow is configured
but has no claimed remote result.

This host has no `gtk4.pc`, `libadwaita-1.pc`, or `graphene-gobject-1.0.pc`. After clearing the
source-only `graphene-sys` cache, a normal `cargo check --all-targets --all-features --locked`
correctly stopped at missing `graphene-gobject-1.0`. No local GUI link, launch, screenshot,
display-server, accessibility, or CI result is claimed.

## Remaining scope

- Reproducible native CI execution and full semantic/catalog/feature compatibility.
- Secure provider profiles, Secret Service and explicit session-only secrets, onboarding, custom
  endpoints, active multi-provider switching, and persistent settings.
- Runtime gettext lookup, complete canonical UI coverage, and visual locale/RTL verification.
- XDG/portal integration, file workflows, clipboard/drag-and-drop/notifications, comprehensive
  accessibility, Wayland/X11 smoke tests, packaging, Flatpak, and release artifacts.
