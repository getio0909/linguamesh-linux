# LinguaMesh for Linux

LinguaMesh for Linux is the native Rust, GTK 4, and libadwaita client for the LinguaMesh
translation suite. The current Core `0.1.0-alpha.2` vertical slice starts disconnected, connects
only after an explicit user action, requires a deliberate model choice for a new profile, streams
translated text, and supports cancellation with partial-output retention. It can explicitly
remember multiple non-secret provider profiles, switch or update them through the same explicit
connection action, and revalidate each model preference after reconnect while keeping credentials
session-only. A derived provider-setup guide moves from startup through configuration, connection,
and model selection, reports an unavailable worker without remaining stuck at startup, then
identifies the provider stable ID/model that will receive the next request. Saved
copies can be removed without interrupting an already connected session. The client also displays
typed errors, switches appearance, records locale preference, exposes redacted diagnostics, and
shows the selected saved profile's last provider health check as a localized UTC timestamp or
normalized failure category without exposing provider error text or credentials.

## Project authority

- [`GLOBAL_GOAL.md`](GLOBAL_GOAL.md) pins the global specification revision.
- [`REPOSITORY_ROLE.md`](REPOSITORY_ROLE.md) defines this repository's ownership boundaries.
- [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) records what is actually implemented and verified.

The authoritative specification lives in the sibling `linguamesh-project` repository. Product
work must remain compatible with LinguaMesh Core and the central release train. Native CI pins the
reviewed Core revision `b29067b78d420c96f57d670d3dd860cba3abc703`, a fuzz/docs-only descendant of
the runtime baseline `8623b2c8829e4d9cf7299c74440dcfabb4e320db`; it retains typed provider rate-limit handling while preserving persisted provider health status and bounded
FileLease lifecycle validation and strict
routing-profile validation, schema-15 routing-profile persistence, schema-16 document-job
routing-profile persistence, and schema-17 document quality-mode persistence on top of the existing
document and provider contract, including schema-18 document translation-preset persistence.
Earlier reviewed revisions added
`SQLITE_OPEN_NOFOLLOW` to file-backed storage, protected-span and request-level glossary
restoration, bounded semantic chunking for long streamed text, bounded translation history, and
optional translation-memory storage with versioned request identity, and the bounded TXT/Markdown/
SRT/WebVTT/CSV/JSON/HTML document contract with preserved line endings, verbatim Markdown fences, and validated
subtitle timing, bounded DOCX/PPTX/XLSX/EPUB package reconstruction, bounded text-PDF page extraction and reconstruction with structured HTML fallback, plus schema-17 document job snapshots that survive worker restart without persisting source paths or credentials, plus
validated non-secret provider/model/glossary options reused by Resume and Retry after restart. A
selected saved routing profile can dispatch document segments through a document-capable candidate;
document jobs keep fallback disabled by policy, and routed jobs reconnect the saved profile after
restart instead of silently reverting to the active provider.
The same Core document boundary rejects suspicious OOXML compression ratios before XML inspection
and rejects unsupported OOXML macro and digital-signature parts before import or reconstruction.
Retryable provider failures carry a bounded `Retry-After` hint when available; Linux applies the
Core `RetryPolicy` contract with an eight-second maximum backoff, stable jitter, cancellation-aware
waits, and an in-memory two-failure circuit breaker that cools down for thirty seconds before
retrying a candidate.

The GTK provider form consumes the pinned Core provider catalog for adapter and model-listing
compatibility. It supports optional bounded, non-secret profile notes, custom request headers,
organization/project IDs, region, account identifiers, and an optional HTTP/HTTPS/SOCKS5 proxy URL.
Proxy authentication is entered separately as a bounded `username:password` value, kept in the
session or Secret Service by explicit user choice, and never embedded in the proxy URL.
The proxy is validated without embedded credentials and applied by Core's transport. A bounded total
provider request timeout (1–600 seconds, default 30), connection-establishment timeout (1–120
seconds, default 10), and streaming idle timeout (1–300 seconds, default 60) are saved with the
profile and applied independently by Core's transport. The streaming idle budget resets after each
received response chunk. An optional bounded PEM trust bundle augments system roots while TLS
verification remains enabled; private keys and malformed bundles are rejected. An optional
combined PEM client certificate/private-key identity is entered through a masked field, resolved
through a persistent or session SecretRef, and applied by Core rustls without disabling
verification. The identity is cleared from the form immediately and never stored in SQLite.
Saved profiles restore these values;
custom headers reject authorization, credential-shaped, and built-in metadata names, while the
application layer forwards safe headers to Chat Completions, Responses, and Azure Chat without
replacing their authentication metadata, while OpenAI organization/project headers remain limited
to Chat Completions and Responses. A catalog drift fails closed before the
window is created; localized labels and endpoint defaults remain Linux-native UI data.

