# Testing and Validation

## Host prerequisites

Rust 1.93.0 is pinned by `rust-toolchain.toml`. A sibling `../linguamesh-core` checkout is required
because the client deliberately uses typed path dependencies instead of copying shared behavior.
Its functional source must match approved revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`. This revision changes file-backed Core storage to add
SQLite's `SQLITE_OPEN_NOFOLLOW` flag and includes its Unix symlink-rejection regression test. On
Linux's default Unix VFS, any symbolic-link path component is rejected. A clean documentation-only
descendant is acceptable
for local path builds when the compiled source tree is unchanged; validate it with:

```sh
git -C ../linguamesh-core cat-file -e fbf3e9b5927049dccaa19f8c36013495ffebba12^{commit}
git -C ../linguamesh-core diff --quiet \
  fbf3e9b5927049dccaa19f8c36013495ffebba12..HEAD -- \
  Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml crates assets migrations
test -z "$(git -C ../linguamesh-core status --porcelain)"
```

A sibling `../linguamesh-l10n` checkout at the revision pinned by `tools/sync-l10n.sh` is required
to verify the checked-in PO catalogs.

Validate localization provenance and gettext syntax with:

```sh
bash tools/sync-l10n.sh --check
for file in l10n/linux/*/LC_MESSAGES/linguamesh.po; do
  msgfmt --check --check-format -o /dev/null "$file"
done
```

Toolkit-independent validation requires no GTK development headers:

```sh
cargo fmt --all --check
cargo check --all-targets --features demo-provider --locked
cargo clippy --all-targets --features demo-provider --locked -- -D warnings
cargo test --no-default-features --locked
cargo test --features demo-provider --locked
cargo build --features demo-provider --locked
```

The no-default suite contains 41 reducer tests. It covers the disconnected initial state, atomic
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

The ordinary `demo-provider` run passes 66 tests and reports one intentionally ignored namespace
test. Its worker tests validate the exact Core
compatibility contract, prove that fake-service readiness does not auto-connect, require explicit
Connect and model selection, exercise real loopback HTTP/SSE streaming, consume an authenticated
session secret through the bounded typed host-secret broker, and fail closed for unavailable
session or persistent secrets. Persistence coverage creates two profiles with independent models,
restores the full list and active ID without provider requests, reconnects after explicit credential
re-entry, proves two credential values remain isolated, scans SQLite side files for both credential
and `session:` canaries, deletes inactive/missing/connected rows, keeps a deleted connected runtime
usable without recreating it, verifies exact `0700`/`0600` permissions, rejects a permissive parent,
symbolic ancestor, and hard-linked database without following static unsafe paths, preserves every
restart row/default across session switches, failed persistent changes, and public connection
cancellation, and keeps session mode usable after storage initialization fails. A Linux-side
Scenario 5 regression authenticates and saves distinct providers A and B with independent models,
then uses one Connect action per remembered switch and proves each next translation reaches only
the newly confirmed provider. It rejects B with A's credential without changing the active B/model
pair, scans storage for both credentials and all secret references, and verifies the full
profile/model/default snapshot after restart. The suite also covers immediate connection
cancellation, translation cancellation with partial output, active,
queued, and full-command-queue shutdown, translation terminal delivery during shutdown,
delete rejection during translation, saved-model behavior, and failed-switch rollback to the
previous Core `ProviderManager` and model.

The ignored regression is executed separately and must pass exactly once:

```sh
bash tools/run-storage-fault-test.sh
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
it does not cover read-only media, corruption, power loss, or every SQLite VFS failure.

The GTK Rust source can be checked without native linking as a limited diagnostic. The `v4_10`
gtk-rs feature is enabled because the accessibility test helpers and semantic update APIs require
GTK 4.10 or newer:

```sh
DOCS_RS=1 cargo check --all-targets --all-features --locked
DOCS_RS=1 cargo clippy --all-targets --all-features --locked -- -D warnings
```

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
  libgtk-4-dev libadwaita-1-dev dbus-daemon gettext mount pkg-config python3 util-linux weston \
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
provider/model and Ready identity after a failed switch, and completes a streamed translation. It
also verifies the storage-unavailable session-only warning persists in Ready, injects a two-profile
startup snapshot, verifies persisted-active prefill without activation, browses another row without
changing the runtime/default, checks the disconnected storage warning, preserves a
persistent secret reference so the real Connect path fails closed, rejects a disabled saved row
without re-enabling it, checks delete-pending control blocking, applies an exact deletion result,
and verifies a fresh random draft ID. The test runs once in a private D-Bus session under X11/Xvfb,
then the binary-only suite runs again in another private D-Bus session under forced Wayland and
headless Weston. `tools/run-wayland-test.sh` creates a private `0700` runtime directory, unsets
`DISPLAY`, sets `GDK_BACKEND=wayland`, waits at most ten seconds for a dedicated socket, and uses
exit traps to stop Weston and remove the directory. Tests are serialized because GTK owns
process-global state. The same test asserts the baseline accessibility roles, editor properties,
visible-label relations and mnemonics, focusability, explicit Stop name, hidden-empty-error
behavior, and Busy-state reset. GTK's helpers prove semantic presence and reset behavior; they do
not prove AT-SPI export, Orca speech, physical keyboard traversal, RTL/high-contrast presentation,
a physical compositor, or GPU rendering.

The GitHub Actions native workflow pins Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, installs the headers plus D-Bus, Xvfb, test-only
mount-namespace tools, and Weston support, and runs the real storage write-fault gate and both
display gates before the all-feature build. The storage write-fault change passes its exact local
namespace test through the unprivileged path. Runtime-storage functional revision
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

## Repository foundation check

```sh
set -euo pipefail
required_files="README.md LICENSE AGENTS.md REPOSITORY_ROLE.md GLOBAL_GOAL.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md THIRD_PARTY_NOTICES.md IMPLEMENTATION_STATUS.md Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml src/lib.rs src/model.rs src/worker.rs src/main.rs docs/architecture.md docs/testing.md docs/releasing.md tools/sync-l10n.sh tools/run-wayland-test.sh tools/run-storage-fault-test.sh l10n/compatibility.json l10n/manifest.json .gitignore .github/workflows/foundation.yml .github/workflows/native.yml"
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

## Unimplemented validation

Broader GTK component/UI automation, AT-SPI/Orca and physical-keyboard accessibility coverage,
physical-compositor and GPU-backed Wayland coverage, a broader X11/desktop matrix, a native Secret Service backend and
secure-credential onboarding/persistence tests, broader XDG and portal tests, third-party
local-server interoperability, Flatpak smoke tests, runtime localization behavior, runtime database
faults beyond the implemented Linux `ENOSPC` transaction boundary, dependency/license automation,
and release builds remain required before a supported release.
