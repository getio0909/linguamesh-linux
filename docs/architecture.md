# Linux Architecture

## Implemented vertical slice

`src/model.rs` is a toolkit-independent reducer over canonical `linguamesh-domain` types. It starts
in `Disconnected` with no active provider. `ProviderProfile` and `ProviderProfileId` are Core types,
not Linux copies. A connection places a validated profile in pending state; only a matching success
commits that profile and its discovered models. A stale success or failure is rejected without
changing the active state. A failed switch therefore preserves the previous provider, models, and
selection.

Model discovery never selects the first entry implicitly. Startup atomically restores a sorted
vector of saved profiles, the persisted active ID, and each model preference, while the reducer
remains `Disconnected`. The selected form row, persisted default, connected saved row, and pending
deletion are separate state so browsing one profile cannot mutate another. A saved model is
restored only after an explicit reconnect and only when that exact model remains in discovery;
otherwise the user must select a model deliberately. The reducer also enforces ordered translation
events, retains partial output on cancellation or failure, and maps every Core `0.1.0-alpha.2`
error category to safe UI text. Fixed provider/file/worker and reducer-state/category errors are
catalog-backed at the GTK boundary, while dynamic provider diagnostics retain explicit English
fallbacks. Its onboarding stage is derived from the same authoritative state
as `Starting`, `Unavailable`, `Configure provider`, `Connecting`, `Select model`, or `Ready`; no
parallel wizard state or persisted completion flag can race startup, restoration, pending model
confirmation, or rollback.

With `demo-provider`, `src/worker.rs` creates bounded command and event channels on a dedicated
Tokio runtime. It validates the Core contract before doing provider work, then creates Core's
bounded typed host-secret channel and a `linguamesh_application::ProviderManager`. The reviewed Core
functional revision is `8b096475b00fbb4b8f5c88db3d6c7f35d7e046b9`; compared with the prior
alpha.2 pin, it makes file-backed SQLite opens include `SQLITE_OPEN_NOFOLLOW`, adds streamed
protected-span and request-level glossary restoration, and rejects suspicious OOXML compression
ratios and unsupported macro/signature parts before XML inspection. Core now advertises the bounded
`file_lease_v1` lifecycle; Linux validates the lease around portal-backed document reads and revokes it
after the document bytes are copied into the bounded job. Provider adapters also carry
bounded `Retry-After` hints into typed errors; Linux applies cancellation-aware bounded backoff
and an in-memory circuit breaker before trying the next approved candidate. The required contract
is exact Core `0.1.0-alpha.2`, ABI 1, protocol 1, provider catalog `0.1.0`, and these features:

- `cancellation_v1`
- `compatibility_negotiation_v1`
- `file_lease_v1`
- `typed_rust_host_secret_broker_v1`
- `model_discovery_v1`
- `protected_spans_v1`
- `long_text_chunking_v1`
- `bounded_text_document_v1`
- `routing_planner_v1`
- `translation_quality_modes_v1`
- `translation_presets_v1`
- `streaming_text_v1`
- `text_translation_v1`

The worker loads every stored profile and the last activated ID before the development fake service
starts on loopback and emits `DemoProviderReady`, which only supplies an endpoint when no restored
profile exists. Startup does not create an active provider or issue a provider request. Provider
controls remain disabled until the preceding storage result and this readiness event have both
arrived, preventing a startup result from racing an explicit connection. An explicit `Connect`
command creates a candidate `ProviderManager`; only successful secret resolution and model
discovery replace the active manager. Explicit `SelectModel` confirmation is required before
`Translate` unless the restored model preference is discovered again. A rejected selection restores
the GTK dropdown to the last confirmed model.

Connection uses a `CancellationToken`, while translation uses Core's `CancellationHandle`. Both
are reachable outside the bounded command queue, so a full queue cannot prevent a stop request.
Translation commands receive ordered Core events, preserve partial output, and terminate with a
typed terminal result. Provider URL policy, HTTP, SSE parsing, prompts, credential use, and
translation cancellation remain in Core.