## Native stack

The client uses stable Rust, GTK 4.10 or newer through gtk-rs, GLib/GIO, and libadwaita. Shared provider,
streaming, cancellation, compatibility, and secret-broker behavior comes through Core's public
Rust application layer. The Linux layer owns native state reduction, host-secret responses,
background scheduling, and widgets.

The current GTK surface includes baseline accessibility semantics: the main workspace, headings,
status, and errors expose explicit roles; source/output editors expose names, multi-line and
read-only properties; editable fields and dropdowns are labelled by visible mnemonic labels; the
output region reports translation busy state; document-job progress exposes a native progress-bar
role with a bounded completed/total fraction; empty errors are hidden from the accessibility tree;
and Stop has the explicit accessible name “Stop translation”. A CI fixture reads the running
application through AT-SPI and verifies the named Stop control plus both text-editor roles. Native
CI also starts Orca with Speech Dispatcher in the isolated X11 session, inspects that named control
through AT-SPI, and requires Orca's speech-generator debug record for the Linux application tree. This remains headless Orca
integration evidence, not a human listening, physical-keyboard, high-contrast, or physical-compositor
review. The
non-sensitive Diagnostics panel localizes its Core ABI/protocol summary, fixed field labels, state
values, and profile-storage status through the runtime catalog while omitting source text, endpoints,
and secret references.

The dependency-free `tools/check-localization-keys.py` audit verifies literal catalog keys,
`tools/check-localization-placeholders.py` checks every literal fallback template against the
canonical English template and placeholder contract, and `tools/check-visible-localization.py`
rejects non-empty hard-coded GTK labels, titles, tooltips,
placeholders, dialog actions, file-filter names, and list options across every Rust source file
under `src/`. Native and Foundation CI run all three audits before building the client; empty label
assignments used to clear transient UI state remain allowed. The audit is source-level evidence and
does not replace human translated-copy, plural, or visual review.

## Build and run

On Debian or Ubuntu, install native development headers:

```sh
sudo apt-get install libgtk-4-dev libadwaita-1-dev gettext pkg-config
cargo run --features gui
```

The development worker starts a loopback-only fake service and places its endpoint in the form when
the default endpoint is still untouched. Readiness does not connect it: click **Connect**, wait for
model discovery, and deliberately choose a model before translating. The **Provider setup** card
shows each required step and, once ready, the exact provider name, stable ID, and confirmed model
for the next request. A pending model change remains in Step 2 until committed, and a stopped worker
is shown as unavailable with request controls disabled. The card is derived from live state and
writes no completion flag. A user-supplied OpenAI-compatible base endpoint such as
`http://127.0.0.1:11434/v1/` follows the same flow.

Use **Open text file** to load a UTF-8 TXT, Markdown, CSV, JSON, HTML, SRT, WebVTT, bounded DOCX/PPTX/XLSX/EPUB package, or text-based PDF into the source editor. The native
GTK file dialog and asynchronous GIO partial read enforce a 4 MiB limit, strip a UTF-8 BOM, reject
invalid UTF-8, and never place the selected path or file contents in diagnostics. Dropping one GIO
file onto the source editor reuses the same bounded import path. Native CI verifies the real XDG
document-portal lease lifecycle, drives the real GTK portal FileChooser backend under Xvfb, verifies
the application-level GTK FileDialog callback, and performs a real XTest drag through the source
editor. Prompted desktop flows and physical shell rendering remain separate follow-up work.

