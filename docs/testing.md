# Testing and Validation

The Linux worker tests cover both local-model wire contracts: the `local-loopback` preset uses
OpenAI-compatible `/v1/` discovery and SSE, while the `ollama` preset uses native `/api/tags`
discovery and `/api/chat` NDJSON streaming. The native fixture asserts deliberate model selection,
fragmented UTF-8 handling, cancellation, and the translated `你好，Ollama！` output without a
credential. These tests do not replace interoperability testing against a running third-party
Ollama daemon.

The `lm_studio_style_openai_compatible_provider_translates_without_secret` fixture exercises the
same generic `/v1/` Chat Completions contract used by LM Studio-style local servers. It verifies
model discovery, deliberate selection, streaming translation, and the absence of a credential;
the fixture is protocol evidence, not a claim that a particular LM Studio build is installed.

The GTK provider regression also selects the Anthropic Messages preset, verifies its HTTPS `/v1/`
default, and requires a manual Model ID before Connect. The model is validated locally before any
worker connection or host SecretRef resolution; saved profiles restore the non-secret model ID.
The profile form also round-trips an optional bounded non-secret note through Core schema 19;
credential-shaped notes are rejected before persistence, and the note never enters provider input.
It also round-trips the optional bounded organization identifier through Core schema 20; the
OpenAI-compatible adapter adds it only as `OpenAI-Organization` and rejects credential-shaped values.
The same form round-trips bounded canonical JSON custom headers through Core schema 23; the Core
domain and OpenAI adapter reject authorization, credential-shaped, and built-in metadata names,
while a safe header is applied without replacing authentication metadata. The same provider profile
contract round-trips a credential-free proxy URL through Core schema 25; unsupported schemes, URL
paths, query strings, and embedded proxy credentials are rejected before any host-secret request,
and the selected transport applies the proxy to provider HTTP requests.
The same contract round-trips a bounded total request timeout (1–600 seconds, default 30) through
Core schema 26, a bounded connection-establishment timeout (1–120 seconds, default 10) through
schema 27, and a bounded streaming idle timeout (1–300 seconds, default 60) through schema 28.
All reject out-of-range values before any host-secret request and are applied independently to every
currently supported provider transport. A stalled-body fixture verifies that streaming idle timeout
returns a typed timeout error. Core rejects malformed or private-key PEM bundles before constructing
the client; Linux form tests cover the persisted optional trust-bundle field.

The environment-gated `running_client_certificate_provider_connects`,
`running_client_certificate_provider_rejects_untrusted_server`, and
`running_client_certificate_provider_rejects_hostname_mismatch`, and
`running_client_certificate_provider_rejects_untrusted_client` regressions exercise real HTTPS
model-discovery requests against `tools/client-certificate-http-fixture.py`. Run them with
`bash tools/run-client-certificate-interop-test.sh`; the runner creates temporary trusted and
untrusted CAs, server certificates, a wrong-SAN server certificate, and a client identity,
requires the client certificate during the TLS handshake, and deletes all material after all four
tests. The worker supplies the identity through a session SecretRef and the trusted CA through the
bounded trust-bundle field. The first test proves successful Linux rustls client-authentication
wiring; the second proves that a server certificate outside the configured trust bundle is rejected;
the third proves hostname verification rejects a wrong SAN even when the signing CA is trusted; the
fourth proves a server with a different client-CA trust chain rejects the presented identity.
None of the tests persists a key or claims live enterprise interoperability.

The same provider fixture covers the Google Gemini preset through the `/v1beta/` Generate Content
contract: `models` discovery filters entries that support `generateContent`, and the streaming
path consumes fragmented SSE candidates until a `finishReason` terminal event. Credentials use the
`x-goog-api-key` header and never appear in diagnostics. This deterministic loopback fixture does
not claim live external Gemini-account or quota coverage.

The Azure OpenAI fixture uses a resource URL, the fixed API version `2024-10-21`, and an
`api-key` session header. It supplies `fake-deployment` manually, verifies that no deployment-list
request is made, and streams the deterministic `你好，Azure！` response through the real worker path.
This proves request shaping and secret isolation only; live Azure account, quota, and deployment
availability remain unverified.

The dedicated ignored GTK fixture `gtk_provider_protocol_presets_use_native_transports` now drives
the Anthropic, Gemini, and Azure production preset rows through the real Connect, model-selection,
and Translate handlers. Anthropic uses the manual `claude-test` model, `x-api-key` session
credential, and `/v1/messages` SSE; Gemini discovers and translates `gemini-2.0-flash` through the
loopback `/v1beta/` path; Azure uses the `fake-deployment` manual model and `api-key` session
credential through the resource endpoint. Native CI runs the fixture serialized under Xvfb and
DBus; it remains deterministic protocol evidence rather than live-provider account or quota
evidence.

The OpenAI Responses fixture uses the shared `/v1/models` discovery route and `/v1/responses` with
the session Bearer credential. The worker verifies one model-list request, one typed-SSE translation
request, ignores `response.created`, streams `response.output_text.delta`, and completes on
`response.completed` with deterministic `你好，Responses！` output.

Authentication failures from provider HTTP 401/403 responses are mapped to the catalog-backed
actionable authentication message before GTK renders the error label. The focused model regression
checks Simplified Chinese copy and confirms backend status numbers are not shown to users; worker
fixtures separately verify wrong credentials remain out of diagnostics and persistence.

Provider HTTP 429 responses are mapped to the shared `RateLimited` category while preserving the
bounded `Retry-After` hint. The focused model regression renders the localized category and plural
retry instruction; the shared Core provider-api test covers the status mapping used by every adapter.
Live provider quota behavior remains unverified.

The serialized GTK fixture `gtk_authentication_failure_shows_localized_redacted_error` extends this
boundary through the real Connect button, Core worker rejection event, Simplified Chinese locale,
and `Alert` presentation. It is marked ignored in the ordinary Rust suite because GTK initialization
is thread-bound; Native CI runs the dedicated display-backed fixture and is the authoritative result
for this path on hosts without the pinned GTK/Xvfb runtime. The Native workflow invokes this exact
test name with `--exact --ignored --test-threads=1` so the serialized fixture cannot be skipped by
the general test step.

The serialized GTK fixture `gtk_offline_connection_failure_preserves_confirmed_session` then
connects a confirmed provider, deliberately releases a loopback port, and submits a second
connection attempt to that unavailable endpoint. It verifies the network `Alert`, cleared
credential field, preserved active provider/model, and untouched source buffer while the UI returns
to Ready. Native CI invokes it explicitly with the same DBus/Xvfb serialization; this is the
display-backed Scenario 17 evidence when the local host has no GTK display runtime.

The serialized GTK fixture `gtk_connection_test_reports_models_and_redacts_credential` exercises
the production **Test connection** button without committing a provider session. A successful
bearer-token probe reports a bounded discovered-model count through the localized status note and
clears the credential field; a second probe with a wrong canary renders the catalog-backed
authentication error without the canary or HTTP status details. Native CI invokes this exact test
under the same DBus/Xvfb serialization; the fixture is prerelease Scenario 8/Provider Hub evidence
and does not claim live-provider or human visual/Orca review.

The serialized GTK fixture `gtk_provider_health_label_tracks_selected_saved_profile_state` drives
the Provider Hub health label through its hidden, successful-timestamp, normalized-failure, and
cleared states using only non-secret saved-profile metadata. It confirms that the selected saved
profile controls the label and that clearing the selection hides it again. Native CI runs this
exact test under DBus/Xvfb; it does not claim live-provider, visual, or human accessibility review.

The serialized GTK fixture `gtk_cancel_translation_preserves_partial_output` selects the slow
loopback model, starts a real streamed translation, clicks the production **Stop translation**
button after the first delta, and verifies that the partial output remains visible while the
operation reaches exactly the `Cancelled` UI state. The fixture also confirms that Stop becomes
disabled, Retry becomes available, and no later provider delta changes the cancelled output.
Native CI runs this exact test under the same DBus/Xvfb serialization; it is Linux evidence for
Scenario 6 and does not claim physical transport cancellation for every provider.

