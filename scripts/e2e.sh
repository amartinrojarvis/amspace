#!/bin/bash
# Diagnósticos nativos sobre un bundle ya compilado; nunca instala ni cierra la app.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec python3 - "$ROOT" "$@" <<'PY'
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import signal
import subprocess
import sys
import tempfile
import time


def fail(message):
    raise SystemExit(message)


if len(sys.argv) not in (3, 4) or (len(sys.argv) == 4 and sys.argv[3] != "--with-space-transfers"):
    fail("Uso: ./scripts/e2e.sh <bundle.app/Contents/MacOS/amspace> [--with-space-transfers]")
if sys.platform != "darwin":
    fail("La batería necesita macOS y una sesión gráfica.")
root = Path(sys.argv[1])
binary = Path(sys.argv[2]).expanduser().resolve()
if not binary.is_file() or not os.access(binary, os.X_OK):
    fail("El binario no existe o no es ejecutable.")
try:
    info = plistlib.loads((binary.parent.parent / "Info.plist").read_bytes())
except (OSError, plistlib.InvalidFileException) as error:
    fail(f"Se requiere un bundle empaquetado: {error}")
domain = "io.github.amartinrojarvis.amspace"
if info.get("CFBundleIdentifier") != domain or info.get("CFBundleExecutable") != binary.name:
    fail("El ejecutable no coincide con el bundle de AMSpace.")
with_spaces = len(sys.argv) == 4
if with_spaces:
    running = subprocess.run(["/usr/bin/pgrep", "-x", binary.name], capture_output=True)
    if running.returncode == 0:
        fail("Cierra la app normal antes de --with-space-transfers; el runner no la cerrará.")
    if running.returncode != 1:
        fail("No se ha podido comprobar que la app normal esté cerrada.")

cache = root / ".cache"
cache.mkdir(exist_ok=True)
stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
out = Path(tempfile.mkdtemp(prefix=f"e2e-{stamp}-", dir=cache))
# Los diagnósticos usan rutas .cache relativas: este cwd aísla también sus capturas.
(out / ".cache").mkdir()
archs = subprocess.check_output(["/usr/bin/lipo", "-archs", str(binary)], text=True).strip()
digest = hashlib.sha256()
with binary.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)
metadata = {
    "date_utc": stamp,
    "binary": str(binary),
    "version": info["CFBundleShortVersionString"],
    "build": info["CFBundleVersion"],
    "binary_architectures": archs,
    "host_architecture": platform.machine(),
    "macOS": platform.mac_ver()[0],
    "binary_sha256": digest.hexdigest(),
    "deadline_seconds": 180,
    "space_transfers": with_spaces,
}


def preferences():
    result = subprocess.run(["/usr/bin/defaults", "export", domain, "-"], capture_output=True, timeout=10)
    if result.returncode:
        missing = any(text in result.stderr.lower() for text in (b"does not exist", b"not found"))
        if result.returncode == 1 and missing:
            return {}
        raise RuntimeError("No se ha podido leer el dominio persistente de preferencias.")
    return plistlib.loads(result.stdout)


cases = [
    ("tab-controls", ["--check-tab-controls"]),
    ("localizations-es", ["-app.language", "es", "--check-localizations"]),
    ("localizations-en", ["-app.language", "en", "--check-localizations"]),
    ("tab-workflows-and-close", ["--check-tab-close"]),
    ("priority-shortcuts", ["--check-priority-shortcuts"]),
]
if with_spaces:
    cases += [("tab-groups", ["--check-tab-groups"]), ("space-transfer", ["--check-space-transfer"])]
results = []
try:
    before = preferences()
    prefs_error = None
except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
    before = None
    prefs_error = str(error)

for name, args in cases:
    started = time.monotonic()
    status = "FAIL"
    detail = ""
    process = None
    try:
        with (out / f"{name}.log").open("wb") as log:
            process = subprocess.Popen([str(binary), *args], cwd=out, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = process.wait(timeout=metadata["deadline_seconds"])
                status = "PASS" if code == 0 else "FAIL"
                detail = f"exit={code}"
            except subprocess.TimeoutExpired:
                detail = "deadline excedido"
    except OSError as error:
        detail = str(error)
    finally:
        if process is not None:
            # Solo el grupo creado por este caso: incluye sus fixtures, no la app instalada.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
    row = {"case": name, "status": status, "seconds": round(time.monotonic() - started, 2), "detail": detail}
    results.append(row)
    print(f"{status} {name}: {detail} ({row['seconds']}s)", flush=True)

try:
    after = preferences()
    if before is None:
        raise RuntimeError(prefs_error)
    changed = sorted(key for key in before.keys() | after.keys() if (key in before) != (key in after) or before.get(key) != after.get(key))
    prefs_status = "FAIL" if changed else "PASS"
    # Solo nombres de claves; ni valores ni snapshots persistentes en el informe.
    prefs_detail = f"Claves modificadas: {', '.join(changed)}" if changed else "Dominio persistente idéntico; sin excepciones."
except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
    prefs_status, prefs_detail = "FAIL", str(error)
results.append({"case": "persistent-preferences", "status": prefs_status, "detail": prefs_detail})
(out / ".cache").rename(out / "captures")
passed = all(row["status"] == "PASS" for row in results)
report = {"metadata": metadata, "results": results, "status": "PASS" if passed else "FAIL"}
(out / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
lines = [f"AMSpace {metadata['version']} build {metadata['build']} · {stamp}",
         f"Binario: {binary}", f"SHA-256: {metadata['binary_sha256']}",
         f"macOS {metadata['macOS']} · anfitrión {metadata['host_architecture']} · bundle {archs}",
         *[f"{r['status']} {r['case']}: {r['detail']}" for r in results],
         f"RESULTADO GLOBAL: {report['status']}", "Capturas propias: captures/; logs por caso; sin snapshots de preferencias."]
(out / "report.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"{prefs_status} persistent-preferences: {prefs_detail}")
print(f"RESULTADO GLOBAL: {report['status']} · {out}")
sys.exit(0 if passed else 1)
PY
