# Releasing

## Current state

An unreleased native application target and a pinned Flatpak packaging scaffold now exist. The
GNOME 49 SDK workflow builds a prerelease CI bundle from pinned sources and runs the bounded
Xvfb/private-D-Bus sandbox smoke. The native gate verifies the real document-portal lease lifecycle,
the real interactive `xdg-desktop-portal-gtk` FileChooser backend, the application-level GTK
FileDialog callback, a real source-editor drag/drop gesture, and visible desktop-shell delivery to a
real `dunst` notification daemon under Xvfb, and now exercises headless Orca AT-SPI speech-generation
dispatch in its isolated X11 session. It still has no physical compositor/GPU rendering, signed artifact, or
distributable release has been verified. The
vertical slice must not be tagged or published as a product release, and no packaging claim beyond
the recorded CI build is valid. Native CI now uploads the release-mode Linux binary together with
SHA-256, deterministic SPDX 2.3, build-context, repository-only source-archive, machine-specific
performance-baseline, and exact-pin `ROLLBACK.md` sidecars; these files remain unsigned CI evidence
and are not distributable release artifacts. `ROLLBACK.md` records an actionable future rollback
sequence without inventing a previous stable revision. The source snapshot still requires the pinned
Core and localization repositories
for a build. Its bundled fake provider is development-only behavior. The optional
OpenAI-compatible endpoint form accepts a one-shot session credential, clears the field
immediately, and never persists the credential value. A saved-profile dropdown and explicit remember
checkbox can create, update, activate, switch, and remove multiple rows containing provider names,
endpoints, model preferences, and persistent Secret Service references in the XDG user data SQLite database. The
private application directory is `0700` and the database is `0600`. Removing the connected row
leaves its already validated runtime session active but no longer persistent. Core's `synchronous=FULL`
WAL durability and no-follow
SQLite open behavior on Linux's default Unix VFS remains required. Startup prefills the last
persistently activated row but never auto-connects, so a credential must be entered again when
required. A derived Provider setup card guides configuration, explicit connection, and deliberate
model selection without storing a completion flag, distinguishes worker failure from startup, and
shows the confirmed next-request stable ID/model identity.
The routing-profile dialog now persists an explicit Core mode (`Manual`, `Ordered`, or `Automatic`);
approved fallback remains a separate opt-in checkbox that defaults off, and candidate checkboxes limit
the saved provider/model pairs included in a profile. Manual mode persists exactly the first selected
provider/model pair; Ordered and Automatic preserve the selected chain. Adjacent up/down controls and row drag-and-drop
set their Ordered-mode sequence and expose catalog-backed accessible labels. Existing profiles can
be edited and saved through the same stable ID. New profiles use Core-compatible 1–128 byte ASCII
IDs, and edit mode locks the existing ID so release references do not drift. The automated Linux
candidate-management boundary is covered by the serialized GTK lifecycle fixture and worker routing
tests; this remains prerelease evidence until visual, translated-copy, end-user Orca, cross-client,
signing, rollback, and stable-release review is complete.
The serialized GTK lifecycle regression now proves edit-mode ID locking plus candidate deselection,
save, worker list, and editor reload persistence. This remains prerelease test evidence until the
required visual, translated-copy, and end-user Orca review is recorded.
It also uses and deletes the selected record, applies the typed deletion event, verifies selection
cleanup, and confirms the worker list refreshes empty; the Native and Flatpak gates remain the
release evidence for this display-backed path.
The external-provider path includes deterministic Ollama-compatible OpenAI `/v1/`, native
`/api/`, and Gemini `/v1beta/` loopback fixtures plus a passed opt-in third-party daemon regression
using Docker `ollama/ollama:0.11.10` and `qwen2.5-0.5b-instruct:latest`. The GTK form exposes the
native Ollama preset and its `ollama_chat` adapter plus the Anthropic Messages preset with a manual
Model ID field, the Gemini Generate Content preset with discovered models, and the Azure OpenAI
preset with a manually entered deployment and pinned API version, plus the OpenAI Responses preset
with discovered models and typed SSE streaming,
while persistent
secret references use the Linux GIO Secret Service adapter and fail closed when the desktop keyring
is unavailable or requires an interactive prompt. The native workflow
pins reviewed Core functional revision
`072d6b92df875153a60a9d1256ab814891fe775b`, whose Core delta adds bounded document lease
smoke and AddressSanitizer gate in addition to the protocol decoder fuzz gate and bounded FileLease lifecycle
and engine-scoped ABI lease controls plus Unix POSIX-descriptor document consumption; Android and
Windows handle transfer remain open. Its storage delta adds
`SQLITE_OPEN_NOFOLLOW`, adds the trusted `/proc/self/fd/<fd>` descriptor path for hosts that pin a
private inode, rejects suspicious OOXML compression ratios and unsupported macro/signature
parts before XML inspection, and whose
text path adds protected-span, request-level glossary, and bounded long-text restoration. Its storage
tests also cover replaying a committed provider profile from a WAL sidecar after a reader snapshot
and writer disconnect. The
bounded SRT/WebVTT/CSV/JSON/HTML document contract, rather than checking
out a floating branch. Functional revision
`7d7eba9960b657f0460fb0daaaaebaaa609f39b1` passed Native Linux run `29604269568` (job
`87963611054`) and repository-foundation run `29604269516`; it includes the no-credential
OpenAI-compatible loopback regression, secure onboarding fixtures, strict Clippy, both display
gates, and the all-feature build. Earlier functional revision
`9729b23ce1a4280ebb434339e880010103b4859d` passed Native Linux run `29580444723` (job
`87884607879`) with 65 library tests and the first real GTK flow. Wayland-gate revision
`10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed Native Linux run `29582513061` (job
`87891382469`) with the same GTK binary test succeeding under both X11/Xvfb and forced
Wayland/headless Weston. Neither validation creates a distributable artifact or satisfies the
future release gate below.

The current Linux gate consumes Core `8623b2c8829e4d9cf7299c74440dcfabb4e320db` and l10n
`c2526bfb3f6ff57895bdc3eeed743e26c8783613` (506 catalog messages). The reviewed Flatpak source
pin is Linux `7513d983011fdd81374cfb879b23647aef388f7e`; the current packaging pin is the same
commit. Local exports now synchronize the temporary file and parent directory before the atomic
move, then synchronize the parent again after finalization; a serialized child-process
interruption fixture also verifies that the final destination is absent while the synced temporary
bytes remain inspectable after SIGKILL. This is bounded process/crash-durability evidence, not
physical power-loss or alternate-VFS validation. The Provider Hub displays the
selected saved profile's last health-check timestamp or normalized failure category using the
same catalog; raw provider diagnostics and credentials remain excluded. The profile contract includes
a bounded total provider request timeout of 1–600 seconds, a bounded connection-establishment
timeout of 1–120 seconds (default 10), and a bounded streaming-idle timeout of 1–300 seconds
(default 60). Optional PEM trust bundles augment system roots without disabling TLS verification;
malformed bundles are rejected before transport construction. The environment-gated Linux
client-certificate fixtures create trusted and untrusted temporary CAs, a trusted-CA wrong-SAN
endpoint, and a server with a different client-CA trust chain, and Native CI runs the worker's real
`/v1/models` discovery against all four cases. This validates certificate wiring, server-side
client authentication, configured trust-bundle enforcement, hostname verification, and rejection
of an untrusted client identity; generated keys are deleted after the tests, and no enterprise
endpoint, signed artifact, or stable release is implied. Normalized
usage
labels
distinguish provider-reported, locally estimated, and unknown counts without pricing assumptions;
provider billing equivalence and stable ABI projection remain open. Request-level glossary rules,
bounded CSV/TBX interchange, and persistent glossary libraries are implemented in the Linux slice;
cross-client parity, live provider accounts, physical VFS/power-loss behavior, signing, rollback,
and stable-release authorization remain outside the release claim.

The Linux document slice now persists bounded TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF jobs, translates pending prose segments
sequentially, and restores completed or cancelled snapshots into the editor. Validated non-secret
provider/model/glossary options are persisted and reused by Resume and Retry after restart only when
the active runtime matches. The queue listing, explicit job selection, and document-job execution
path are covered by the current Native/Flatpak validation gate; bounded concurrent document
execution (up to four jobs with per-job cancellation isolation) is included in the Linux evidence,
but the release remains prerelease. DOCX/PPTX/XLSX/EPUB export preserves
non-text package resources and rewrites only supported OOXML text parts; malformed, encrypted, traversal,
DTD-bearing, or over-limit packages are rejected. EPUB also requires a first `mimetype` entry,
preserves navigation and resources, translates XHTML/HTML text, and updates OPF language metadata.
Text PDFs retain page association and available coordinates; reliable ASCII streams are rewritten in
place, while unsupported encodings use a page-aware HTML alternative. Image-only PDF OCR is an
optional Linux development capability: it requires separately installed `pdftoppm` and `tesseract`,
is explicitly enabled by the user, and produces page-marked TXT rather than a reconstructed PDF.
It is not a pixel-identical output claim and must not be enabled by packaging defaults.
The Linux UI surfaces Core's bounded page-level warnings for those limitations and uncertain reading
order without treating them as OCR or pixel-fidelity guarantees.
Subtitle imports retain cue IDs and timestamps and surface configurable line-length and reading-speed
warnings; timing is never rewritten automatically.

The current native gate also includes a real post-startup `ENOSPC` regression for persistent model
updates, profile deletion, and provider switching. Runtime-storage functional revision
`c37702c76c3b1a2f9cec805cf9e219721ef7b5ce` passed Native Linux run `29586532049` (job
`87904787338`) and repository-foundation run `29586531915` (job `87904787120`). Ubuntu exercised
the controlled mount fallback and proved exact rejection, continued use of the prior session,
session-only post-fault model selection, and restart recovery of only pre-fault state. The regular
worker suite also covers corrupt-database and non-writable-directory fail-closed behavior. This is
not evidence for power-loss recovery or every storage-failure path.

Linux profile startup inspects existing SQLite `-wal` and `-shm` sidecars through the pinned parent
descriptor and rejects symbolic-link, non-regular, or hard-linked aliases before Core opens the
database. The parent descriptor and existing sidecar identities remain pinned through Core open;
the sidecars are checked again afterward and changed identities fail closed. Replacement after that
second inspection and non-default SQLite VFS behavior remain unverified release boundaries.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. real desktop Secret Service CRUD/cleanup, the session-only fallback, complete SecretRef-backed profile lifecycle, multi-profile management, bounded native text import (including source-editor drag-and-drop), XDG paths, document-portal leases and interactive file workflows, accessibility, Wayland, practical X11 support, desktop notification delivery, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials;
   the native binary and Flatpak evidence sidecars are promoted only after signing, source archive,
   rollback, and release-manifest checks are complete.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
