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
SPARKLE = "http://www.andymatuschak.org/xml-namespaces/sparkle"


def run(*args):
    return subprocess.check_output([str(a) for a in args], text=True).strip()


def prepare(*, feed_only=False):
    built = plistlib.loads((APP / "Contents/Info.plist").read_bytes())
    expected = plistlib.loads((ROOT / "assets/Info.plist").read_bytes())
    version = built["CFBundleShortVersionString"]
    cargo = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)[1]
    if built != expected or version != cargo or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise SystemExit("Versiones desincronizadas: vuelve a compilar el paquete.")
    run("codesign", "--verify", "--deep", "--strict", APP)
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
    signature = run(SIGNER, "--account", ACCOUNT, "-p", archive)
    run(SIGNER, "--account", ACCOUNT, "--verify", archive, signature)
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
    run(SIGNER, "--account", ACCOUNT, feed)
    run(SIGNER, "--account", ACCOUNT, "--verify", feed)
    (DIST / "SHA256SUMS.txt").write_text("".join(
        f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in (archive, feed)
    ))
    print(f"Preparada {version}, build {built['CFBundleVersion']}. ZIP y feed firmados y verificados.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--feed-only", action="store_true", help="Regenerar el catálogo firmado conservando el ZIP existente.")
    prepare(feed_only=parser.parse_args().feed_only)
