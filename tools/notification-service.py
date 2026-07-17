#!/usr/bin/env python3
import os
import signal

import dbus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib


capture_path = os.environ["LINGUAMESH_NOTIFICATION_CAPTURE"]


class NotificationService(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, "/org/freedesktop/Notifications")

    @dbus.service.method(
        "org.freedesktop.Notifications",
        in_signature="susssasa{sv}i",
        out_signature="u",
    )
    def Notify(
        self,
        app_name,
        replaces_id,
        app_icon,
        summary,
        body,
        actions,
        hints,
        expire_timeout,
    ):
        with open(capture_path, "a", encoding="utf-8") as capture:
            capture.write(f"summary={summary}\nbody={body}\n")
        return dbus.UInt32(1)

    @dbus.service.method(
        "org.freedesktop.Notifications",
        in_signature="",
        out_signature="as",
    )
    def GetCapabilities(self):
        return ["body"]

    @dbus.service.method(
        "org.freedesktop.Notifications",
        in_signature="",
        out_signature="ssss",
    )
    def GetServerInformation(self):
        return ("LinguaMesh notification fixture", "LinguaMesh", "1.0", "1.2")


loop = GLib.MainLoop()


def stop(_signal, _frame):
    loop.quit()


dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
bus.request_name("org.freedesktop.Notifications")
service = NotificationService(bus)
signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
loop.run()