The serialized GTK fixture `gtk_glossary_and_protected_terms_preserve_translation` drives the real
glossary entry field and Translate action against a request-inspecting loopback provider. The
provider receives an opaque protected marker instead of `LinguaMesh` and returns that marker split
across streamed deltas; the production reducer restores `凌瓦网` in the completed output. Native CI
runs the exact fixture under DBus/Xvfb, providing Linux Scenario 9 glossary/protected-span evidence
without persisting glossary content or claiming provider-specific behavior beyond the contract.

The worker fixture `glossary_library_commands_persist_and_delete_across_worker_restart` exercises
Core schema 33 through the Linux command path. It saves a validated library, lists it, deletes it,
and confirms the normalized term rows are gone. The GTK workspace actions now provide the
production save/list/load/delete path; a display-backed selector fixture remains Native-CI
authoritative because this host lacks the pinned GTK test runtime. The production glossary chooser
now accepts `.csv` and `.tbx` files, applies the same 4 MiB partial-read bound, and dispatches TBX
files to Core's restricted parser; Core tests cover entity decoding, DTD rejection, missing terms,
and malformed/oversized input. Cross-client library parity is not claimed.

The serialized GTK fixture `gtk_interrupted_document_job_restores_and_resumes` creates a persisted
two-segment text job, drives the production Translate and Pause controls after the first segment is
committed, shuts down the worker, and starts a second GTK worker against the same database. The
restored paused snapshot retains one completed segment and the untouched source buffer; after a
fresh session-only provider connection, the production Resume control completes the remaining
segment without duplicating the first. Native CI runs the exact fixture under DBus/Xvfb, providing
Linux Scenario 12 restart/resume evidence without claiming physical power-loss recovery.

The production Document jobs dialog also exposes Export translation report for every persisted
snapshot. The action writes a deterministic TSV report with non-secret identifiers, configuration,
segment counts, warning kinds, state, and Unix timestamps. Fields are single-line escaped, source
aliases are rejected before the asynchronous GIO write, and document source text, credentials,
local paths, and provider-reported usage are never included. The `usage` field is a bounded,
non-sensitive local estimate derived from persisted source/translated segment lengths; retry counts
remain explicit `unknown` because persistence does not retain attempt history. The unit regression
document_translation_report_is_redacted_and_counts_segments covers the report builder, deterministic
usage JSON, and absence of source-segment bodies; the
serialized GTK queue fixture also requires one focusable, redacted-report button with a tooltip for
each persisted row. The fixture does not open a native chooser, so the asynchronous write callback
and visual file-selection flow remain covered by the existing portal/CI boundary.

Translation export naming follows the document contract: the default is
`<original-base-name>.<target-bcp47-tag>.<extension>`, with control characters and path
separators sanitized and `und` used when no target tag is available. If the selected local
destination already exists, the GTK save path chooses the first available deterministic `-1`,
`-2`, ... suffix instead of replacing it; the same collision guard applies to report exports.
After the collision check, each local export writes to a same-directory temporary file, closes it,
syncs the file and parent directory, and uses GIO's non-overwriting move to finalize the destination;
the parent directory is synced again after the move. If either durability barrier fails, the export
reports a save error rather than claiming durable completion. Non-local URIs fall back to GIO's
exclusive create and retain an explicit remote-VFS boundary. A file created by another process in the
race window is left unchanged, temporary artifacts are removed after a failed finalization, and the
export reports a save error instead of overwriting it. This writer is shared by translated output, document reports, glossary CSV,
routing-profile JSON, translation-history TSV, and translation-memory TSV exports; no user-visible
export path uses overwrite-enabled replacement.
The `ExportWriteStrategy` policy makes that split explicit: a local path with a parent uses the
same-directory atomic move, while a URI without a verifiable local path is constrained to exclusive
creation. The `non_local_export_uses_exclusive_create_fallback` regression preserves the remote URI
unchanged and prevents a future refactor from treating a remote backend as a local rename target;
it does not claim atomicity or connectivity for every remote GIO/VFS backend.
The `non_local_source_alias_is_rejected_by_uri_identity` regression also rejects an export target
that is the same non-local URI as the imported source, even though no local inode is available for
canonicalization or hard-link checks.
The `translation_output_name_uses_source_stem_and_target_locale` and
`collision_safe_output_path_adds_stable_suffix_without_overwriting` regressions cover the naming
and collision rules, while the ignored GTK regression
`gtk_atomic_output_writer_never_replaces_existing_file` covers the exclusive-write boundary,
failed-finalization cleanup, and the report regression checks the stable output identifier. Native
CI runs the ignored fixture
under serialized DBus/Xvfb; the local host's GUI linker limitation keeps that check CI-authoritative.
The paired ignored fixture `gtk_atomic_output_writer_survives_process_interruption` starts a child
GTK test process, waits until its same-directory temporary file is synced and the final move is
about to begin, then terminates that process. It requires the final destination to remain absent
and the durable temporary bytes to remain inspectable, demonstrating the process-interruption
boundary without claiming physical power-loss recovery. Native CI runs this fixture serialized
under DBus/Xvfb; the child uses only temporary files and removes its directory after inspection.
The focused unit regression `local_export_sync_barrier_accepts_file_and_parent_directory` also
opens a nested local export, calls the file-and-directory barrier directly, and removes the fixture.
It proves the local descriptor path is callable without claiming physical power-loss recovery or
alternate-VFS behavior.

The GTK regression `provider_presets_map_to_stable_native_and_compatible_defaults` validates the
six-position Linux preset order against the bundled Core provider catalog. Adapter types must match
the catalog, and manual-model visibility is derived from its `model_listing` field. The application
performs the same compatibility check before creating the window and fails closed with an English
diagnostic if the pinned catalog drifts.

The worker regression `reviewed_core_contract_is_required_exactly` mutates each compatibility
dimension independently—Core semantic version, ABI major, protocol version, provider-catalog
version, and required feature set—and requires every mismatch to return `ProtocolIncompatible`.
This is a fail-closed Scenario 16 contract check; it does not claim compatibility with an
unreviewed Core release.

The Linux worker regression `gemini_provider_discovers_and_streams_without_secret` now exercises
that fixture through the real `ProviderManager` and worker path: it discovers
`gemini-2.0-flash`, deliberately selects it, and completes `你好，Gemini！` without a credential.

The real-daemon regression is opt-in and never downloads a model by default. With a running
third-party daemon and an installed model, execute:

```sh
LINGUAMESH_OLLAMA_MODEL=smollm:135m bash tools/run-ollama-interop-test.sh
```

The 2026-07-23 Linux refresh used the pinned Docker image `ollama/ollama:0.11.10` with a
host-network temporary daemon and `smollm:135m`; `running_third_party_ollama_provider_translates_without_secret`
passed exactly once without a credential. The daemon and model store were removed after the run.

Set `LINGUAMESH_OLLAMA_PULL=1` only in an isolated environment when the named model should be
pulled through Ollama's `/api/pull`; the script then exercises real `/api/tags` discovery and
`/api/chat` translation through the Linux worker. The default test suite keeps this regression
ignored because the daemon and model are external prerequisites.

The Linux checkpoint has a reproducible external pass using Docker image
`ollama/ollama:0.11.10`, model `qwen2.5-0.5b-instruct:latest`, and the Qwen GGUF SHA-256
`9ee36184e616dfc76df4f5dd66f908dbde6979524ae36e6cefb67f532f798cb8`. The harness reported
`1 passed; 0 failed` through native `/api/tags` and `/api/chat`; the temporary daemon and model
were removed after validation. This evidence is prerelease-only and does not cover GPU execution.

