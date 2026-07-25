#!/usr/bin/env python3
import os
import signal

import dbus
import dbus.bus
import dbus.mainloop.glib
import dbus.service
from gi.repository import GLib


SERVICE_NAME = "org.freedesktop.secrets"
SERVICE_PATH = "/org/freedesktop/secrets"
COLLECTION_PATH = "/org/freedesktop/secrets/collection/login"
SESSION_PATH = "/org/freedesktop/secrets/session/prompt_fixture"
ITEM_PATH = "/org/freedesktop/secrets/item/prompt_fixture"
PROMPT_PATH = "/org/freedesktop/secrets/prompt/fixture"
FIXTURE_INTERFACE = "com.linguamesh.SecretServicePromptFixture"
OPERATION = os.environ.get("LINGUAMESH_SECRET_SERVICE_PROMPT_OPERATION", "store")
PROMPT_DISMISSED = os.environ.get("LINGUAMESH_SECRET_SERVICE_PROMPT_DISMISSED") == "1"
READY_FILE = os.environ.get("LINGUAMESH_SECRET_SERVICE_PROMPT_READY_FILE")


class SecretService(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, SERVICE_PATH)

    @dbus.service.method(
        "org.freedesktop.Secret.Service",
        in_signature="sv",
        out_signature="vo",
    )
    def OpenSession(self, _mechanism, _input):
        return (
            dbus.String("", variant_level=1),
            dbus.ObjectPath(SESSION_PATH),
        )

    @dbus.service.method(
        "org.freedesktop.Secret.Service",
        in_signature="a{ss}",
        out_signature="aoao",
    )
    def SearchItems(self, _attributes):
        if OPERATION == "delete":
            return ([dbus.ObjectPath(ITEM_PATH)], [])
        return ([], [])

    @dbus.service.method(
        "org.freedesktop.Secret.Service",
        in_signature="s",
        out_signature="o",
    )
    def ReadAlias(self, _alias):
        return dbus.ObjectPath(COLLECTION_PATH)

    @dbus.service.method(FIXTURE_INTERFACE, in_signature="", out_signature="s")
    def Ping(self):
        return "ready"

    @dbus.service.method(
        "org.freedesktop.Secret.Service",
        in_signature="o",
        out_signature="",
    )
    def CloseSession(self, _session):
        return None


class SecretCollection(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, COLLECTION_PATH)

    @dbus.service.method(
        "org.freedesktop.Secret.Collection",
        in_signature="a{sv}(oayays)b",
        out_signature="oo",
    )
    def CreateItem(self, _properties, _secret, _replace):
        return (dbus.ObjectPath(ITEM_PATH), dbus.ObjectPath(PROMPT_PATH))


class SecretItem(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, ITEM_PATH)

    @dbus.service.method(
        "org.freedesktop.Secret.Item",
        in_signature="",
        out_signature="o",
    )
    def Delete(self):
        return dbus.ObjectPath(PROMPT_PATH)


class SecretPrompt(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, PROMPT_PATH)

    @dbus.service.method(
        "org.freedesktop.Secret.Prompt",
        in_signature="s",
        out_signature="",
    )
    def Prompt(self, _window_id):
        self.Completed(
            dbus.Boolean(PROMPT_DISMISSED),
            dbus.String("", variant_level=1),
        )

    @dbus.service.signal("org.freedesktop.Secret.Prompt", signature="bv")
    def Completed(self, _dismissed, _result):
        return None


loop = GLib.MainLoop()


def stop(_signal, _frame):
    loop.quit()


dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
request_result = bus.request_name(
    SERVICE_NAME,
    dbus.bus.NAME_FLAG_REPLACE_EXISTING | dbus.bus.NAME_FLAG_DO_NOT_QUEUE,
)
if request_result not in (
    dbus.bus.REQUEST_NAME_REPLY_PRIMARY_OWNER,
    dbus.bus.REQUEST_NAME_REPLY_ALREADY_OWNER,
):
    raise RuntimeError("Secret Service prompt fixture could not own its bus name.")
service = SecretService(bus)
collection = SecretCollection(bus)
item = SecretItem(bus)
prompt = SecretPrompt(bus)
if READY_FILE:
    with open(READY_FILE, "w", encoding="ascii") as ready_file:
        ready_file.write("ready\n")
signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
loop.run()
