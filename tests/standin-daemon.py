#!/usr/bin/env python3
"""A stand-in for kdeconnectd with one phone that runs ewe Mobile for Android:
the daemon object, the device, and its `remotecommands` serving a C1 list
(docs/ECOSYSTEM.md). For a PRIVATE session bus only (dbus-run-session) —
it takes the daemon's name.

    standin-daemon.py <fixture.json> <log file> [--stock]

  fixture.json   the C1 commandList object (the "ewe" marker and the apps)
  log file       every triggerCommand key is appended here, one per line
  --stock        the phone runs the stock app: no remotecommands object at all

The device: id abc123, name "Pixel 9", paired and in reach, type phone.
Sending SIGUSR1 makes it emit `commandsChanged` with a one-app list.
"""
import json
import signal
import sys

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
fixture = open(sys.argv[1], "rb").read()
log = sys.argv[2]
stock = "--stock" in sys.argv[3:]
ROOT = "/modules/kdeconnect"
DEV = ROOT + "/devices/abc123"


class Daemon(dbus.service.Object):
    @dbus.service.method("org.kde.kdeconnect.daemon", in_signature="bb", out_signature="as")
    def devices(self, only_reachable, only_paired):
        return ["abc123"]

    @dbus.service.method("org.kde.kdeconnect.daemon", out_signature="s")
    def announcedName(self):
        return "stand-in"

    @dbus.service.method("org.kde.kdeconnect.daemon", out_signature="s")
    def selfId(self):
        return "selfid"


class Device(dbus.service.Object):
    PROPS = {
        "name": "Pixel 9", "type": "phone", "isReachable": True, "isPaired": True,
        "pairState": dbus.Int32(3), "isPairRequested": False, "isPairRequestedByPeer": False,
        "verificationKey": "ABCD", "supportedPlugins": dbus.Array(["kdeconnect_remotecommands", "kdeconnect_notifications"], signature="s"),
    }

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return self.PROPS

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, iface, prop):
        return self.PROPS[prop]

    @dbus.service.method("org.kde.kdeconnect.device", out_signature="as")
    def loadedPlugins(self):
        return ["kdeconnect_remotecommands", "kdeconnect_notifications"]


class RemoteCommands(dbus.service.Object):
    def __init__(self, *a):
        super().__init__(*a)
        self.commands = dbus.ByteArray(fixture)

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return {"commands": self.commands, "canAddCommand": False, "deviceId": "abc123"}

    @dbus.service.method("org.kde.kdeconnect.device.remotecommands", in_signature="s")
    def triggerCommand(self, key):
        with open(log, "a") as f:
            f.write(str(key) + "\n")

    @dbus.service.signal("org.kde.kdeconnect.device.remotecommands", signature="ay")
    def commandsChanged(self, commands):
        pass

    def change(self):
        new = {"ewe": {"name": "ewe Mobile", "command": "", "v": 1, "count": 1},
               "org.signal": {"name": "Signal (changed)", "command": "org.signal", "sys": False}}
        self.commands = dbus.ByteArray(json.dumps(new).encode())
        self.commandsChanged(self.commands)


name = dbus.service.BusName("org.kde.kdeconnect", bus)
daemon = Daemon(bus, ROOT)
device = Device(bus, DEV)
rc = None if stock else RemoteCommands(bus, DEV + "/remotecommands")
if rc is not None:
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, lambda *_: (rc.change(), True)[1])
print("stand-in up", flush=True)
GLib.MainLoop().run()