The Linux checkout consumes the canonical gettext bundle from immutable l10n revision
`c2526bfb3f6ff57895bdc3eeed743e26c8783613`. The bundle contains 506 messages, and
`bash tools/sync-l10n.sh --check` verifies every PO/MO catalog and the generated manifest before
the native build. History/memory row metadata, document-job IDs, active-provider mode summaries,
unavailable provider/model labels, and routing-profile actions/mode labels are asserted through
catalog keys rather than concatenated English UI fragments; non-English packs remain
machine-generated drafts pending human review. The editor metrics regression checks character
counts and an explicitly approximate token estimate without exposing text in diagnostics. Completed
translation output also shows a localized usage line whose source is provider-reported, locally
estimated, or unknown; missing counts remain unavailable rather than fabricated. Routing
profile tests also verify preference-index round trips and preservation of hidden Core constraints
when visible privacy/capability controls are edited. Constraint parser tests cover comma-separated
provider/model lists, positive numeric limits, and rejection of unsafe or empty values.

The license-notice dialog uses a read-only bundled `THIRD_PARTY_NOTICES.md` file rather than any
networked source. The regression reads the bundled notice text and requires representative
entries (`GTK 4`, `LGPL-2.1-or-later`, `MIT`, `LinguaMesh Core`). The production action and
dialog title/tooltip remain catalog-backed; focusability is preserved for the window and close
controls so the boundary is still navigable under keyboard and accessibility testing.

The About action has a serialized GTK fixture that opens the modal and checks the version/details
label, focusable Close control, and omission of endpoint and secret markers. The non-GTK regression
also verifies that the details formatter contains only application/Core compatibility fields.

Every routing decision carries a redacted explanation into the diagnostics panel: the eligible
candidate keys, rejected candidate keys with stable reason codes, ranking inputs, and configured
fallback order. Model, worker, and GTK lifecycle tests assert these details while ensuring that
endpoints, credentials, and translated content never enter the event or visible diagnostics.

Quality-mode UI behavior maps the localized Fast/Balanced/Best dropdown to the Core
`TranslationQualityMode` values and keeps the selector enabled for a selected document job. The
worker restart regression selects `Best`, persists it through a routed dispatch, and verifies the
resumed snapshot retains `Best`. Core
tests cover the versioned `translation-prompt-v2` directives and deterministic rejection of empty
or Unicode-replacement output before `Completed`; no hidden extra provider request is introduced.

Retry policy is covered at both contract boundaries. Core provider adapters parse numeric or HTTP-date
`Retry-After` headers into an optional, backward-compatible error field capped at sixty seconds.
Linux consumes that hint with an eight-second maximum wait, otherwise uses bounded exponential backoff
with stable candidate-key jitter, cancels the wait on shutdown, and opens an in-memory candidate
circuit after two retryable failures for a thirty-second cooldown. The worker tests
`routing_backoff_prefers_retry_hint_and_stays_bounded` and
`routing_circuit_breaker_opens_after_repeated_failures_and_resets` cover the policy without logging
endpoints, credentials, request text, or provider output.

The GTK action row covers text translation, retry, native clipboard copy, local language swap and clear-workspace, export, provider switching,
and cancellation. Copy is enabled only for non-empty output and sends bytes directly to the display
clipboard without passing through Core or persistence; the display-backed assertion remains a Native
GTK boundary because this host cannot link the GTK test binary locally.

Translation-preset UI behavior maps the localized General/Technical/Marketing dropdown to the Core
`TranslationPreset` values and carries the selection into ordinary requests. Linux tests cover all
stable IDs and compatibility negotiation rejects a Core that does not advertise
`translation_presets_v1`; document jobs persist and restore the selected preset through schema 18
after pause, retry, and worker restart.

The source-level localization checks are reproducible without GTK or third-party packages:

```sh
python3 -B tools/check-localization-keys.py
python3 -B tools/check-localization-placeholders.py
python3 -B tools/check-visible-localization.py
```

The first command checks catalog key coverage for localization calls, diagnostics, and error-message
mappings; the second checks canonical English fallback text, placeholder identity, and malformed
braces in literal templates; the third rejects non-empty literal strings passed directly to GTK
visible-control APIs and direct string-list options. It scans every Rust file under `src/` and also
covers file-filter names. Empty strings used to clear a transient label are intentionally permitted.
This remains source-level evidence; human translated-copy, plural, and visual review are separate
gates.

The runtime locale suite also loads the generated `en-XA` accented and `ar-XB` RTL pseudo-locales.
They are layout and direction test data rather than qualified translations; the headless GTK
fixtures select them with `LINGUAMESH_TEST_LOCALE=en-XA` or `LINGUAMESH_TEST_LOCALE=ar-XB` and verify
placeholder-preserving expansion or RTL metadata.

The routing-profile worker regression saves, lists, and deletes a Core `routing_planner_v1` profile
without persisting provider endpoints, credentials, or translation content. A separate regression
selects a saved candidate, reconnects it through the host secret broker, and completes an ordinary
text request while asserting the typed decision event. The ordered-chain regression stops the first
saved provider before dispatch, verifies the next eligible candidate is selected, and asserts the
typed routing-fallback event and translated output. The worker also retries a retryable stream
failure across remaining automatic or ordered candidates, preserving event ordering and partial
output. The automatic-chain regression additionally sets a quality preference, verifies the higher-
quality candidate is selected, then shuts it down before dispatch and proves the worker uses only
the next approved candidate. A document-job regression selects a saved document-capable routing candidate while a
different provider is active, translates every pending segment through that candidate, and asserts
that the document decision reports no fallback even when the profile permits explicit fallback.
The production fallback confirmation window is covered by the dedicated GTK test
`gtk_fallback_approval_dialog_requires_an_explicit_one_shot_action`. It verifies the modal warning
copy and focusable actions, that `Close` dismisses without dispatch or approval, and that one
`Translate` click records one-shot approval and exactly one translation dispatch. The test is
marked ignored in the parallel Rust suite and is run explicitly under `dbus-run-session` and
`xvfb-run` in the Native workflow so GTK initialization remains on one thread.
The dedicated GTK test `gtk_routing_profile_candidate_controls_have_accessible_lifecycle` covers
the production candidate editor with two saved provider/model pairs. It checks the labelled profile
ID field, stable mode choices, explicit fallback checkbox, focusable candidate rows, accessible
up/down button labels, row reordering, Manual-mode single-candidate enforcement, and the Use
close-and-select lifecycle. It uses the same serialized DBus/Xvfb fixture boundary as the fallback
dialog test and remains prerelease evidence until visual, translated-copy, and end-user Orca review.
The GTK dialog creates a bounded profile from saved provider/model selections and now exposes the
Core `Manual`, `Ordered`, and `Automatic` modes in a stable order. Its separate explicit fallback
checkbox is off by default; when a routing profile is selected, it takes precedence over the
ordinary text fallback path, while document jobs never auto-fallback. Candidate checkboxes and
adjacent up/down controls and row drag-and-drop allow a profile to include and order a subset of
saved provider/model pairs. Manual mode persists only the first selected pair and deactivates extra
selections when loaded or selected; Ordered and Automatic preserve the candidate chain. The icon controls expose localized accessible labels, while empty
selections and invalid drag IDs are rejected before persistence. Each saved profile row also has an
**Edit** action that restores the persisted mode, fallback consent, candidate selection/order, and
ID; saving updates the same profile record rather than creating a duplicate.
New profiles validate IDs against Core's 1–128 byte ASCII identifier rule; edit mode locks the
existing ID to protect saved references, and a new profile cannot reuse an existing ID.
The same GTK lifecycle regression now enters edit mode, proves the ID field is locked, deselects a
candidate, saves the existing record, lists it through the worker, and reopens the editor to verify
the reduced chain survives the persistence round trip.
The same serialized fixture then uses that profile, invokes the production **Delete** action, applies
the typed `RoutingProfileDeleted` event, verifies the selected profile ID is cleared, and consumes
the worker refresh that returns an empty profile list. Native CI remains authoritative for this
display-backed lifecycle because the fixture is ignored in the parallel local test suite.
Core exchange tests round-trip a bounded profile, reject malformed/oversized JSON and unknown
fields, and assert that no endpoint or credential-shaped field can be exported. Worker tests cover
UTF-8 import, duplicate-ID rejection, persistence errors, and export of the validated profile.
Native and Flatpak CI remain authoritative for the GTK file chooser callbacks.
The restart regression `document_job_resume_reconnects_saved_routing_profile_after_restart`
interrupts a routed job, reopens the database, reconnects the saved profile through the host secret
broker, and completes the remaining segments while asserting a zero-fallback decision.