The pinned Core also protects common URLs, email addresses, Markdown code, and placeholders before
prompt construction. The adapter restores those spans across split streamed deltas and rejects
missing, duplicate, or changed markers as typed malformed responses; Linux therefore never renders
provider output that structurally drops one of these spans.

The text workspace exposes Core's `Fast`, `Balanced`, and `Best` quality modes as a request-scoped
selector. Fast uses one direct pass, Balanced adds deterministic output checks, and Best asks the
model for an internal critique and revision before final text. Core does not add hidden paid
follow-up calls. Document jobs persist the selected mode in schema 17 and reuse it for each segment
after pause, retry, or restart. Schema 18 persists the selected translation preset with the same
bounded, non-secret validation.

The same workspace exposes localized `General`, `Technical`, and `Marketing` translation presets.
The reducer stores the selected Core `TranslationPreset` and attaches it to ordinary text and
document requests;
compatibility negotiation requires `translation_presets_v1`. Presets are bounded request metadata,
not executable instructions or credentials, and document jobs persist and reuse the selected preset
through schema 18 after pause, retry, or restart.

The Linux GTK form consumes the bundled Core provider catalog for adapter compatibility and model
listing policy before creating a window; a stale mapping fails closed. Its localized labels and
endpoint defaults remain native UI data. The provider catalog's `local-loopback` preset uses the
OpenAI-compatible `/v1/` adapter, while its loopback-only `ollama` preset uses Core's native `/api/` adapter. The Linux form also exposes the
localized Anthropic Messages preset backed by Core's `anthropic_messages` manual-model adapter and
the Google Gemini preset backed by `gemini_generate_content`. Gemini discovers only models that
advertise `generateContent`, streams `/v1beta/` SSE candidates, and sends an optional credential as
`x-goog-api-key`; its deterministic fixture does not represent live account/quota validation.
The Linux worker also runs the same contract through `ProviderManager` and the GTK-facing worker
path, deliberately selecting the discovered `gemini-2.0-flash` model before translation.
Anthropic defaults to HTTPS `/v1/`, requires a non-empty Model ID before Connect, and validates that
ID before resolving any host SecretRef. The worker's deterministic native
fixture covers `/api/tags` model discovery and `/api/chat` NDJSON streaming with explicit model
selection. The GTK provider form now exposes all six presets, restores the selected preset for saved
profiles, and changes untouched default fields when the user switches protocol. The opt-in harness
also passed against an independently running Docker Ollama daemon and an installed Qwen GGUF model;
GPU execution remains outside this prerelease evidence. Azure OpenAI uses Core's
`azure_openai_chat` adapter: the resource endpoint is validated before the session secret is
resolved, the deployment name is supplied manually as the selected model, API version `2024-10-21`
is pinned, and the credential is sent only as the `api-key` header. Azure deployment enumeration is
not attempted; the GTK form exposes all six presets and restores the selected preset for saved
profiles.

OpenAI Responses uses Core's `openai_responses` adapter with the shared `/v1/models` discovery
endpoint and `/v1/responses` translation endpoint. The client sends the credential only through
the Bearer header and consumes typed SSE events, including `response.output_text.delta` and
`response.completed`; metadata events are not treated as translated text.

The Linux text workspace adds an in-memory request-level glossary field. Core validates duplicate
rules and credential-shaped terms, selects only locale-matching entries, protects immutable names,
and restores required target terms after streaming; glossary content is not part of saved provider
profiles or SQLite persistence.

