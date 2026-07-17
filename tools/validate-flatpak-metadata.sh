#!/usr/bin/env bash
set -euo pipefail

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
manifest="$root_dir/packaging/flatpak/dev.linguamesh.LinguaMesh.yml"
desktop="$root_dir/packaging/flatpak/dev.linguamesh.LinguaMesh.desktop"
metainfo="$root_dir/packaging/flatpak/dev.linguamesh.LinguaMesh.metainfo.xml"
sources="$root_dir/packaging/flatpak/cargo-sources.json"

python3 -m json.tool "$manifest" >/dev/null
python3 -m json.tool "$sources" >/dev/null
desktop-file-validate "$desktop"
appstreamcli validate "$metainfo"

python3 - "$manifest" "$sources" <<'PY'
import json
import pathlib
import re
import sys

manifest = json.loads(pathlib.Path(sys.argv[1]).read_text())
sources = json.loads(pathlib.Path(sys.argv[2]).read_text())
assert manifest["app-id"] == "dev.linguamesh.LinguaMesh"
assert manifest["command"] == "linguamesh-linux"
assert "--socket=wayland" in manifest["finish-args"]
assert "--socket=fallback-x11" in manifest["finish-args"]
assert "--talk-name=org.freedesktop.secrets" in manifest["finish-args"]
assert "--talk-name=org.freedesktop.Notifications" in manifest["finish-args"]
module = manifest["modules"][0]
git_sources = [source for source in module["sources"] if isinstance(source, dict) and source.get("type") == "git"]
assert len(git_sources) == 2
for source in git_sources:
    assert re.fullmatch(r"[0-9a-f]{40}", source["commit"])
assert any(source == "cargo-sources.json" for source in module["sources"])
assert sources
for source in sources:
    assert source["type"] in {"archive", "inline"}
    if source["type"] == "archive":
        assert re.fullmatch(r"[0-9a-f]{64}", source["sha256"])
print("Flatpak metadata and vendored source manifest are valid.")
PY