## Host prerequisites

Rust 1.93.0 is pinned by `rust-toolchain.toml`. A sibling `../linguamesh-core` checkout is required
because the client deliberately uses typed path dependencies instead of copying shared behavior.
The current synchronized checkout must be Core revision
`f5b818c3598d78e7cac30604577fa8057d380737`, a Linux storage-hardening descendant that adds the
non-locking `unix-none` VFS fail-closed regression on top of runtime baseline
`8623b2c8829e4d9cf7299c74440dcfabb4e320db`. The baseline carries bounded document lease
consumption smoke, POSIX-descriptor document consumption, and the AddressSanitizer gate, plus the
protocol decoder fuzz gate and bounded FileLease lifecycle,
including Linux's portal-read lease checks, and the explicit request-level
Incognito privacy policy and changes file-backed Core storage to add SQLite's `SQLITE_OPEN_NOFOLLOW`
flag, adds protected-span restoration and request-level glossary
protection for streamed text, and adds bounded semantic chunking. On
Linux's default Unix VFS, any symbolic-link path component is rejected. A clean documentation-only
descendant is acceptable
for local path builds when the compiled source tree is unchanged; validate it with:

```sh
git -C ../linguamesh-core cat-file -e f5b818c3598d78e7cac30604577fa8057d380737^{commit}
git -C ../linguamesh-core diff --quiet \
  f5b818c3598d78e7cac30604577fa8057d380737..HEAD -- \
  Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml crates assets migrations
test -z "$(git -C ../linguamesh-core status --porcelain)"
```

The same Core revision also includes bounded SQLite WAL replay regressions: a committed provider
profile remains recoverable when a reader holds a snapshot while the writer disconnects, and a
second Linux-only child-process fixture aborts after commit while using the bundled `unix-excl` VFS.
The next open replays the WAL sidecar in both tested paths. These cover the tested disconnect and
process-crash sequences only; power loss and other SQLite VFS failures remain outside the claim.

The same Core pin also negotiates `bounded_text_document_v1`, `routing_planner_v1`,
`translation_quality_modes_v1`, and `translation_presets_v1`: Linux imports only bounded UTF-8 TXT,
Markdown, CSV, JSON, HTML, SRT, WebVTT, DOCX, PPTX, XLSX, EPUB packages, and text-based PDF pages, preserves line endings, keeps Markdown fenced code and subtitle timing
structure verbatim, and
persists pending/running/paused document jobs and validated non-secret translation options, including
the selected translation preset and
the optional routing-profile ID, for worker
restart recovery. The Linux worker tests also cover
sequential prose-segment translation, per-segment persistence, safe reconstruction (including DOCX/PPTX/XLSX/EPUB package resources and PDF page association), structured HTML fallback for unsupported PDF encodings, and cancellation
to a persisted cancelled snapshot. The GTK surface now exposes per-job progress and
pause/resume/retry controls, and the worker regression
`document_job_list_returns_multiple_saved_jobs_for_queue_selection` verifies that two pending jobs
are listed together for explicit selection. `cancelled_document_job_can_be_retried_without_losing_pending_segments`
verifies that a cancelled job retains both pending segments and can be retried to completion with
the saved provider/model options. The worker regressions
The serialized GTK fixture `gtk_document_jobs_dialog_selects_between_multiple_jobs` drives the
production Document jobs window with pending, paused, and cancelled snapshots, asserts all rows and
their localized file count, then selects the paused row and verifies its job ID, state, and source
text are loaded without selecting another job. It reopens the queue, finds the single Resume action
for the paused row, activates it, and verifies that the same paused job remains selected while the
dialog closes after the command is sent. It then finds the single Retry action for the cancelled
row, activates it, and verifies the cancelled snapshot remains selected while the dialog closes.
It finally finds the single Pause action for the pending row, activates it, and verifies the pending
snapshot remains selected while the dialog closes. Native CI runs this fixture under the same
serialized DBus/Xvfb boundary as the other GTK document controls.

`imports_pptx_and_preserves_notes_and_resources`,
`document_job_translation_reconstructs_docx_and_preserves_binary_parts` and
`document_job_translation_reconstructs_xlsx_and_preserves_formulas_and_numbers`,
`document_job_translation_reconstructs_pptx_and_preserves_notes_and_resources` drive the
persisted-job translation path end to end, then inspect reconstructed OOXML while checking that
binary resources, formulas, and numeric cells survive. The worker concurrency gate allows four
document jobs and isolates their event streams and cancellation state by job ID.
`concurrent_document_jobs_run_independently` proves that two slow jobs can stream and complete
together, while `cancelling_one_concurrent_document_job_keeps_the_other_running` proves that
targeted cancellation does not interrupt its survivor. The fifth-job limit and duplicate-start
guard reject before any new Running snapshot is persisted. PDF imports
also expose bounded structured warnings for image-only pages, uncertain reading order, and limited
reconstruction; the UI warning test verifies that only page numbers and fixed text are shown, never
source content. Subtitle imports also expose configurable Core thresholds for line length and
reading speed; the default UI warning test verifies cue-number-only output.

The reviewed Core pin also rejects an OOXML ZIP entry whose uncompressed size is more than 200 times
its compressed size once the entry reaches 1 KiB. It also rejects OOXML macro (`vbaProject.bin`) and
digital-signature (`_xmlsignatures/`) parts as unsupported before XML inspection. These boundaries
are exercised by the Core document fixture and Linux wrapper fixtures
`rejects_docx_archive_with_suspicious_compression_ratio` and
`rejects_macro_and_signature_ooxml_packages_before_import`; they apply to DOCX, PPTX, and XLSX imports
before worker translation.

The serialized GTK fixture `gtk_malicious_archive_import_fails_closed_before_document_job`
drives the production asynchronous GIO `load_source_file` path with DOCX packages containing a
`../outside.txt` traversal entry, a highly compressed repetitive entry, an OOXML macro part, and a
digital-signature part. It verifies that every package surfaces a fixed import error, creates no
document-job snapshot, preserves the empty source editor, and leaves the private fixture directory
free of the forbidden entry names. Native CI runs this fixture under the same DBus/Xvfb serialization
as the other GTK document boundaries, so macro and signature rejection is exercised before any
production extraction or document-job creation.

A sibling `../linguamesh-l10n` checkout at the revision pinned by `tools/sync-l10n.sh` is required
to verify the checked-in PO catalogs.

Validate localization provenance and gettext syntax with:

```sh
bash tools/sync-l10n.sh --check
python3 -B tools/check-localization-keys.py
for file in l10n/linux/*/LC_MESSAGES/linguamesh.po; do
  msgfmt --check --check-format -o /dev/null "$file"
done
```

Toolkit-independent validation requires no GTK development headers:

```sh
cargo fmt --all --check
cargo check --all-targets --features demo-provider --locked
cargo clippy --all-targets --features demo-provider --locked -- -D warnings
cargo deny --manifest-path Cargo.toml --all-features check
cargo test --no-default-features --locked
cargo test --features demo-provider --locked
cargo build --features demo-provider --locked
DOCS_RS=1 cargo check --all-targets --all-features --locked
```