The GTK routing-profile dialog maps a stable dropdown order to Core's `Manual`, `Ordered`, and
`Automatic` modes. A separate **Allow approved fallback** checkbox records explicit consent and is
disabled by default; the worker still applies Core's policy that manual and document dispatches do
not fall through to another candidate. Saved provider/model pairs are exposed as focusable
candidate checkboxes with adjacent up/down controls; the icon controls use catalog-backed accessible
labels, rows can be dragged before another candidate. Manual mode normalizes the saved selection to
the first displayed provider/model pair, while Ordered and Automatic persist the selected chain in
displayed order. Existing records load their mode, explicit fallback consent,
candidate order, and stable ID back into the same editor; saving replaces that ID through the
storage upsert without exposing secrets. New profiles use Core-compatible 1–128 byte ASCII IDs;
edit mode locks the existing ID so references remain stable, and new profiles reject duplicate IDs
before reaching the upsert path.

The same dialog imports and exports a bounded JSON exchange format. Core serializes only validated
profile fields (candidate capabilities and privacy constraints); endpoint, credential, user-content,
unknown-field, malformed, non-UTF-8, and over-64-KiB payloads are rejected. Imports never overwrite
an existing ID, so replacing a profile remains an explicit in-app edit.

Core also performs bounded long-text chunking before provider calls. It prefers paragraph, sentence,
and whitespace boundaries, treats protected markers as indivisible, streams chunks in source order,
and stops before starting another chunk when cancellation is requested. The 16 KiB default is a
conservative byte estimate, not a tokenizer-derived model capacity claim.

The request reducer carries `TranslationPrivacyMode` explicitly. The GTK Incognito mode toggle
maps to `TranslationRequest::privacy_mode = Incognito`, and Core's serde default keeps older request
payloads equivalent to `Standard`. Completed standard translations are persisted through Core's
bounded SQLite history migration (100 entries, 4 MiB source/output limit); startup restores the count
and **Clear history** deletes all entries. Incognito completion skips the history write. **View
history** reads a bounded newest-first snapshot, supports exact per-entry deletion, and exports the
displayed snapshot as escaped UTF-8 TSV. The persisted **Save translation history** policy keeps
existing rows when disabled and blocks future standard completion writes until re-enabled. The
persisted **Save translation memory** policy independently gates a bounded Core schema-5 cache whose
identity includes normalized source, locales, provider/model, chunking options, glossary,
protected-span policy, prompt-template version, and quality mode. Incognito bypasses lookup and
write; **View translation memory** supports inspection, escaped TSV export, exact deletion, and
clear-all.

The native text-file path delegates TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF format detection and bounded UTF-8/BOM handling to
Core's `bounded_text_document_v1` contract. It preserves the original LF/CRLF/CR line endings and
classifies Markdown fenced code and blank structure as verbatim segments before the editor receives
the source text. Core schema 18 and the worker persist bounded pending/running/paused job snapshots
and segment progress without source paths or credentials. Schema 8 also stores validated non-secret
source/target locales, provider/model IDs, and optional glossary rules. Imported TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF files
become these snapshots before translation; the worker translates pending prose segments sequentially,
writes each completed segment through the confirmed provider or selected saved document-capable
routing candidate, emits a typed non-sensitive routing decision, and routes safe reconstruction back
to the editor. Document jobs keep fallback disabled even when a routing profile permits explicit
fallback. Resume and Retry reuse saved options, including quality mode and translation preset, only after the active runtime matches; routed jobs
also persist the non-secret routing-profile ID and reconnect that profile before selecting the next
candidate after restart.
DOCX/PPTX/XLSX/EPUB package resources remain intact while
supported text parts are rewritten under the same 4 MiB package and 512-entry limits. PDF extraction
keeps page association, coordinates where available, and uncertain reading-order boundaries; ASCII
text streams are rewritten when safe and other text encodings use a structured page-aware HTML
alternative. Core also exposes configurable subtitle line-length and reading-speed warnings keyed by
cue number; the Linux UI renders only those numbers and fixed guidance text. EPUB preserves
metadata, navigation, CSS, and binary resources while rewriting visible XHTML/HTML text and updating
OPF language metadata at export. Encrypted, malformed, traversal, and DTD-bearing packages are rejected.
The GTK surface presents multiple persisted jobs for explicit selection. The worker runs up to four
document jobs concurrently, with per-job bounded event delivery, cancellation, pause state, and
segment persistence isolated by job ID. A fifth start, or a duplicate start for an active job, is
rejected with a typed configuration error without changing the persisted job snapshot. The UI still
uses explicit selection for queue actions, and the broader release gate remains prerelease.