Each successful TXT, Markdown, CSV, JSON, HTML, SRT, WebVTT, DOCX, PPTX, XLSX, EPUB, or PDF import is also stored as a bounded Core document job.
**Translate**
then sends pending prose segments sequentially through the confirmed provider, emits segment events,
persists each completed segment, and saves only validated non-secret source/target locale,
provider/model identifiers, quality mode, translation preset, and glossary rules. A worker restart can restore the unfinished snapshot;
**Resume** and **Retry** reuse those options only after the active provider and model match. **Stop**
cancels the active document segment and leaves the source unchanged; Incognito mode intentionally
rejects new document jobs because their progress must be persisted. Subtitle timestamps and cue IDs
remain unchanged; cue text is translated without automatic timing or line-length rewriting. Core
The text workspace also provides localized `General`, `Technical`, `Marketing`, `English (United
States)`, and `Chinese (Simplified, Mainland China)` translation presets. The regional presets
carry bounded `en-US`/`Latn` and `zh-CN`/`Hans` preferences. Each preset is a bounded request-level
preference; document jobs persist and reuse the selected preset across pause, retry, and restart.

reports cue-level warnings when the configured line-length or reading-speed guidance is exceeded;
the Linux UI shows cue numbers without source text. CSV
delimiters, quoted fields, variable-width rows, and line endings remain unchanged; the Linux chooser
translates eligible text fields by default, skips common identifier and numeric columns, and uses
Core's selected-column contract for hosts that provide explicit column selection. JSON keys, numbers,
booleans, nulls, whitespace, and escaping remain unchanged; string values are translated by default.
HTML tags, attributes, links, scripts, and styles remain unchanged; visible text is translated and
special characters are escaped on reconstruction. DOCX/PPTX/XLSX imports retain package resources and rewrite only
bounded text nodes in supported OOXML parts; encrypted, traversal, oversized, malformed, and DTD-bearing
packages are rejected.
EPUB imports require a bounded ZIP with a first `mimetype` entry, preserve metadata, navigation,
spine, CSS, and binary resources, translate visible XHTML/HTML text, and update OPF `dc:language`
metadata at export time. Text PDFs preserve page association and available coordinates; reliable
ASCII text streams are rewritten in place, while unsupported PDF text encoding exports a page-aware
HTML file with fidelity limitations rather than claiming pixel-identical reconstruction. Image-only
pages remain unchanged by default. When the user enables **OCR**, Linux invokes the optional
`pdftoppm`/`tesseract` plugin under bounded input, page, output, and timeout limits and imports a
page-marked TXT document; it never rewrites the original PDF or claims pixel-identical reconstruction.
If the tools are unavailable or fail, the original image-only PDF remains intact and the fixed error
is shown in the UI.
The editor surfaces Core's structured warnings for limited reconstruction, image-only pages, and
uncertain reading order without including source content in diagnostics.

After a translation completes, **Copy translation** places the output on the system clipboard through
the native GTK display clipboard, while **Export translation** opens a native GTK save dialog and
writes the output asynchronously as UTF-8. **Swap languages** locally exchanges the supported
English/Chinese source-target pair without sending a request or changing editor contents; Auto-source
and Japanese-target combinations keep the action disabled. **Clear workspace** is a local,
network-free action that removes the source text, translated output, request diagnostics, and
transient file notices while preserving provider, locale, glossary, and history settings. Both output actions remain disabled
without output; export reports a localized success notice and refuses to overwrite the imported source
file. Clipboard contents are never persisted or logged.

The credential field is optional. Its value is copied into Core's secret-aware `SecretValue`, the
widget is cleared immediately, and the temporary GTK string is dropped. Without Remember, a
`session:` `SecretRef` lets the bounded typed host-secret broker provide it once during connection.
When Remember is selected with a credential, the Linux host stores it through Secret Service and
persists only the resulting `secret-service:` reference with the profile. New saved profiles
receive a random stable ID independent of their display name. The worker stores each non-secret
profile copy in Core's SQLite database at
`$XDG_DATA_HOME/dev.linguamesh.LinguaMesh/linguamesh.sqlite3` (normally under
`~/.local/share`) with a `0700` application directory and `0600` database file. Core opens SQLite
with no-follow protection on Linux's default Unix VFS, rejecting any symbolic-link component; the
Linux layer additionally rejects hard links and non-private storage paths.