The `cargo-deny` policy is also enforced by the Native workflow. Advisory, license, and source
violations fail the gate; duplicate dependency versions are warnings while the GTK/Adwaita graph
is converged incrementally.

The current no-default suite reports `85 passed; 1 ignored`. It covers the text-import decoder, request-level glossary,
and explicit Incognito privacy policy in addition to the disconnected initial state, atomic
sorted restoration of multiple profiles without activation, duplicate/missing/default/session-ref
snapshot rejection, form-only selection, exact pending deletion, connected-row removal that keeps
the runtime session, pending and active canonical profiles, exact stale-result rejection, atomic
rollback, deliberate model selection, per-ID credential-free upserts, session switches that
preserve every restart row/default, active-ID model updates while another row is displayed,
saved-model restoration only when available, ordered events, partial output, all Core alpha.2 error
categories, the derived provider-setup stages through Ready, rollback that preserves the confirmed
Ready identity, pending-model confirmation that cannot claim Ready, worker-unavailable state,
storage-unavailable fallback, runtime persistence degradation that retains the confirmed session,
and diagnostics that omit content, endpoints, IDs, model IDs, and secret references.

The current `demo-provider` run reports `166 passed; 7 ignored` (the ignored cases require an
external OCR fixture, four client-certificate HTTPS fixtures, a third-party Ollama daemon, or a
private storage-fault mount). The dedicated
`tools/run-ocr-test.sh` and `tools/run-storage-fault-test.sh` runners each pass one exact test on a
host with the required tools and mount namespace; the third-party Ollama runner remains opt-in and
requires an installed model. Its worker tests validate the exact Core
compatibility contract including `long_text_chunking_v1`, prove that fake-service readiness does not auto-connect, require explicit
Connect and model selection, exercise real loopback HTTP/SSE streaming, consume an authenticated
session secret through the bounded typed host-secret broker, and fail closed for unavailable
session or persistent secrets. Persistence coverage creates two profiles with independent models,
restores the full list and active ID without provider requests, reconnects after explicit credential
re-entry, proves two credential values remain isolated, scans SQLite side files for both credential
and `session:` canaries, deletes inactive/missing/connected rows, keeps a deleted connected runtime
usable without recreating it, verifies exact `0700`/`0600` permissions, rejects a permissive parent,
symbolic ancestor, final database symlink, and hard-linked database without following unsafe paths,
and replaces the visible parent after `openat2(RESOLVE_NO_SYMLINKS)` while verifying the
descriptor-pinned Core migration remains in the original directory. It preserves every
restart row/default across session switches, failed persistent changes, and public connection
cancellation, and keeps session mode usable after storage initialization fails. A dedicated
preflight-race regressions replace the validated parent with a symlink or regular file between path
validation and descriptor opening, while companion final-component regressions replace the database
path with a symlink, distinct regular file, or hard link after preflight; a file created after a
missing-file preflight is also rejected by the exclusive open. All require rejection before an unsafe
descriptor is accepted. Existing SQLite `-wal` and `-shm` sidecars are inspected through the pinned
parent descriptor; `hard_linked_database_sidecars_are_rejected_without_modifying_targets` rejects
hard-linked aliases for both sidecars before Core opens the database, while
`replaced_database_sidecar_is_rejected_after_snapshot` verifies that a changed existing sidecar
identity is rejected after Core open. It also verifies that a completed standard translation is recorded in bounded
history, an Incognito completion bypasses both translation-memory lookup and persistence, and the
startup count/clear command path uses the same database. The focused
`incognito_translation_bypasses_existing_memory_and_persists_nothing` regression first stores a
standard result, then requires an Incognito repeat to reach the provider again while history and
translation-memory counts remain unchanged. A Linux-side
`gtk_incognito_translation_bypasses_memory_and_persistence` fixture also drives the production GTK
Incognito toggle, authenticated connection, model selection, and Translate action. It verifies the
standard request creates one history and one translation-memory entry, the identical Incognito
request reaches the loopback provider again, and both persisted counts remain at one.

The worker regressions also verify normalized usage persistence for provider-reported completions
and local translation-memory estimates, including policy gating, Incognito, restart visibility, and
deletion cleanup. Persisted records contain no source/output text, endpoint, credential, or pricing
data.
Scenario 5 regression authenticates and saves distinct providers A and B with independent models,
then uses one Connect action per remembered switch and proves each next translation reaches only
the newly confirmed provider. It rejects B with A's credential without changing the active B/model
pair, scans storage for both credentials and all secret references, and verifies the full
profile/model/default snapshot after restart. The suite also covers immediate connection
cancellation, translation cancellation with partial output, active,
queued, and full-command-queue shutdown, translation terminal delivery during shutdown,
delete rejection during translation, saved-model behavior, and failed-switch rollback to the
previous Core `ProviderManager` and model.

The isolated regressions are executed separately and must each pass exactly once:

```sh
bash tools/run-storage-fault-test.sh
bash tools/run-secret-service-test.sh
```

The runner compiles the exact library test as the calling user, enters a private mount namespace,
mounts an 8 MiB tmpfs, and fills it until the kernel returns `ENOSPC`. It prefers an unprivileged
user namespace. When Ubuntu restricts that mount, the CI fallback uses passwordless `sudo` only for
the private mount coordinator and drops back to the original UID/GID with `setpriv` before executing
the test binary; supplementary groups and inheritable, ambient, and bounding capabilities are
cleared, the environment is reset, and `no_new_privs` is enabled. It verifies failed persistent
model update, exact-ID deletion, and provider switch independently. Each operation must be rejected
before storage becomes unavailable, the previous engine/model must still translate, a subsequent
model change must remain session-only, and restart must restore only the pre-fault profile, default,
and model. The runner requires
`1 passed; 0 failed; 0 ignored` so a missing or skipped test cannot count as evidence, and cleanup
unmounts the private filesystem. This proves the implemented Linux `ENOSPC` transaction boundary;
the focused `read_only_database_directory_reports_error_but_session_mode_still_works` regression
additionally covers a non-writable private directory. Corrupt-database fail-closed behavior is
covered by the regular worker suite. Power loss and every SQLite VFS failure remain outside these
automated boundaries.

The Secret Service runner creates an isolated XDG data directory, starts a real `gnome-keyring`
Secret Service daemon on a private D-Bus session with a persistent `login` collection, stores and
resolves an item, locks the collection and verifies fail-closed lookup, stops and restarts the
daemon, then resolves and deletes the item before rerunning the cleanup round trip. It also runs the
worker secure-onboarding connect/translate/restart test and the GTK Remember/clear-form flow against
an authenticated loopback provider under Xvfb. It proves CRUD, persistent restoration, locked-item
handling, cleanup, and SecretRef-only persistence without touching a developer keyring. The GTK
flow also enters a bounded secret custom-header JSON value, verifies that a second persistent
SecretRef is created and that both sensitive fields clear immediately, scans SQLite artifacts for
both canaries, and deletes both Secret Service items during cleanup. The worker persistence
regression additionally proves that proxy-authentication and client-certificate SecretRefs are
retained only when persistent and that all three session-only SecretRefs are removed before a
profile can reach SQLite.

The prompted-flow runner starts a separate Python Secret Service fixture four times. It returns a
non-root prompt path from `CreateItem` and `Delete`, then exercises both completion outcomes: an
approved prompt completes the store/delete operation, while a dismissed prompt fails closed with
`SecureStorageUnavailable` and the stable interactive-prompt message:

```sh
bash tools/run-secret-service-prompt-test.sh
```