Image-only PDF pages are a separate, explicit opt-in path. The GTK toggle is only used when Core
reports a PDF with no extractable text. The worker then invokes `pdftoppm` and `tesseract` through
`src/ocr.rs` without a shell, with safe language identifiers, private `0700` temporary storage,
`0600` input files, bounded PDF/page/image/text/output sizes, and a 30-second process deadline.
The plugin returns page-numbered text that Linux stores as a normal TXT document job; it never
modifies the source PDF. Missing tools, malformed input, empty recognition, timeout, and limit
breaches become fixed localized errors, and the default path remains unchanged when OCR is off.

With `gui`, `src/main.rs` binds this state and worker to GTK 4/libadwaita widgets. GTK objects remain
on the main context, which processes at most 64 queued events per timer tick without performing
network work. The shell exposes a saved-profile dropdown, provider name, endpoint, optional session
credential, explicit Connect, **Remember profile, model, and credential in Secret Service**,
**Remove saved profile**,
model selection, source and target locales, source and streamed output editors, native **Open text
file** import, single-file drag-and-drop onto the source editor, Translate/Stop,
typed errors, appearance, runtime catalog-backed locale preference, **View history**, **Clear history**,
**View translation memory**, **Clear translation memory**, and redacted
diagnostics.
The native file action also exposes an explicit **OCR** toggle for image-only PDFs; while OCR is
running, conflicting import and translation controls are disabled and the status is localized.
An always-current Provider setup card explains the next required action, warns when saved-profile
storage is unavailable, distinguishes fatal worker shutdown from startup, and identifies the
confirmed provider stable ID/model that will receive the next request. It never connects, selects,
or persists anything itself. A stopped worker or disconnected event channel marks the worker
unavailable and disables provider, model, translation, and cancellation commands.
Selecting a restored profile prefills only its non-secret form fields without connecting or
changing the active runtime model. New persistent profiles use a GLib random UUID validated as a
Core `ProviderProfileId`; display names are never database keys. Pending connection, model
selection, translation, or deletion disables conflicting controls. Text import uses GTK's native
`FileDialog` and GIO's bounded partial asynchronous read; only UTF-8 TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT content up to
4 MiB is accepted, and file paths/content are excluded from diagnostics.

The GTK boundary also owns baseline accessibility semantics. The workspace uses the `Main` role;
onboarding and provider headings use `Heading`; the live operation label uses `Status`; and the
error label uses `Alert` while remaining hidden when there is no error. Source and output
`TextView`s use `TextBox` with explicit accessible names and multi-line properties, with output
marked read-only and both editors kept focusable. Visible labels establish `LabelledBy` relations
and mnemonic targets for every entry and dropdown, including the editor labels. The output region
reports `Busy` during translation and resets it at a terminal state, while the Stop button uses
the explicit accessible name `Stop translation`.

Native CI executes this same GTK binary flow first through serialized X11/Xvfb and then through a
separate private D-Bus session with `GDK_BACKEND=wayland`. The Wayland runner starts headless Weston
inside a private `0700` `XDG_RUNTIME_DIR`, waits a bounded time for its dedicated socket, removes
`DISPLAY` to prevent X11 fallback, and always terminates the compositor and removes the runtime
directory. The X11 path also runs `tools/run-gtk-keyboard-focus-test.sh` under `xfwm4` to exercise
Tab/Shift+Tab traversal for the tested controls; an application-window Capture-phase handler keeps
the provider fields in an explicit order while preserving modified shortcuts. The
`tools/run-gtk-atspi-test.sh` fixture separately reads the live tree through `python3-pyatspi` and
checks the named Stop control plus both text-editor roles. Native CI also runs
`tools/run-orca-atspi-test.sh`, which starts Orca with Speech Dispatcher, inspects that named control
through AT-SPI, and checks Orca's speech-generator debug record for the Linux application tree. These remain headless
protocol/backend and Orca-integration gates, not claims about physical compositors, GPU rendering,
desktop integration, physical desktop keyboard coverage, human listening, or speech quality.

