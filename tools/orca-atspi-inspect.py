#!/usr/bin/env python3
"""Focus the production Stop control through the live Linux AT-SPI tree."""

from __future__ import annotations

import subprocess
import sys
import time
from collections.abc import Iterator

import pyatspi


def descendants(node: object) -> Iterator[object]:
    """遍历辅助技术树，并忽略应用退出时失效的节点。"""
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


def role_name(node: object) -> str:
    """返回节点的稳定人类可读角色名称。"""
    try:
        return str(node.getRoleName())  # type: ignore[attr-defined]
    except Exception:
        return ""


def state_summary(node: object) -> str:
    """返回聚焦诊断所需的最小 AT-SPI 状态集合。"""
    try:
        state = node.getState()  # type: ignore[attr-defined]
    except Exception:
        return "unknown"
    names = []
    for name in ("STATE_FOCUSABLE", "STATE_SENSITIVE", "STATE_ENABLED", "STATE_VISIBLE"):
        value = getattr(pyatspi, name, None)
        try:
            if value is not None and state.contains(value):
                names.append(name.removeprefix("STATE_").lower())
        except Exception:
            continue
    return ",".join(names) or "none"


def focus_by_pointer(node: object) -> bool:
    """使用 AT-SPI 几何信息点击控件，让 GTK 按真实输入路径取得焦点。"""
    try:
        rectangle = node.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)  # type: ignore[attr-defined]
        x = int(rectangle.x + rectangle.width / 2)
        y = int(rectangle.y + rectangle.height / 2)
    except Exception:
        return False
    result = subprocess.run(
        ["xdotool", "mousemove", "--sync", str(x), str(y)],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return False
    result = subprocess.run(
        ["xdotool", "click", "1"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        return False
    time.sleep(0.2)
    try:
        return bool(node.getState().contains(pyatspi.STATE_FOCUSED))  # type: ignore[attr-defined]
    except Exception:
        return False


def find_stop_control(deadline: float) -> object | None:
    """等待应用注册并找到名为 Stop translation 的按钮。"""
    while time.monotonic() < deadline:
        try:
            desktop = pyatspi.Registry.getDesktop(0)
            for node in descendants(desktop):
                if str(getattr(node, "name", "")) != "Stop translation":
                    continue
                if "button" in role_name(node).lower():
                    return node
        except Exception:
            pass
        time.sleep(0.1)
    return None


def main() -> int:
    """通过 AT-SPI 聚焦生产控件，触发 Orca 的可访问对象处理路径。"""
    node = find_stop_control(time.monotonic() + 20.0)
    if node is None:
        print(
            "Orca AT-SPI fixture could not find the named Stop translation button.",
            file=sys.stderr,
        )
        return 1
    try:
        component = node.queryComponent()  # type: ignore[attr-defined]
        focused = bool(component.grabFocus())
    except Exception:
        focused = False
    if not focused:
        focused = focus_by_pointer(node)
    if not focused:
        print(
            "Orca AT-SPI fixture could not focus the Stop translation button "
            f"(states: {state_summary(node)}).",
            file=sys.stderr,
        )
        return 1
    print(
        f"Orca AT-SPI fixture focused accessible control: {role_name(node)}: Stop translation."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