This proves the adapter's prompt signal handling and fail-closed boundary through a private D-Bus
fixture; it does not claim that a real user approved or visually reviewed a desktop prompt. End-user
prompt acceptance and unlock UX remain separate manual validation gates. The GTK connection flow's
localized session-only recovery dialog is covered by the serialized
`gtk_secret_storage_fallback_dialog_requires_explicit_session_only_action` fixture in Native CI:
the dialog keeps Remember enabled until the user activates the explicit session-only action, then
clears Remember while the credential field remains focusable; closing the dialog leaves the choice
unchanged. The production callback requests focus on that field, while the exact active-window
focus owner remains a window-manager concern. This is UI lifecycle evidence only; physical prompt
approval and visual review remain manual.

The localization unit suite parses every official Linux MO catalog, checks action entries are
available, and verifies unique BCP 47 tags plus Arabic RTL metadata:

```sh
cargo test --features gui --lib localization::tests --locked
```

The checked-in PO sources must pass gettext syntax validation, and the generated MO table must be
readable by gettext tooling:

```sh
for file in l10n/linux/*/LC_MESSAGES/linguamesh.po; do
msgfmt --check --check-format -o /dev/null "$file"
done
msgunfmt l10n/linux/zh-Hans/LC_MESSAGES/linguamesh.mo >/dev/null
```

The opt-in image-only PDF path is validated with a private generated fixture. It requires
ImageMagick, Poppler, and Tesseract and runs the ignored external-plugin test explicitly:

```sh
bash tools/run-ocr-test.sh
```

The fixture proves page text is recovered through the bounded `pdftoppm`/`tesseract` boundary;
ordinary tests keep OCR disabled and cover unavailable-tool, malformed-PDF, and safety-limit paths.

The notification transport runner starts a private `org.freedesktop.Notifications` fixture service and
captures the real GTK translation test's `Notify` call on a private D-Bus session. It requires fixed
generic title and body text and absence of the source and translated strings. This proves the
application-to-notification-service transport and privacy boundary, not desktop-shell rendering.

The notification daemon runner starts the real `dunst` server under Xvfb, waits for it to own the
session notification name, runs the same GTK translation flow, verifies the daemon receives the
generic redacted payload, and checks that a visible, viewable Dunst desktop-shell window exists:

```sh
bash tools/run-notification-daemon-test.sh
```

This proves the X11 desktop-shell rendering path in the headless CI display; it does not prove a
physical compositor, GPU behavior, or visual placement on every desktop environment.

The document portal runner starts the real XDG document portal services on a private D-Bus session,
adds a temporary fixture through a file descriptor, verifies the returned host path, grants and
revokes the application read permission, and deletes the lease. This proves the document-portal
lease lifecycle without touching a developer file; application-level chooser and drag/drop callbacks
are exercised by the dedicated GTK fixture below.

The interactive file chooser runner starts the real `xdg-desktop-portal-gtk` backend under Xvfb,
issues the actual `FileChooser.OpenFile` request, injects a fixture path into the visible chooser,
and verifies the response URI and UTF-8 contents. This proves backend portal UI and lease behavior;
the Linux application's GTK `FileDialog` callback is verified by the application-level fixture:

```sh
bash tools/run-gtk-file-chooser-portal-test.sh
```

That runner launches the real binary with a hidden test environment, drives the visible chooser with
`xdotool`, and asserts the callback plus asynchronous GIO read markers.

The toolkit-independent suite also tests the text-import decoder for UTF-8 BOM removal, invalid
UTF-8 rejection, and the 4 MiB bound. The native GTK flow verifies the **Open text file** control
is focusable, is disabled after worker loss, and registers URI-list/GIO `DropTarget` types on the
source editor. The application-level drag fixture performs an actual XTest drag through the editor:

```sh
bash tools/run-gtk-drag-and-drop-test.sh
```

The fallback-enabled ordinary-text path also requires a localized confirmation window before dispatch;
the dialog's **Translate** action grants one request and **Close** sends nothing. Physical
desktop-shell rendering and prompted portal/keyring approval UI remain manual boundaries.

The GTK Rust source can be checked without native linking as a limited diagnostic. The `v4_10`
gtk-rs feature is enabled because the accessibility test helpers and semantic update APIs require
GTK 4.10 or newer:

```sh
DOCS_RS=1 cargo check --all-targets --all-features --locked
DOCS_RS=1 cargo clippy --all-targets --all-features --locked -- -D warnings
```

## Flatpak metadata validation

The primary Linux packaging scaffold is kept in `packaging/flatpak`. It pins the Linux and Core
source commits, vendors every registry dependency from `Cargo.lock`, installs the desktop entry,
AppStream metadata, and uses Wayland with practical X11 fallback, Secret Service, notifications,
network access, and only the application data directory. Validate the checked-in metadata with:

```sh
bash tools/validate-flatpak-metadata.sh
```

This check parses the manifest and Cargo source set, validates 40-character source pins and SHA-256
archives, and runs `desktop-file-validate` plus `appstreamcli`. The `Flatpak Linux` workflow runs
the pinned manifest in the GNOME 49 SDK container, uploads a prerelease CI bundle, and runs
`tools/run-flatpak-smoke.sh` under Xvfb with a private D-Bus session. That smoke only proves
installation and bounded application startup; it does not publish a release or prove interactive
file-chooser portal leases or physical desktop-shell notification rendering. `flatpak-builder` is not installed on this host, so local SDK
build and sandbox launch remain unavailable.

After the SDK build, the workflow runs `tools/create-flatpak-evidence.py` to emit a `SHA256SUMS`
sidecar and deterministic SPDX 2.3 SBOM from the bundle and locked Cargo dependency set. These
uploads are CI-only evidence and are not a stable release or signature.

The native workflow also builds the GTK binary in release mode and uploads it with deterministic
integrity sidecars. To reproduce the generator locally after a successful native build:

```sh
cargo build --release --bin linguamesh-linux --features gui --locked
mkdir -p native-evidence
cp target/release/linguamesh-linux native-evidence/linguamesh-linux
python3 tools/create-native-evidence.py \
  --binary native-evidence/linguamesh-linux \
  --cargo-lock Cargo.lock \
  --output-dir native-evidence \
  --linux-revision "$(git rev-parse HEAD)" \
  --core-revision "f5b818c3598d78e7cac30604577fa8057d380737" \
  --localization-revision "c2526bfb3f6ff57895bdc3eeed743e26c8783613"
(cd native-evidence && sha256sum -c SHA256SUMS)
```

The workflow also adds `linguamesh-linux-source.tar.gz` to the same artifact and appends its digest
to `SHA256SUMS`. This is a repository-only source snapshot; it still requires the pinned Core and
localization repositories for a build. The binary, source archive, `SHA256SUMS`, `SBOM.spdx.json`,
and `BUILD-INFO.txt` are unsigned prerelease evidence only, not a stable or distributable release.
Before either evidence directory is uploaded, Native and Flatpak CI re-check every listed SHA-256
entry and parse the SPDX JSON document. This catches a corrupt or incomplete evidence bundle before
it becomes a retained CI artifact; it does not provide a signature or stable-release authorization.
The generated `ROLLBACK.md` records the exact source pins and future signed-release rollback
sequence without inventing a previous stable revision.

The Native workflow also runs `tools/run-performance-baseline.sh` for representative DOCX
reconstruction, XLSX reconstruction, and saved-profile routing dispatch tests. It records the
kernel, CPU, memory, Rust, Core, and localization context with elapsed seconds in
`LINUX-PERFORMANCE-BASELINE.tsv`. These are machine-specific trend baselines and must not be quoted
as portable performance numbers.

These commands bypass sys-crate discovery and do not validate headers, ABI, linking, launch, or
display behavior. Their cached sys-crate output can also make a later ordinary Cargo check look
successful. The `pkg-config` commands below are therefore mandatory native-gate prerequisites;
run `cargo clean -p graphene-sys` before rechecking native discovery after a source-only check.
Do not extend this shortcut to `cargo test --no-run`: the GTK binary test still requires native
linking.