The user-facing endpoint example is loopback. Under its shared endpoint policy, Core accepts
loopback HTTP and also accepts HTTPS endpoints; the Linux client does not duplicate URL parsing.
Automated client evidence covers the built-in provider and a deterministic Ollama-compatible
OpenAI `/v1/` fixture on loopback. `tools/run-ollama-interop-test.sh` provides an explicit
real-daemon path that starts an isolated local daemon when needed and runs the worker regression
against a caller-selected installed model; the test remains opt-in because model installation is
an external prerequisite. The Linux checkpoint passed this path against Docker
`ollama/ollama:0.11.10` with `qwen2.5-0.5b-instruct:latest`.

## Secret lifecycle and persistence boundary

The optional credential field implements session use only. On Connect, its value is copied into
Core's secret-aware `SecretValue`, the widget is cleared immediately, and the temporary GTK string
is dropped; the GTK buffer is not claimed to be zeroized. A random `session:` `SecretRef`
identifies the broker request without containing the credential. The worker may satisfy the
matching request once; a missing or mismatched session secret returns `SecretUnavailable`. Core
retains the `SecretValue` only for the active provider session and clears it when that manager is
disconnected or replaced.

When the user selects the remember checkbox, the worker uses Core's `linguamesh-storage` crate to
create or update that credential-free profile and atomically make it the persisted default only
after connection and model discovery succeed. The saved user-configured fields are the provider
name, endpoint, and validated model preference. Multiple rows may share a name or endpoint because
their random stable IDs remain distinct. Selecting a new model updates only the connected saved row
before the in-memory confirmation is emitted, even when another row is displayed in the form. A
failed candidate or cancellation observed before the persistence commit leaves the active manager,
every saved row, and the restart default unchanged; a session-only switch does the same.

Profile deletion is a separate exact-ID command. Core commits its transactional row deletion first,
then the worker emits success. Removing a non-default row leaves the persisted default unchanged.
Removing the connected or persisted-default row clears its persisted/default marker after commit;
an already validated matching runtime engine and selected model remain alive as session-only, and
later model changes do not recreate the row. A missing row, storage failure, active translation,
shutdown, or stale result preserves the reducer snapshot and produces a typed rejection.

Any `Persistence` error returned by an already-open `Storage` during persistent connection, model
selection, or deletion degrades that worker instance to session-only mode. The operation-specific
rejection is emitted before `ProfileStorageUnavailable`; the worker also drops the storage handle
and active saved-profile marker before accepting more work. It does not replace the active manager,
profile, or confirmed model. The reducer clears its cached saved-profile/default mirror while
preserving the validated runtime session, and a restart can therefore expose only state committed
before the fault.

The database is
`$XDG_DATA_HOME/dev.linguamesh.LinguaMesh/linguamesh.sqlite3`, using GLib's resolved user data
directory. The Linux worker creates a private `0700` parent directory or accepts an existing more
restrictive parent with no group or other access. It requires a regular, single-link `0600`
database file, rejecting relative paths, symbolic links, hard links, and non-private directories.
Core is responsible for the schema,
migrations, transactions, and SQLite open flags. On Linux's default Unix VFS, Core's
`SQLITE_OPEN_NOFOLLOW` open rejects any symbolic-link component in the resolved path; such a path
produces a typed storage failure and leaves explicit session-only connections available. This
behavior is a Linux security prerequisite for the integration and is not claimed for other VFS
implementations.