Secret custom request headers follow the same boundary: they are cleared from the form immediately,
stored by Secret Service when remembered, or passed through a `session:` reference for one connection.

Startup restores the complete saved-profile list and displays the last persistently activated row,
but remains disconnected and performs no provider request. Selecting another row only prefills the
form. Enter the credential again when required, then click **Connect** to validate and switch.
**Remove saved profile** deletes only that stored row; if it is currently connected, the validated
runtime session and model continue in visibly session-only mode. Provider controls remain disabled
until startup finishes. Credential values are never written to the database; only persistent
`SecretRef` identifiers are stored. Secret Service absence, locked items, and unsupported
interactive prompts fail closed with typed errors instead of falling back to plaintext. Native CI
also exercises non-root prompt paths for store and delete and verifies the same typed rejection;
Secret Service and portal unlock prompts remain outside the automated gate. Session-only
connection remains available when remembering is disabled or profile storage/keyring access is
unavailable. If a persistent Secret Service store is declined or unavailable, the Linux client
shows a localized warning and explicitly offers session-only recovery without silently changing the
Remember choice. Connection and translation can both be cancelled, and a failed provider switch
preserves the previously confirmed provider and model.

If an already-open database later returns a persistent write error, the triggering Connect, model
change, or deletion is rejected before any success is reported. The worker drops that storage
handle and saved-profile marker, keeps the previously validated engine and model usable in
session-only mode, and reports storage as unavailable. A private Linux tmpfs regression forces a
real `ENOSPC` at each transaction boundary and verifies after restart that only pre-fault state was
committed. A non-writable private-directory regression and corrupt-database regression also fail
closed while retaining session-only translation; power-loss and broader SQLite VFS behavior remain
unverified.

The tested external-provider path includes deterministic loopback fixtures for generic OpenAI-compatible
servers (including LM Studio-style `/v1/` deployments), Ollama-compatible OpenAI `/v1/`, and native
Ollama `/api`: model discovery returns `llama3.2:latest`, the worker requires
deliberate selection, and streaming uses `/v1/chat/completions` or `/api/chat` without a credential.
The GTK provider form exposes localized OpenAI-compatible, native Ollama, Anthropic Messages,
Google Gemini, Azure OpenAI, and OpenAI Responses presets. Anthropic uses the HTTPS `/v1/` endpoint and requires a
manual Model ID before Connect; Gemini uses the HTTPS `/v1beta/` Generate Content endpoint with
model discovery; Azure OpenAI uses the resource endpoint, sends the session credential in the
`api-key` header, pins API version `2024-10-21`, and requires the deployment name as a manual model
value. Azure model discovery is intentionally manual, so the client never enumerates deployments.
Every preset also exposes an optional manual model field. Native or protocol-compatible discovery
remains first; when its listing endpoint is unavailable or returns no models, Core retains the
validated selected model as a localized `Manual` entry. Authentication, network, and timeout errors
remain typed failures and do not silently fall back.
HTTP 429 responses are classified as a localized rate limit and retain a bounded `Retry-After`
retry hint; quota and billing semantics are not inferred.
OpenAI Responses uses `/v1/models` discovery and typed SSE events from `/v1/responses`, including
`response.output_text.delta` and `response.completed`; its credential is sent only as Bearer
authentication. Custom endpoint edits remain preserved when switching presets.
`tools/run-ollama-interop-test.sh` provides an
opt-in regression against a caller-selected installed model; the default suite keeps it ignored
because model installation is external. The Linux checkpoint passed this harness against Docker
`ollama/ollama:0.11.10` with `qwen2.5-0.5b-instruct:latest`; GPU and stable-release evidence remain
separate gates. Full
validation commands, the header-free local path, and the GTK gates for X11/Xvfb and forced
Wayland/headless Weston are documented in
[`docs/testing.md`](docs/testing.md). Native CI also builds a release-mode binary and uploads it
with SHA-256, SPDX 2.3, build-context, and repository-only source-archive sidecars; this is
unsigned prerelease evidence, not a stable or distributable release. The source archive records the
Linux checkout only and still requires the pinned Core and localization repositories for a build.
Native CI also records a machine-specific performance baseline for representative DOCX, XLSX, and
routing-dispatch tests; the measurements are evidence for trend review, not cross-machine claims.