## Native GUI validation

On Debian or Ubuntu, install the native development headers:

```sh
sudo apt-get install \
  libgtk-4-dev libadwaita-1-dev dbus-daemon gettext gnome-keyring mount pkg-config python3 util-linux weston \
  xauth xvfb
```

Then run the complete native gate:

```sh
pkg-config --modversion gtk4
pkg-config --modversion libadwaita-1
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
GDK_BACKEND=x11 dbus-run-session -- xvfb-run --auto-servernum \
  --server-args="-screen 0 1280x800x24" \
  cargo test --all-targets --all-features --locked -- --test-threads=1
bash tools/run-storage-fault-test.sh
dbus-run-session -- bash tools/run-wayland-test.sh
cargo build --all-targets --all-features --locked
cargo build --release --bin linguamesh-linux --features gui --locked
```

Run the development slice with:

```sh
cargo run --features gui
```

The all-feature binary test creates real GTK/libadwaita widgets, verifies the initial disconnected
state and the Provider setup progression from Starting through Configure, Connecting, Select model,
and Ready, then checks that Ready identifies the confirmed provider stable ID/model. It verifies
that pending model confirmation remains in Step 2 and fatal worker shutdown becomes Unavailable
instead of remaining at Starting, with Connect, model, translation, and stop controls disabled. It
waits for fake-endpoint readiness without auto-connect, clears a session credential from the form
immediately after Connect, explicitly selects a discovered model, preserves the active
provider/model and Ready identity after a failed switch, and completes a streamed translation. The
The dedicated `gtk_one_click_provider_switch_uses_new_session_and_isolates_credentials` fixture
restores two saved rows, connects A, translates once, selects B, and connects once more. It proves
that the next request reaches only B, the old provider receives no additional inference, the active
provider remains A while B is only displayed, and both one-shot credential fields are cleared.
The same GTK binary regression selects the native Ollama preset, connects to the deterministic
`/api/` fixture, verifies `ollama_chat` model discovery, and translates `你好，Ollama！` without a
credential. A completed translation also exercises the registered application notification path; its payload is
  localized generic copy and does not contain source or translated content. It
also verifies the storage-unavailable session-only warning persists in Ready, injects a two-profile
startup snapshot, verifies persisted-active prefill without activation, browses another row without
changing the runtime/default, checks the disconnected storage warning, preserves a
persistent secret reference so the real Connect path fails closed when the test desktop has no
matching Secret Service item, rejects a disabled saved row
without re-enabling it, checks delete-pending control blocking, applies an exact deletion result,
and verifies a fresh random draft ID. The test runs once in a private D-Bus session under X11/Xvfb,
then the binary-only suite runs again in another private D-Bus session under forced Wayland and
headless Weston. `tools/run-wayland-test.sh` creates a private `0700` runtime directory, unsets
`DISPLAY`, sets `GDK_BACKEND=wayland`, waits at most ten seconds for a dedicated socket, and uses
exit traps to stop Weston and remove the directory. Tests are serialized because GTK owns
process-global state. The same test asserts the baseline accessibility roles, editor properties,
visible-label relations and mnemonics (including the fallback-provider dropdown), focusability, explicit Stop name, hidden-empty-error
behavior, and Busy-state reset. The test also switches the runtime locale to Simplified Chinese and
verifies the catalog-backed Translate and Stop labels, then switches to Arabic and verifies RTL
direction without replacing the source editor buffer before restoring English. GTK's helpers prove
semantic presence and reset behavior. The dedicated `tools/run-gtk-keyboard-focus-test.sh` also runs
the real binary under Xvfb and `xfwm4`, activates the Provider preset through its `Alt+P` mnemonic,
then injects Tab/Shift+Tab events and asserts focus events for the tested onboarding and workspace
controls. Native CI repeats this real-binary fixture with
`LINGUAMESH_KEYBOARD_FOCUS_LOCALE=ar` and requires the production workspace to report RTL before
asserting the same focus traversal. The application-window Capture-phase handler keeps the provider
fields in an explicit Tab/Shift+Tab order while preserving modified shortcuts.
`tools/run-gtk-atspi-test.sh` starts
the AT-SPI bus, reads the live accessibility tree with `python3-pyatspi`, and verifies the named
Open, Translate, Retry, fallback-consent, and Stop controls with their expected roles, plus two
exported text-editor roles. The GTK unit test also verifies that document
progress uses the native progress-bar role, exposes a bounded completed/total fraction, and hides
the progress control when no document job is selected. This proves AT-SPI semantic export only; it does
not prove Orca speech, physical-compositor behavior, RTL presentation, or GPU rendering. Native CI runs the same fixture once with English defaults and once with
`LINGUAMESH_TEST_LOCALE=zh-CN`; the second run checks the catalog-backed Simplified Chinese names
for Open, Translate, Retry, fallback consent, and Stop without weakening the role assertions. Native
CI also runs `LINGUAMESH_TEST_LOCALE=ar` and checks the Arabic Open/Translate/Stop names plus the
catalog's English fallback names for Retry and fallback consent. The
diagnostics panel uses the catalog-backed `diagnostics.summary` template for its
Core ABI/protocol header, localizes fixed labels and state values through the Linux diagnostics
keys, and keeps source content, endpoints, identifiers, and secret references redacted.

`tools/run-orca-atspi-test.sh` adds the installed Orca process to a separate Xvfb/private-D-Bus
session. `tools/orca-atspi-inspect.py` finds the production Stop push button through AT-SPI and
confirms its focusable state; the fixture then requires Orca's debug stream to contain the Linux
application tree and a `SPEECH GENERATOR` record. Native CI runs the default English fixture and a
second `LINGUAMESH_TEST_LOCALE=ar` fixture that resolves the Arabic Stop name. This proves headless
Orca integration and speech-generation dispatch for English; the Arabic run proves the localized
AT-SPI tree and focus path only because the CI speech backend does not provide stable locale-specific
speech output. Neither run replaces a human listening review, physical desktop review, or a claim
about speech quality across locales.

`tools/run-gtk-accessibility-preferences-test.sh` runs the serialized GTK component test in a
private Xvfb and DBus session. It applies a temporary `HighContrast` GTK theme and disables
`gtk-enable-animations`, and sets the process-local GTK font to `Sans 24`. It then asserts that
libadwaita detects high contrast and reduced motion and that the title's Pango context receives the
larger font size. The fixture restores the theme, animation, and font settings before exit; it does
not modify the developer's desktop preferences. This verifies the Linux client's system-supported
contrast, motion, and text-scaling behavior; manual visual review remains required for supported
releases.

The GitHub Actions native workflow pins Core revision
`f5b818c3598d78e7cac30604577fa8057d380737` and localization revision
`c2526bfb3f6ff57895bdc3eeed743e26c8783613`, installs the headers plus D-Bus, Xvfb, test-only
mount-namespace tools, and Weston support, and runs the real storage write-fault gate and both
display gates before the all-feature build. The storage write-fault change passes its exact local
namespace test through the unprivileged path.

Before the Linux client tests, Native CI runs `bash tools/verify-linux-sdk-package.sh` from the
checked-out Core tree at that exact revision. The verifier builds the Linux SDK twice in release
mode, compares the complete archive SHA-256, checks every packaged file, validates the pkg-config
metadata, and compiles a static C consumer against the packaged FFI library. The result is
reproducible coordination evidence for the pinned Core contract; it is not a signed or published
release artifact.