Before creating or opening the database, the host checks every existing path component for a
symbolic link and prevalidates an existing leaf as a regular single-link file. It then opens the
parent with Linux `openat2(RESOLVE_NO_SYMLINKS)`, opens the final component with
`O_NOFOLLOW | O_CLOEXEC`, and keeps that file descriptor alive while Core opens the exact
`/proc/self/fd/<fd>` path. Core's ordinary path open remains the no-follow SQLite gate; the
descriptor-backed API rejects paths outside that exact form. This prevents a concurrent parent
path replacement from redirecting the migration/open operation, while broader filesystem and VFS
guarantees remain platform-specific.

Neither credential values nor session references are persisted. A runtime session reference is
stripped before the profile reaches SQLite. When the user chooses Remember with a credential, the
Linux GIO Secret Service adapter stores the value and SQLite retains only its persistent
`secret-service:` reference. Restored profiles resolve that reference on explicit Connect; missing,
locked, unavailable, or interactive-only keyring states fail closed. There is no plaintext fallback,
startup does not auto-connect, and the UI keeps an explicit session-only path.

Native CI exercises this onboarding boundary with an authenticated loopback provider: the GTK form
clears the credential immediately, persists only the SecretRef, reconnects after worker restart, and
checks that the credential canary is absent from SQLite. A separate prompt fixture returns non-root
prompt paths for store and delete and verifies the adapter fails closed; user approval and unlock UI
remain outside the automated boundary.

`l10n/linux/` is a byte-for-byte consumer snapshot of the canonical PO/MO catalogs at the revision
enforced by `tools/sync-l10n.sh`. The GTK host parses all twelve official MO catalogs at runtime,
switches translated action, workspace-widget, active-provider, status summary/partial-output, text-file import, provider-profile, source/target language, editor text-metrics, routing preference/privacy/document constraints, provider/model allowlists and denylists, and quality/request-size limits plus onboarding stage/detail controls and System/Light/Dark theme
labels without replacing active source text, applies RTL root direction for Arabic, and maps stable
worker startup, Core compatibility, and profile-storage error sentences through the same catalog.
Provider-specific diagnostic detail remains an explicit English fallback.

The Linux source also runs two dependency-free localization audits. The key audit verifies every
literal catalog key and its supplemental dynamic-key table against the canonical catalog. The
visible-control audit scans GTK labels, titles, tooltips, placeholders, dialog actions, and list
options and rejects non-empty literals that bypass the localization helper. Empty reset labels are
allowed because they remove transient text rather than present user-facing copy.

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
integration, and credential resolution. This slice includes the XDG profile-database path,
runtime action-label catalogs, a real document-portal lease lifecycle fixture, a real interactive
`xdg-desktop-portal-gtk` FileChooser backend fixture, and a completion notification through
`GApplication`; the notification contains localized generic copy and never source or translated
content. Native CI also delivers that payload to a real `dunst` notification daemon under Xvfb,
asserts a visible viewable Dunst desktop-shell window, verifies the asynchronous GTK FileDialog
callback, and performs a real URI-list drag/drop through the source editor. Physical compositor/GPU
rendering and end-user prompted approval remain validation boundaries.

## Security and portability boundaries

Persistent secrets use Secret Service. When that service is unavailable, the UI offers only the
clearly labeled in-memory credential path and fails closed for persistent references; remembering
profile fields does not weaken that boundary. File and directory handling
must follow XDG locations, restrictive permissions, portal leases, and cleanup rules. Wayland is
required; the headless Wayland gate and practical X11/Xvfb gate cover the current real-widget slice,
while physical compositor and broader desktop coverage remain incomplete.

Changes affecting shared contracts, the security model, display support, GTK/libadwaita policy, or distribution packaging require central compatibility review. GTK and other LGPL dependencies require documented license compliance before distribution.
