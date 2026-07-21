#!/usr/bin/env python3
"""Inspect the running GTK application through the Linux AT-SPI tree."""

from __future__ import annotations

import sys
import time
from collections.abc import Iterator

import pyatspi


def descendants(node: object) -> Iterator[object]:
    """遍历辅助技术树，忽略应用退出时可能失效的节点。"""
    yield node
    try:
        count = int(node.childCount)  # type: ignore[attr-defined]
    except Exception:
        return
    for index in range(count):
        try:
            child = node.getChildAtIndex(index)  # type: ignore[attr-defined]
        except Exception:
            continue
        yield from descendants(child)


def node_summary(node: object) -> str:
    """将节点转换为不含源文本或凭据的诊断行。"""
    try:
        name = str(node.name)  # type: ignore[attr-defined]
    except Exception:
        name = "<unnamed>"
    try:
        role = str(node.getRoleName())  # type: ignore[attr-defined]
    except Exception:
        role = "<unknown-role>"
    return f"{role}: {name}"


def role_token(node: object) -> str:
    """统一 AT-SPI 的人类可读角色名称和常量式断言。"""
    try:
        role = str(node.getRoleName())  # type: ignore[attr-defined]
    except Exception:
        return ""
    normalized = role.upper().replace(" ", "_")
    return normalized if normalized.startswith("ROLE_") else f"ROLE_{normalized}"


def find_nodes(deadline: float) -> list[object]:
    """等待应用注册到 AT-SPI，并返回当前可读节点。"""
    while time.monotonic() < deadline:
        try:
            desktop = pyatspi.Registry.getDesktop(0)
            nodes = list(descendants(desktop))
        except Exception:
            nodes = []
        if any(
            str(getattr(node, "name", "")) in {"LinguaMesh", "linguamesh-linux"}
            for node in nodes
        ):
            return nodes
        time.sleep(0.1)
    return nodes


def main() -> int:
    nodes = find_nodes(time.monotonic() + 20.0)
    if not nodes:
        print("AT-SPI fixture could not enumerate the accessibility desktop.", file=sys.stderr)
        return 1

    expected = {
        "Open text file": {"ROLE_PUSH_BUTTON"},
        "Allow approved fallback": {"ROLE_CHECK_BOX"},
        "Translate": {"ROLE_PUSH_BUTTON"},
        "Retry translation": {"ROLE_PUSH_BUTTON"},
        "Stop translation": {"ROLE_PUSH_BUTTON"},
    }
    by_name: dict[str, list[object]] = {}
    for node in nodes:
        try:
            name = str(node.name)  # type: ignore[attr-defined]
        except Exception:
            continue
        if name:
            by_name.setdefault(name, []).append(node)

    missing: list[str] = []
    wrong_roles: list[str] = []
    for name, roles in expected.items():
        matches = by_name.get(name, [])
        if not matches:
            missing.append(name)
            continue
        if not any(role_token(node) in roles for node in matches):
            wrong_roles.append(f"{name} ({', '.join(sorted(roles))})")

    text_nodes = [node for node in nodes if role_token(node) in {"ROLE_TEXT", "ROLE_EDITBAR"}]
    if len(text_nodes) < 2:
        wrong_roles.append("text editors (two ROLE_TEXT/ROLE_EDITBAR nodes)")

    if missing or wrong_roles:
        print("AT-SPI fixture did not find the expected accessible controls.", file=sys.stderr)
        for node in nodes:
            summary = node_summary(node)
            if role_token(node) in {"ROLE_TEXT", "ROLE_EDITBAR", "ROLE_PUSH_BUTTON", "ROLE_LABEL"}:
                print(summary, file=sys.stderr)
        if missing:
            print(f"Missing accessible names: {', '.join(missing)}", file=sys.stderr)
        if wrong_roles:
            print(f"Unexpected accessible roles: {', '.join(wrong_roles)}", file=sys.stderr)
        return 1

    print("GTK AT-SPI fixture passed: named controls and roles are exported to the accessibility tree.")
    for name in expected:
        role = next(role_token(node) for node in by_name[name])
        print(f"AT-SPI control: {role}: {name}")
    for node in text_nodes:
        print(f"AT-SPI text editor: {node_summary(node)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
