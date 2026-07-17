#!/usr/bin/env python3

import os
import pathlib
import subprocess
import threading
import time
import urllib.parse

import dbus
import dbus.mainloop.glib
from gi.repository import GLib


def fail(message):
    raise SystemExit(message)


fixture = pathlib.Path(os.environ["LINGUAMESH_FILE_CHOOSER_FIXTURE"]).resolve()
folder = fixture.parent
selected = {"uri": None, "response": None}
loop = GLib.MainLoop()


def abort(message):
    selected["error"] = message
    loop.quit()


def activate_dialog():
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        for pattern in ("Open text file", "Open File", "Select a File"):
            try:
                output = subprocess.check_output(
                    ["xdotool", "search", "--onlyvisible", "--name", pattern],
                    stderr=subprocess.DEVNULL,
                    text=True,
                )
            except subprocess.CalledProcessError:
                continue
            windows = [line for line in output.splitlines() if line.strip()]
            if not windows:
                continue
            window = windows[-1]
            try:
                subprocess.run(["xdotool", "key", "--window", window, "ctrl+l"], check=True)
                subprocess.run(
                    ["xdotool", "type", "--window", window, "--delay", "1", str(fixture)],
                    check=True,
                )
                subprocess.run(["xdotool", "key", "--window", window, "Return"], check=True)
                subprocess.run(["xdotool", "key", "--window", window, "Return"], check=True)
                return
            except (OSError, subprocess.CalledProcessError) as error:
                abort(f"File chooser portal automation failed: {error}")
                return
        time.sleep(0.1)
    abort("File chooser portal dialog did not become visible.")


def on_response(response, results):
    selected["response"] = int(response)
    uris = results.get("uris", [])
    selected["uri"] = str(uris[0]) if uris else None
    loop.quit()


dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
desktop = bus.get_object(
    "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop"
)
chooser = dbus.Interface(desktop, "org.freedesktop.portal.FileChooser")
filters = dbus.Array(
    [
        dbus.Struct(
            (
                dbus.String("Text files"),
                dbus.Array(
                    [
                        dbus.Struct((dbus.UInt32(0), dbus.String("*.txt"))),
                        dbus.Struct((dbus.UInt32(0), dbus.String("*.md"))),
                    ],
                    signature="(us)",
                ),
            ),
            signature="sa(us)",
        )
    ],
    signature="(sa(us))",
)
options = dbus.Dictionary(
    {
        "accept_label": dbus.String("Open"),
        "modal": dbus.Boolean(True),
        "filters": filters,
        "current_folder": dbus.ByteArray(os.fsencode(folder) + b"\0"),
    },
    signature="sv",
)
handle = chooser.OpenFile("", "Open text file", options)
bus.add_signal_receiver(
    on_response,
    signal_name="Response",
    dbus_interface="org.freedesktop.portal.Request",
    path=str(handle),
)
threading.Thread(target=activate_dialog, daemon=True).start()
loop.run()

if selected.get("error"):
    fail(selected["error"])
if selected["response"] != 0:
    fail(f"File chooser portal returned response {selected['response']}.")
if not selected["uri"]:
    fail("File chooser portal returned no URI.")
parsed = urllib.parse.urlparse(selected["uri"])
if parsed.scheme != "file":
    fail("File chooser portal returned a non-file URI.")
selected_path = pathlib.Path(urllib.parse.unquote(parsed.path)).resolve()
if selected_path.name != fixture.name:
    fail("File chooser portal selected an unexpected file.")
try:
    contents = selected_path.read_text(encoding="utf-8")
except OSError as error:
    fail(f"File chooser portal returned an unreadable file: {error}")
if contents != fixture.read_text(encoding="utf-8"):
    fail("File chooser portal returned unexpected file contents.")
print("File chooser portal fixture passed: interactive selection returned the UTF-8 fixture.")