The GTK AT-SPI fixture bounds cleanup of its private application, window manager, and accessibility
launcher processes; a fixture that prints successful accessibility assertions but cannot reap its
processes is recorded as a failed gate rather than accepted as evidence.
The current Linux diagnostics localization revision `026c35b8dbb1c13c22d77809cc5fe72e6af6f5a3`
contains 422 catalog messages; the source-level catalog
audit and runtime locale tests cover the diagnostics labels. The current fixed-error localization revision `b6d2503`
passed Native Linux run `29627668119`, Foundation run `29627668093`, and
Flatpak run `29627668108`; the native job validated the pinned 117-message catalog and GTK
Simplified Chinese provider-card/source-target assertions in addition to the existing storage,
Secret Service, portal, notification, drag/drop, X11, and forced-Wayland gates. The workflow
checks out l10n revision `0ba26705e113230ae7d9e74db54039e1e82296ce`.
The current reducer-state localization revision `9f21836f214d3056934fac9322adc0f20791834e`
passed Native Linux run `29628307915`, Foundation run `29628307945`, and Flatpak run
`29628307886`; the native job validated the pinned 148-message catalog, localized fixed reducer
errors and category prefixes, and the existing storage, Secret Service, portal, notification,
drag/drop, X11, and forced-Wayland gates. The workflow checks out l10n revision
`08118b498646ebf56cbb072b937d95fceb34b75c`.
The MO integration revision `6c5bfb305967d0f01488ad09ade6e5b88eebbdb0` passed Native Linux run
`29628986188`, Foundation run `29628986160`, and Flatpak run `29628986187`; the workflow validates
both PO syntax and MO readability before the runtime GTK gates and checks out l10n revision
`0b906034784a1b5e81a879649abbfda001fa9e67`.
The current locale-selector coverage revision is validated locally with 184 catalog messages and
l10n revision `d21c3b0d065831b20cf31c9bf3009ffd262e4797`; it adds localized names for all twelve
official interface packs and refreshes those labels during runtime locale switching. The same
revision includes fixed translations for invalid UTF-8 import, storage fallback, provider/model
state, secret-channel, and profile validation failures before the native CI gate.
Runtime-storage functional revision
`c37702c76c3b1a2f9cec805cf9e219721ef7b5ce` passed Native Linux run `29586532049` (job
`87904787338`): strict all-feature Clippy, 66 ordinary library tests with one intentional ignore,
the real GTK binary test under X11/Xvfb, the exact storage-fault test with 1 pass and 0 ignored via
the controlled `sudo` fallback, the same GTK test under forced Wayland/headless Weston, and the
all-target all-feature build all succeeded. Repository-foundation run `29586531915` (job
`87904787120`) also passed. Wayland-gate revision
`10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed Native Linux run `29582513061` (job
`87891382469`): strict all-feature Clippy, 65 library tests, the real GTK binary test under
X11/Xvfb, the same test under forced Wayland/headless Weston, and the all-target all-feature build
all succeeded. Repository-foundation run `29582513073` (job `87891382540`) also passed. The
preceding functional onboarding revision
`9729b23ce1a4280ebb434339e880010103b4859d` passed Native Linux run `29580444723` (job
`87884607879`): strict all-feature Clippy, 65 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded under the then-current X11 gate. Repository-foundation
run `29580444697` also passed.
The preceding functional multi-profile revision
`c88d37a5de2f03c2ae5d2940c4d25e5d998c301d` passed Native Linux run `29577918335` (job
`87876528763`): strict all-feature Clippy, 62 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded. Repository-foundation run `29577918346` also passed.
The preceding functional persistence revision
`c58a54c2479045773358bd9c456b45a958e98e1e` passed Native Linux run `29574265570` (job
`87865028892`): strict all-feature Clippy, 50 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded. Repository-foundation run `29574265553` also passed. A
local host without `gtk4.pc` or `libadwaita-1.pc` must not substitute this remote result for an
unexecuted local GUI build, launch, or GTK test.

Accessibility functional revision `d6bd2bd06ccdf04f3aead0c7f1da5ba74f84c550` passed repository
foundation run `29589043314` (job `87913221612`) and Native Linux run `29589043315` (job
`87913221576`). The native job passed the accessibility assertions described above together with
strict Clippy, 66 ordinary library tests and one intentional ignore, the exact storage-fault gate,
both display gates, and the all-target all-feature build. Its predecessor `e483ad8` failed at the
first focusability assertion because GTK dropdowns defaulted to non-focusable; the final revision
sets every labelled control and action explicitly focusable.

The history-policy checkpoint is Core `fb00f3dd6b62a8a3a47350acc85831e60e266929`, Linux
`7173d4a4217d6211c7dc92c368d9f033874198f5`, and l10n `40f3914e1b28fddd8f38d287fa121010f5192f1c`.
Local Core workspace tests, Clippy, offline build, and cargo-deny passed; Linux passed all-target
Clippy, 54 portable tests, and 82 demo-provider tests with one intentional ignore; l10n validation
passed with 244 messages. The Linux GUI test remains CI-linked because the local GTK libraries
cannot link the current GTK 4.10 symbols.
Core and l10n remote validation for this checkpoint is recorded after their pushes; local Core
storage tests passed 15 cases and Linux passed the policy worker regression. The Linux GUI test
remains CI-linked because the local GTK libraries cannot link the current GTK 4.10 symbols.
The Linux translation-memory controls are covered by the current worker/storage slice; remote
native and Flatpak evidence for this revision is recorded after the push.

## Repository foundation check

```sh
set -euo pipefail
required_files="README.md LICENSE AGENTS.md REPOSITORY_ROLE.md GLOBAL_GOAL.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md THIRD_PARTY_NOTICES.md IMPLEMENTATION_STATUS.md Cargo.toml Cargo.lock deny.toml rust-toolchain.toml rustfmt.toml src/lib.rs src/model.rs src/worker.rs src/main.rs docs/architecture.md docs/testing.md docs/releasing.md tools/sync-l10n.sh tools/run-wayland-test.sh tools/run-storage-fault-test.sh l10n/compatibility.json l10n/manifest.json .gitignore .github/workflows/foundation.yml .github/workflows/native.yml"
for file in $required_files; do
  test -s "$file" || {
    printf 'Missing required file: %s\n' "$file"
    exit 1
  }
done
grep -Fqx 'Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`' GLOBAL_GOAL.md
if find . -type f \( -name '*.md' -o -name '*.yml' \) -not -path './.git/*' -exec awk '/[[:blank:]]$/ { printf "%s:%d: trailing whitespace\n", FILENAME, FNR; bad=1 } END { exit bad }' {} +; then
  printf '%s\n' 'Foundation validation passed.'
else
  exit 1
fi
git diff --check
```

The current Linux runtime/storage-error localization slice adds catalog-backed startup,
compatibility-read, and profile-database path/permission errors. Its portable regression test
asserts Simplified Chinese translations while preserving safe dynamic diagnostic detail.

## Remaining validation before a supported release

The automated Linux slice now covers the main GTK/AT-SPI semantic tree, keyboard focus, headless
Orca integration, portal and Flatpak smoke paths, catalog key/placeholder invariants, the
`cargo-deny` advisory/license/source policy, and the implemented storage transaction boundary.
The storage regressions cover parent-directory replacement with a symlink or regular file and
final-database-component replacement with a symlink, distinct regular file, or hard link through
descriptor-pinned `openat2`/`O_NOFOLLOW` opens; a missing final leaf is created only through an
exclusive open. Existing SQLite `-wal`/`-shm` sidecars are checked through the pinned parent and
hard-linked aliases are rejected before Core opens the database; existing identities are checked
again after Core open. The preflight suite also replaces the validated parent with a distinct
private directory and rejects the device/inode change. Replacement after the second sidecar
inspection, broader same-UID filesystem/VFS variants, and power loss remain outside the tested
boundary.
Remaining evidence is deliberately explicit: human screen-reader listening and translated-copy/
RTL/visual review; physical compositor, GPU-backed Wayland, and broader X11/desktop coverage;
prompted interactive Secret Service approval; broader filesystem/VFS and power-loss races; signed
distributable artifacts and stable-release authorization; and the other native clients. These gaps
keep the Linux branch prerelease even though the listed automated gates are green.
