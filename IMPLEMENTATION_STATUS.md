# Implementation Status

Status: Session-only local-provider slice verified without native headers; revised GTK/Xvfb CI pending

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO resources are synchronized and format-validated, but selecting
`zh-CN` continues to present an explicit English fallback until the GTK gettext adapter is
implemented and tested.

## Implemented

- Rust 1.93.0 Cargo package with locked dependencies and optional `demo-provider`/`gui` features.
- Toolkit-independent state for active and pending provider/model selection, language selection,
  source and streamed output, strictly ordered typed events, repeated-start rejection,
  cancellation, partial output, safe errors, theme, locale, and redacted diagnostics.
- Credential-free provider profiles exist only in memory for the current process. A connection
  commits its provider and discovered models atomically after success; failure preserves the last
  usable provider and model, and an active translation rejects provider changes.
- Bounded background worker using direct sibling `linguamesh-core` types. It runs the shared
  built-in loopback fake provider or a user-supplied OpenAI-compatible endpoint, model discovery,
  real HTTP/SSE translation, and Core cancellation away from the GTK main context. The user path
  supplies no credential and has automated coverage against an external loopback fake provider.
- Cancellation uses a cross-thread Core handle so command-queue backpressure cannot prevent the
  request. Control commands receive priority, and cancellation racing with completion is idempotent.
- GTK 4/libadwaita application source with a session-only provider name/endpoint form, active
  provider status, native selectors, source/output text views, translate/stop actions, typed error
  and partial-result display, appearance switching, locale fallback notice, keyboard mnemonics,
  and redacted diagnostics. Provider/model/language controls are blocked during connection or an
  active operation. Event processing is capped per main-context tick, and model updates avoid
  `RefCell` reentrancy across GTK notifications.
- Fourteen canonical official/pseudo PO catalogs pinned to l10n revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `873b6da45447f73e4be4e2f1127c3c8d0f188cf2` and localization revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. The revised native gate installs D-Bus/Xvfb support
  and runs serialized all-target, all-feature tests under X11 before building the application.

## Local validation evidence

Validated on 2026-07-17 with Rust 1.93.0:

- The pinned global-goal SHA-256 matched the sibling authoritative file.
- The sibling Core checkout was clean at pinned revision
  `873b6da45447f73e4be4e2f1127c3c8d0f188cf2`.
- `cargo fmt --all --check` passed.
- `cargo check --all-targets --features demo-provider --locked` passed.
- `cargo clippy --all-targets --features demo-provider --locked -- -D warnings` passed.
- `cargo test --no-default-features --locked` passed: 16 tests, 0 failed.
- `cargo test --features demo-provider --locked` passed: 23 tests, 0 failed. The tests used
  real loopback streaming, active cancellation, cancellation-after-terminal behavior, successful
  external loopback discovery and translation, failed-connection rollback, and active-operation
  connection rejection. A state regression also verifies that an event stream ending without a
  terminal event becomes a failed operation while retaining partial output.
- `cargo build --features demo-provider --locked` passed.
- `DOCS_RS=1 cargo check --all-targets --all-features --locked` and the equivalent strict Clippy
  command passed only as source-level diagnostics of the GTK code.
- The exact foundation block in `docs/testing.md`, `git diff --check`, shell syntax validation, and
  code-comment/secret-pattern scans passed.
- `bash tools/sync-l10n.sh --check` passed against the exact clean l10n checkout, and all 14 PO
  catalogs passed `msgfmt --check --check-format`.
- The checkout, Rust-toolchain, and Rust-cache action SHAs resolved through the GitHub commits API;
  their action metadata uses Node 24 or a composite action.

## Remote validation evidence

GitHub revision `10977931ceb11bc9d4b86ec49d7fd710e3c1a063` passed the repository-foundation
workflow run `29557845248` and Native Linux run `29557845223`. Native job `87813768615` installed
GTK 4 and libadwaita development headers on Ubuntu 24.04, validated the exact Core and localization
pins, and passed formatting, strict all-feature Clippy, all-feature tests, and the full native
application build.

That run predates the session-provider UI test and Xvfb gate. A GitHub Actions result for the
current change is pending; no native success for this revision is claimed here.

## Not locally validated

This host has no `gtk4.pc`, `libadwaita-1.pc`, or `graphene-gobject-1.0.pc`. After clearing the
source-only `graphene-sys` cache, a normal `cargo check --all-targets --all-features --locked`
correctly stopped at missing `graphene-gobject-1.0`. No local GUI link, launch, screenshot,
display-server, accessibility, or GTK button-test result is claimed. With the GTK binary test
present, `DOCS_RS=1 cargo test --all-targets --all-features --locked --no-run` reaches native
linking and failed on unavailable GTK symbols; it is not a valid header-free substitute.

## Remaining scope

- Execution of the revised Xvfb/GTK workflow for this checkpoint and full
  semantic/catalog/feature compatibility.
- Secure provider profiles, Secret Service, credential handling, onboarding, persistent provider
  settings, and active switching among saved profiles. The current credential-free form is
  process-local and is not a secure profile implementation.
- Interoperability evidence for third-party local servers, including Ollama; automated endpoint
  coverage currently uses only LinguaMesh fake providers on loopback.
- Provider discovery currently runs inline on the worker and cannot be cancelled from the UI;
  shutdown and other commands wait for Core's bounded 30-second provider request timeout.
- Runtime gettext lookup, complete canonical UI coverage, and visual locale/RTL verification.
- XDG/portal integration, file workflows, clipboard/drag-and-drop/notifications, comprehensive
  accessibility, Wayland/X11 smoke tests, packaging, Flatpak, and release artifacts.
