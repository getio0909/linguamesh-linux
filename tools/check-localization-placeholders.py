#!/usr/bin/env python3
"""审计 Linux 源码回退模板与 canonical catalog 的占位符一致性。"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path


CALL_NAMES = (
    "localization::text_plural",
    "localization::text",
    "localized_mnemonic",
    "localized_template",
)
CALL_PATTERN = re.compile(r"(?P<name>" + "|".join(re.escape(name) for name in CALL_NAMES) + r")\s*\(")
KEY_PATTERN = re.compile(r"^[a-z][a-z0-9_.-]+$")
PLACEHOLDER_PATTERN = re.compile(r"\{([a-z][a-z0-9_]*)\}")


def load_catalog(path: Path) -> dict[str, dict]:
    """读取 canonical JSON catalog 的消息定义。"""
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        return {message["key"]: message for message in payload["messages"]}
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"Localization placeholder audit could not read {path}: {error}", file=sys.stderr)
        raise SystemExit(2) from error


def split_arguments(source: str, opening: int) -> tuple[list[str], int] | None:
    """按顶层逗号分割调用参数，并返回闭合括号位置。"""
    arguments: list[str] = []
    start = opening + 1
    depth = 1
    string = False
    escaped = False
    index = start
    while index < len(source):
        character = source[index]
        if string:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                string = False
        elif character == '"':
            string = True
        elif character == '(':
            depth += 1
        elif character == ')':
            depth -= 1
            if depth == 0:
                arguments.append(source[start:index].strip())
                return arguments, index
        elif character == ',' and depth == 1:
            arguments.append(source[start:index].strip())
            start = index + 1
        index += 1
    return None


def rust_string(value: str) -> str | None:
    """解析调用参数中的普通 Rust 字符串字面量。"""
    value = value.strip()
    if len(value) < 2 or value[0] != '"' or value[-1] != '"':
        return None
    result: list[str] = []
    escaped = False
    for character in value[1:-1]:
        if escaped:
            result.append({"n": "\n", "r": "\r", "t": "\t", "\\": "\\", '"': '"'}.get(character, character))
            escaped = False
        elif character == "\\":
            escaped = True
        else:
            result.append(character)
    if escaped:
        return None
    return "".join(result)


def placeholder_names(template: str) -> set[str] | None:
    """提取模板占位符并拒绝未配对的大括号。"""
    names = set(PLACEHOLDER_PATTERN.findall(template))
    remainder = PLACEHOLDER_PATTERN.sub("", template)
    if "{" in remainder or "}" in remainder:
        return None
    return names


def source_calls(source: str, path: Path) -> list[tuple[str, str, list[str], int]]:
    """提取带字面量 key 的本地化调用及其参数。"""
    calls: list[tuple[str, str, list[str], int]] = []
    for match in CALL_PATTERN.finditer(source):
        parsed = split_arguments(source, match.end() - 1)
        if parsed is None:
            continue
        arguments, _ = parsed
        if len(arguments) < 3:
            continue
        key = rust_string(arguments[1])
        if key is None or not KEY_PATTERN.fullmatch(key):
            continue
        line = source.count("\n", 0, match.start()) + 1
        calls.append((match.group("name"), key, arguments, line))
    return calls


def expected_placeholders(message: dict) -> set[str] | None:
    """返回 catalog 消息要求的模板占位符集合。"""
    value = message["value"]
    if value["type"] == "select":
        return set(message["placeholders"]) - {value["selector"]}
    return set(message["placeholders"])


def main() -> int:
    """执行源码回退模板占位符审计。"""
    root = Path(__file__).resolve().parents[1]
    l10n_root = Path(os.environ.get("LINGUAMESH_L10N_DIR", str(root.parent / "linguamesh-l10n")))
    messages = load_catalog(l10n_root / "catalog" / "messages.json")
    failures: list[str] = []
    call_count = 0
    for source_path in sorted((root / "src").glob("*.rs")):
        try:
            source = source_path.read_text(encoding="utf-8")
        except OSError as error:
            print(f"Localization placeholder audit could not read {source_path}: {error}", file=sys.stderr)
            return 2
        for name, key, arguments, line in source_calls(source, source_path):
            if key not in messages:
                continue
            call_count += 1
            expected = expected_placeholders(messages[key])
            fallback_arguments = arguments[2:4] if name == "localization::text_plural" else arguments[2:3]
            for offset, fallback_argument in enumerate(fallback_arguments):
                fallback = rust_string(fallback_argument)
                if fallback is None:
                    continue
                actual = placeholder_names(fallback)
                if actual is None:
                    failures.append(f"{source_path}:{line}: {key} fallback contains malformed placeholder braces")
                elif actual != expected:
                    failures.append(
                        f"{source_path}:{line}: {key} fallback {offset + 1} placeholders "
                        f"{sorted(actual)} do not match {sorted(expected or set())}"
                    )
    if failures:
        print("Localization placeholder audit failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print(f"Localization placeholder audit passed: {call_count} catalog-backed literal calls checked.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
