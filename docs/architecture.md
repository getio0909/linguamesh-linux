# Linux Architecture

## Implemented vertical slice

`src/model.rs` is a toolkit-independent reducer over canonical `linguamesh-domain` types. It starts
in `Disconnected` with no active provider. `ProviderProfile` and `ProviderProfileId` are Core types,
not Linux copies. A connection places a validated profile in pending state; only a matching success
commits that profile and its discovered models. A stale success or failure is rejected without
changing the active state. A failed switch therefore preserves the previous provider, models, and
selection.

Model discovery never selects the first entry implicitly. A saved `ProviderProfile.selected_model`
is restored only when that exact model remains in the discovery result; otherwise the user must
select a model deliberately. The reducer also enforces ordered translation events, retains partial
output on cancellation or failure, and maps every Core `0.1.0-alpha.2` error category to safe UI
text.

With `demo-provider`, `src/worker.rs` creates bounded command and event channels on a dedicated
Tokio runtime. It validates the Core contract before doing provider work, then creates Core's
bounded typed host-secret channel and a `linguamesh_application::ProviderManager`. The required
contract is exact Core `0.1.0-alpha.2`, ABI 1, protocol 1, provider catalog `0.1.0`, and these
features:

- `cancellation_v1`
- `compatibility_negotiation_v1`
- `typed_rust_host_secret_broker_v1`
- `model_discovery_v1`
- `streaming_text_v1`
- `text_translation_v1`

The development fake service starts on loopback and emits `DemoProviderReady`, which only supplies
an endpoint. It does not create an active provider. An explicit `Connect` command creates a
candidate `ProviderManager`; only successful secret resolution and model discovery replace the
active manager. Explicit `SelectModel` confirmation is required before `Translate`.

Connection uses a `CancellationToken`, while translation uses Core's `CancellationHandle`. Both
are reachable outside the bounded command queue, so a full queue cannot prevent a stop request.
Translation commands receive ordered Core events, preserve partial output, and terminate with a
typed terminal result. Provider URL policy, HTTP, SSE parsing, prompts, credential use, and
translation cancellation remain in Core.

With `gui`, `src/main.rs` binds this state and worker to GTK 4/libadwaita widgets. GTK objects remain
on the main context, which processes at most 64 queued events per timer tick without performing
network work. The shell exposes provider name, endpoint, optional session credential, explicit
Connect, explicit model selection, source and target locales, source and streamed output editors,
Translate/Stop, typed errors, appearance, locale preference with an English fallback, and redacted
diagnostics. Connection and translation states disable conflicting controls.

The user-facing endpoint example is loopback. Under its shared endpoint policy, Core accepts
loopback HTTP and also accepts HTTPS endpoints; the Linux client does not duplicate URL parsing.
Automated client evidence covers the built-in provider and an external LinguaMesh fake provider on
loopback, not Ollama or another third-party server.

## Secret lifecycle and persistence boundary

The optional credential field implements session use only. On Connect, its value is copied into
Core's secret-aware `SecretValue`, the widget is cleared immediately, and the temporary GTK string
is dropped; the GTK buffer is not claimed to be zeroized. A random `session:` `SecretRef`
identifies the broker request without containing the credential. The worker may satisfy the
matching request once; a missing or mismatched session secret returns `SecretUnavailable`. Core
retains the `SecretValue` only for the active provider session and clears it when that manager is
disconnected or replaced.

No provider profile, endpoint, secret reference, or credential is persisted by this slice. The
worker rejects `PersistenceIntent::Persistent`. A persistent `SecretRef`, including a
`secret-service:` reference, returns `SecureStorageUnavailable` because the native Secret Service
backend is not implemented. There is no plaintext fallback and the UI does not claim that the
profile or credential was saved.

`l10n/linux/` is a byte-for-byte consumer snapshot of the canonical PO catalogs at the revision
enforced by `tools/sync-l10n.sh`. Resource provenance and format validation are implemented, while
runtime gettext lookup remains a separate incomplete client concern.

The application state and worker command/event wrappers intentionally do not derive `Debug`, so
source text and streamed output are not exposed through routine debug formatting. Diagnostics omit
the provider endpoint, secret reference, selected model identifier, source text, and output content.

## Required boundaries

The client uses stable Rust, GTK 4 through gtk-rs, GLib/GIO, and libadwaita for presentation. GTK
objects remain on the main context; network, database, document, tokenization, and core work must
not block it. UI state changes cross bounded, cancellable channels and preserve event ordering.

The client calls the public Rust application layer directly while preserving the same observable
event behavior as FFI clients. It must not fork provider, routing, translation, document,
persistence, or error-domain logic. This checkpoint implements the exact compatibility gate above;
release-manifest integration and broader product compatibility remain required before release.

The client owns native lifecycle, accessibility, appearance, keyboard behavior, file dialogs,
portals, drag-and-drop, clipboard, notifications, XDG paths, desktop metadata, display-server
integration, and credential resolution. Only lifecycle, basic keyboard mnemonics, appearance, and
the text workspace are present in this slice.

## Security and portability boundaries

Persistent secrets must use Secret Service. Until that backend exists, the UI offers only the
clearly labeled in-memory session path and fails closed for persistence. File and directory
handling must follow XDG locations, restrictive permissions, portal leases, and cleanup rules.
Wayland is required; X11 support is practical where dependencies and tests allow it.

Changes affecting shared contracts, the security model, display support, GTK/libadwaita policy, or distribution packaging require central compatibility review. GTK and other LGPL dependencies require documented license compliance before distribution.