The repository now includes a reproducible Flatpak manifest, pinned Cargo source set, desktop
entry, AppStream metadata, and icon under [`packaging/flatpak`](packaging/flatpak). Run
`bash tools/validate-flatpak-metadata.sh` for local metadata validation. The GNOME 49 SDK build and
bounded private-D-Bus sandbox smoke run remotely; the resulting bundle is a prerelease CI artifact,
not a signed or published release. Native CI verifies headless delivery to a real `dunst` notification
daemon; the direct portal chooser backend, application-level chooser/drag fixtures, and visible
Dunst desktop-shell window check pass, while release artifacts remain a separate gate.

The main action row includes a localized About dialog that reports only the application version and
the shared Core version, ABI, and protocol dimensions. It is read-only and does not display provider
endpoints, credentials, model IDs, or translation content.

The two display gates execute the same real GTK binary test. Headless Weston proves that the client
can initialize and complete that flow with `GDK_BACKEND=wayland` and no X11 fallback; it is not
evidence for a physical compositor, GPU rendering, assistive technology, or a complete desktop
matrix.

At worker startup, the client requires exact Core `0.1.0-alpha.2`, ABI 1, protocol 1, provider
catalog `0.1.0`, and the reviewed feature subset. The native workflow checks out the exact
functional revision above; an arbitrary default branch is not compatibility evidence.

Canonical PO/MO catalogs are synchronized from immutable l10n revision
`c2526bfb3f6ff57895bdc3eeed743e26c8783613` and validated with `msgfmt`; the 506-message bundle
adds Linux routing-profile persistence/editor, profile-ID validation and duplicate protection, ordinary-text selection labels, routing preference/privacy/document constraints, provider/model allowlists and denylists, quality/request-size limits, translation quality-mode and translation-preset labels, and source/output character plus approximate-token metrics. The locale selector
exposes all twelve official BCP 47 packs plus the generated `en-XA` accented and `ar-XB` RTL
 pseudo-locales. It switches runtime action, workspace-widget,
active-provider, status summary/partial-output, text-file import/export, provider-profile controls, source/target language options, onboarding stage/detail guidance, fixed provider/file/worker and reducer-state/category error messages, construction-stage provider/default-control copy, and diagnostics labels/state values without replacing active source text;
Arabic and `ar-XB` also switch the GTK workspace root to right-to-left direction. Document-job actions,
dialogue, empty/paused/progress statuses, and queue tooltips are now catalog-backed across the
same official and pseudo packs; document-job row metadata and lifecycle states, exported-output open and failure actions are localized as well. Stable Linux worker startup,
Core compatibility, and profile-storage error sentences now use the same catalog; arbitrary backend
diagnostic detail remains an explicit English fallback. Completed ordinary text output also shows
localized usage metadata with a provider-reported, locally estimated, or unknown source label;
provider billing semantics, pricing, and stable ABI projection remain future work.

The connected model selector appends a localized provenance label to each entry, identifying whether
the model was **Discovered**, provided by the **Catalog**, or entered **Manually**. This presentation
does not persist credentials or change the selected model ID.

Pseudo-locales are layout and direction test data, not qualified translations. Headless fixtures can
select them with `LINGUAMESH_TEST_LOCALE=en-XA` or `LINGUAMESH_TEST_LOCALE=ar-XB`; generated strings
preserve placeholders and add expansion or bidi-isolation markers.

The text workspace accepts bounded semicolon-separated glossary rules such as
`LinguaMesh => 凌瓦网; Acme Product => Acme Product`. Rules are request-scoped and remain in memory;
Core validates conflicts, protects matching terms with opaque markers, restores required target terms
across streamed output, and rejects credential-shaped entries without persisting glossary content.

Linux standard text translation also exposes an explicit **Allow approved fallback** control. The user
must choose a different saved provider; only network or timeout failures from the confirmed primary
provider can select it. The UI records the selection and warns that content may be sent there. Fallback
is unavailable for document jobs, incognito requests, cancellation, authentication failures, model
errors, and unapproved or session-only profiles; partial primary output is retained across the switch.
Before an ordinary request with fallback enabled is dispatched, Linux shows a localized confirmation
window explaining that content may reach the approved provider. **Translate** grants one request;
**Close** cancels without sending. Secret Service or portal unlock prompts remain fail-closed/manual.

