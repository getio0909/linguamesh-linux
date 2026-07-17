#!/usr/bin/env python3
import os
import pathlib
import sys

import dbus
import dbus.types


def fail(message):
    print(message, file=sys.stderr)
    raise SystemExit(1)


fixture_path = pathlib.Path(sys.argv[1]).resolve()
app_id = "dev.linguamesh.LinguaMesh"
bus = dbus.SessionBus()
object_path = bus.get_object(
    "org.freedesktop.portal.Documents",
    "/org/freedesktop/portal/documents",
)
documents = dbus.Interface(object_path, "org.freedesktop.portal.Documents")
fd = os.open(fixture_path, os.O_RDONLY)
doc_id = None

try:
    doc_id = str(documents.Add(dbus.types.UnixFd(fd), False, False))
    info = documents.Info(doc_id)
    encoded_path = bytes(info[0]).rstrip(b"\0")
    if pathlib.Path(os.fsdecode(encoded_path)).resolve() != fixture_path:
        fail("Document portal returned an unexpected host path.")

    documents.GrantPermissions(doc_id, app_id, ["read"])
    granted = documents.List(app_id)
    if doc_id not in granted:
        fail("Document portal did not grant the application read permission.")

    documents.RevokePermissions(doc_id, app_id, ["read"])
    revoked = documents.List(app_id)
    if doc_id in revoked:
        fail("Document portal retained the application permission after revocation.")

    documents.GrantPermissions(doc_id, app_id, ["read"])
    documents.Delete(doc_id)
    if doc_id in documents.List(app_id):
        fail("Document portal retained the lease after deletion.")
finally:
    os.close(fd)

print("Document portal lease fixture passed: add, map, grant, revoke, and delete.")
