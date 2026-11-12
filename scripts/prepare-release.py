#!/usr/bin/env python3
"""Package a built bundle and sign the archive and feed. Never exports private keys."""
import argparse
import datetime
import hashlib
from pathlib import Path
import plistlib
import re
import subprocess
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
APP = DIST / "AMSpace.app"
ACCOUNT = "io.github.amartinrojarvis.amspace"
SIGNER = ROOT / ".cache/sparkle/bin/sign_update"
KEY_LOOKUP = ROOT / ".cache/sparkle/bin/generate_keys"
SPARKLE = "http://www.andymatuschak.org/xml-namespaces/sparkle"


def run(*args):
    return subprocess.check_output([str(a) for a in args], text=True).strip()


def prepare(*, feed_only=False, with_dmg=False):
    if feed_only and with_dmg:
        raise SystemExit("--feed-only conserva el paquete publicado; no se combina con --with-dmg.")
    built = plistlib.loads((APP / "Contents/Info.plist").read_bytes())
    expected = plistlib.loads((ROOT / "assets/Info.plist").read_bytes())
    version = built["CFBundleShortVersionString"]
    cargo = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)[1]
    if built != expected or version != cargo or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise SystemExit("Versiones desincronizadas: vuelve a compilar el paquete.")
    run("codesign", "--verify", "--deep", "--strict", APP)
    # Lookup-only mode cannot create or rotate a key. Refuse incompatible
    # signatures before packaging; the private material stays in the Keychain.
    key_args = ["--account", ACCOUNT]
    if run(KEY_LOOKUP, *key_args, "-p") != built["SUPublicEDKey"]:
        raise SystemExit("La clave pública del Llavero no coincide con SUPublicEDKey.")
    archive = DIST / "AMSpace-universal.zip"
    if feed_only:
        with zipfile.ZipFile(archive) as existing:
            if (
                plistlib.loads(existing.read("AMSpace.app/Contents/Info.plist")) != built
                or existing.read("AMSpace.app/Contents/MacOS/amspace")
                != (APP / "Contents/MacOS/amspace").read_bytes()
            ):
                raise SystemExit("El ZIP existente no corresponde al paquete compilado.")
    else:
        run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", APP, archive)
    signature = run(SIGNER, *key_args, "-p", archive)
    run(SIGNER, *key_args, "--verify", archive, signature)
    ET.register_namespace("sparkle", SPARKLE)
    rss = ET.Element("rss", version="2.0")
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = "AMSpace"
    ET.SubElement(channel, "link").text = "https://github.com/amartinrojarvis/amspace"
    ET.SubElement(channel, "language").text = "es"
    item = ET.SubElement(channel, "item")
    ET.SubElement(item, "title").text = "AMSpace " + version
    ET.SubElement(item, "pubDate").text = datetime.datetime.now(datetime.timezone.utc).strftime("%a, %d %b %Y %H:%M:%S +0000")
    ET.SubElement(item, "description", {f"{{{SPARKLE}}}format": "plain-text"}).text = (ROOT / "docs/RELEASE_NOTES.md").read_text()
    ET.SubElement(item, f"{{{SPARKLE}}}minimumSystemVersion").text = built["LSMinimumSystemVersion"]
    ET.SubElement(item, "enclosure", {
        "url": f"https://raw.githubusercontent.com/amartinrojarvis/amspace/updates/{version}/AMSpace-universal.zip",
        "length": str(archive.stat().st_size),
        "type": "application/octet-stream",
        f"{{{SPARKLE}}}version": built["CFBundleVersion"],
        f"{{{SPARKLE}}}shortVersionString": version,
        f"{{{SPARKLE}}}edSignature": signature,
    })
    feed = DIST / "appcast.xml"
    ET.ElementTree(rss).write(feed, encoding="utf-8", xml_declaration=True)
    run(SIGNER, *key_args, feed)
    run(SIGNER, *key_args, "--verify", feed)
    artifacts = [archive, feed]
    if with_dmg:
        print(run(ROOT / "scripts/dmg.sh"))
        installer = DIST / "AMSpace-installer.dmg"
        installer_signature = run(SIGNER, *key_args, "-p", installer)
        run(SIGNER, *key_args, "--verify", installer, installer_signature)
        detached = DIST / "AMSpace-installer.dmg.ed25519"
        detached.write_text(installer_signature + "\n")
        artifacts.extend([installer, detached])
    (DIST / "SHA256SUMS.txt").write_text("".join(
        f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in artifacts
    ))
    print(f"Preparada {version}, build {built['CFBundleVersion']}. ZIP y feed firmados y verificados.")
    if with_dmg:
        print("DMG y firma separada incluidos en SHA256SUMS.txt. Sparkle sigue usando el ZIP.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--feed-only", action="store_true", help="Regenerar el catálogo firmado conservando el ZIP existente.")
    parser.add_argument("--with-dmg", action="store_true", help="Añadir instalador DMG guiado y firma separada; necesita una sesión de Finder.")
    args = parser.parse_args()
    prepare(feed_only=args.feed_only, with_dmg=args.with_dmg)