The routing-profile dialog also lets the user choose Core's **Manual**, **Ordered**, or **Automatic**
mode before saving a profile. Fallback consent is separate, explicit, and disabled by default; a
manual profile never falls through to another candidate. The candidate editor lists saved provider/
model pairs as focusable checkboxes. Manual mode saves only the first selected pair and deactivates
additional selections when switching to or editing Manual; Ordered and Automatic retain the selected
candidate chain. Adjacent up/down controls and row drag-and-drop reorder them for Ordered mode and expose localized accessible
labels for screen readers and keyboard users. Existing saved profiles can be loaded with **Edit**;
the editor restores their mode, fallback consent, candidate checks, and order, then replaces the
same non-secret profile ID on **Save routing profile**.
The Linux keyboard fixture also activates the Provider preset through its visible `Alt+P` mnemonic
before checking the explicit Tab/Shift+Tab order; Arabic uses the catalog's English fallback for
that mnemonic while still exercising RTL traversal.
New profiles use a bounded ID field (1–128 ASCII letters, numbers, `.`, `_`, or `-`); editing locks
the existing ID so references remain stable, while distinct IDs allow multiple saved routing profiles.
Attempting to create a new profile with an existing ID is rejected instead of silently replacing it.
**Import profile** and per-row **Export** use a bounded Core JSON exchange format containing only
validated candidate capabilities and privacy constraints. Unknown fields, endpoint/credential-shaped
data, malformed or non-UTF-8 files, payloads over 64 KiB, and duplicate IDs are rejected; imports
never overwrite an existing profile.

When an ordinary text request ends in a failed or cancelled state, **Retry translation** becomes
available. It reuses the current source, target, glossary, privacy mode, confirmed provider, and
model through the same worker command path as **Translate**; it is disabled for active document jobs,
busy requests, and completed requests. Document jobs continue to use their separate persisted queue
retry action.

Long source text is split at paragraph, sentence, or whitespace boundaries using a conservative
16 KiB byte estimate when no tokenizer is available; opaque protected markers remain whole and
chunks stream sequentially with cancellation preserved.

The glossary controls support bounded UTF-8 CSV and TBX interchange. **Import glossary** reads a
4 MiB-or-smaller file through the native file dialog, accepts the fixed CSV schema or restricted
TBX language sets, validates up to 256 rules in Core, and keeps imported rules request-scoped in
memory. **Export glossary** writes
the deterministic Core CSV schema to a user-selected file without persisting credentials or
glossary content in provider profiles or SQLite.

The **Incognito mode** toggle carries an explicit Core privacy policy on the next translation
request. Standard completed translations now persist in bounded local SQLite history (100 entries,
with a 4 MiB source/output limit); startup restores only the count and **Clear history** removes all
entries. Incognito requests skip history writes. **Save translation history** is a persisted policy
toggle: disabling it keeps existing entries but prevents future standard completions from being saved.
**View history** opens a bounded scrollable list, supports exact per-entry deletion, and exports the
displayed snapshot as escaped UTF-8 TSV. **Save translation memory** is a separate persisted policy
toggle. Enabled standard requests reuse a bounded local memory only when normalized source, locales,
provider/model identity, glossary, chunking, and versioned translation policies all match. Incognito
requests never read or write memory. **View translation memory** supports inspection, escaped TSV
export, exact per-entry deletion, and **Clear translation memory** removes all entries. Both policy
toggles are disabled while a conflicting operation or unavailable profile storage is active.

When a translation completes, the registered Linux application sends a desktop notification with
localized generic copy only; source and translated content are never included in notification
payloads. Native CI delivers that payload to a real `dunst` daemon under Xvfb and verifies a visible
viewable Dunst desktop-shell window; physical compositor and GPU coverage remain unverified.

## Documentation

- [Architecture](docs/architecture.md)
- [Testing](docs/testing.md)
- [Releasing](docs/releasing.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
