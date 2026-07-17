# Testing and Validation

## Host prerequisites

Rust 1.93.0 is pinned by `rust-toolchain.toml`. A sibling `../linguamesh-core` checkout is required
because the client deliberately uses typed path dependencies instead of copying shared behavior.
It must be at approved revision `873b6da45447f73e4be4e2f1127c3c8d0f188cf2`; validate it with:

```sh
test "$(git -C ../linguamesh-core rev-parse HEAD)" = "873b6da45447f73e4be4e2f1127c3c8d0f188cf2"
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

The first test command covers the pure application state, including atomic provider/model switching
and failed-connection rollback. The feature-enabled test command also starts the shared core fake
provider, performs real loopback HTTP/SSE streaming, verifies cancellation with partial-output
retention, reconnects the built-in provider, connects and translates through a second loopback fake
provider, and proves that a failed connection leaves the previous engine usable. A regression also
verifies that an event stream ending without a terminal event becomes a failed operation while
retaining partial output. The current counts are 16 no-default-feature tests and 23
`demo-provider` tests.

The GTK Rust source can be checked without native linking as a limited diagnostic:

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
sudo apt-get install libgtk-4-dev libadwaita-1-dev dbus-daemon gettext pkg-config xauth xvfb
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
cargo build --all-targets --all-features --locked
```

Run the development slice with:

```sh
cargo run --features gui
```

The all-feature binary test creates real GTK/libadwaita widgets, verifies that invalid connection
attempts retain the active provider/model, clicks Connect using the built-in credential-free
provider, clicks Translate, and waits for the streamed result. A private D-Bus session and Xvfb
provide the runtime environment; tests are serialized because GTK owns process-global state. This
is not comprehensive UI automation, accessibility, or Wayland coverage.

The GitHub Actions native workflow pins Core revision
`873b6da45447f73e4be4e2f1127c3c8d0f188cf2`, installs the headers plus D-Bus/Xvfb support, and runs
the complete gate. A successful workflow run may provide native build evidence. The Xvfb change
has no remote result yet. A local host without `gtk4.pc` or `libadwaita-1.pc` must not claim that
the GUI build, launch, or GTK button test passed.

## Repository foundation check

```sh
set -euo pipefail
required_files="README.md LICENSE AGENTS.md REPOSITORY_ROLE.md GLOBAL_GOAL.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md THIRD_PARTY_NOTICES.md IMPLEMENTATION_STATUS.md Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml src/lib.rs src/model.rs src/worker.rs src/main.rs docs/architecture.md docs/testing.md docs/releasing.md tools/sync-l10n.sh l10n/compatibility.json l10n/manifest.json .gitignore .github/workflows/foundation.yml .github/workflows/native.yml"
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

Broader GTK component/UI automation, accessibility inspection, Wayland/X11 smoke tests, Secret
Service and portal tests, third-party local-server interoperability, Flatpak smoke tests, runtime
localization behavior, dependency/license automation, and release builds remain required before a
supported release.
