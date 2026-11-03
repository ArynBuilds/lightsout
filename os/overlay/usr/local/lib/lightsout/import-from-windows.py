#!/usr/bin/python3
"""Imports what Windows handed over in C:\\LightsOut (on the encrypted drive):
saved Wi-Fi networks (exported by the installer) and the Windows user name.
Runs at every start, so networks added in Windows show up here too."""
import os, re, shutil, xml.etree.ElementTree as ET
from pathlib import Path

C = Path("/mnt/c/LightsOut")
IWD = Path("/var/lib/iwd")
NS = {"w": "http://www.microsoft.com/networking/WLAN/profile/v1"}


def iwd_name(ssid: str, kind: str) -> str:
    # iwd uses the plain name when it's simple, otherwise "=<hex of the bytes>"
    if re.fullmatch(r"[A-Za-z0-9 _-]+", ssid):
        return f"{ssid}.{kind}"
    return f"={ssid.encode('utf-8').hex()}.{kind}"


def import_wifi():
    src = C / "wifi"
    if not src.is_dir():
        return
    IWD.mkdir(mode=0o700, parents=True, exist_ok=True)
    for f in src.glob("*.xml"):
        try:
            root = ET.fromstring(f.read_text(encoding="utf-8", errors="replace"))
            ssid = root.findtext("w:SSIDConfig/w:SSID/w:name", namespaces=NS)
            auth = (root.findtext(".//w:authEncryption/w:authentication", namespaces=NS) or "").lower()
            key = root.findtext(".//w:sharedKey/w:keyMaterial", namespaces=NS)
            if not ssid:
                continue
            if auth == "open":
                (IWD / iwd_name(ssid, "open")).write_text("[Settings]\nAutoConnect=true\n")
            elif key:  # WPA2/WPA3 personal: iwd only needs the passphrase
                p = IWD / iwd_name(ssid, "psk")
                p.write_text(f"[Security]\nPassphrase={key}\n[Settings]\nAutoConnect=true\n")
                os.chmod(p, 0o600)
        except Exception as e:  # one broken profile must not stop the rest
            print(f"skipped {f.name}: {e}")


def import_user():
    u = C / "config" / "windows-user"
    if u.is_file():
        Path("/etc/lightsout").mkdir(exist_ok=True)
        shutil.copyfile(u, "/etc/lightsout/windows-user")


def import_timezone():
    tz = C / "config" / "timezone"
    if tz.is_file():
        name = tz.read_text().strip()
        if re.fullmatch(r"[A-Za-z_]+/[A-Za-z_/+-]+", name) and Path("/usr/share/zoneinfo", name).exists():
            try:
                os.remove("/etc/localtime")
            except FileNotFoundError:
                pass
            os.symlink(f"/usr/share/zoneinfo/{name}", "/etc/localtime")
            Path("/etc/timezone").write_text(name + chr(10))


if __name__ == "__main__":
    import_wifi()
    import_user()
    import_timezone()
