#!/usr/bin/env python3
"""审计 Linux 源码引用的本地化 key 是否存在于 canonical catalog。"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path


CALL_NAMES = ("localization::text", "localized_mnemonic", "localized_template")
CALL_PATTERN = re.compile(
    r"\b(?:" + "|".join(re.escape(name) for name in CALL_NAMES) + r")\s*\("
)
KEY_PATTERN = re.compile(r'"([a-z][a-z0-9_.-]+)"')

# 补充检查通过映射表传入本地化辅助函数的诊断 key。
DIAGNOSTICS_KEYS = {
    "diagnostics.onboarding",
    "diagnostics.provider",
    "diagnostics.provider_saved",
    "diagnostics.profile_storage",
    "diagnostics.saved_profiles",
    "diagnostics.saved_profile",
    "diagnostics.persisted_active_profile",
    "diagnostics.saved_model",
    "diagnostics.model_selected",
    "diagnostics.model_selection_pending",
    "diagnostics.profile_deletion_pending",
    "diagnostics.status",
    "diagnostics.theme",
    "diagnostics.locale",
    "diagnostics.output_bytes",
    "diagnostics.yes",
    "diagnostics.no",
    "diagnostics.profile_storage_pending",
    "diagnostics.profile_storage_available",
    "diagnostics.profile_storage_unavailable",
}


def load_catalog(path: Path) -> set[str]:
    """读取 canonical JSON catalog 的消息 key。"""
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        messages = payload["messages"]
        return {message["key"] for message in messages}
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"Localization key audit could not read {path}: {error}", file=sys.stderr)
        raise SystemExit(2) from error


def referenced_keys(source: str) -> set[str]:
    """提取本地化调用中第二个参数位置的字面量 key。"""
    keys: set[str] = set()
    for match in CALL_PATTERN.finditer(source):
        window = source[match.end() : match.end() + 800]
        first_comma = window.find(",")
        if first_comma == -1:
            continue
        key_match = KEY_PATTERN.match(window[first_comma + 1 :].lstrip())
        if key_match:
            keys.add(key_match.group(1))
    return keys


def main() -> int:
    """执行源码与 catalog 的覆盖审计。"""
    root = Path(__file__).resolve().parents[1]
    l10n_root = Path(
        os.environ.get("LINGUAMESH_L10N_DIR", str(root.parent / "linguamesh-l10n"))
    )
    catalog = l10n_root / "catalog" / "messages.json"
    known = load_catalog(catalog)
    used: set[str] = set()
    for relative in (Path("src/main.rs"), Path("src/model.rs")):
        source_path = root / relative
        try:
            used.update(referenced_keys(source_path.read_text(encoding="utf-8")))
        except OSError as error:
            print(f"Localization key audit could not read {source_path}: {error}", file=sys.stderr)
            return 2
    used.update(DIAGNOSTICS_KEYS)

    missing = sorted(used - known)
    if missing:
        print("Localization key audit found keys missing from the canonical catalog:", file=sys.stderr)
        for key in missing:
            print(f"- {key}", file=sys.stderr)
        return 1
    print(f"Localization key audit passed: {len(used)} Linux source keys are catalog-backed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
