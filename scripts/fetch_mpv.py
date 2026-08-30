#!/usr/bin/env python3
"""Download the latest shinchiro Windows x64 libmpv package into third-party/mpv/win-x64.

Requires: curl (or urllib), and 7-Zip (`7z` on PATH or Program Files).

Usage (from repo root):
    python scripts/fetch_mpv.py
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "third-party" / "mpv" / "win-x64"
API = "https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest"


def find_7z() -> str:
    for candidate in (
        shutil.which("7z"),
        r"C:\Program Files\7-Zip\7z.exe",
        r"C:\Program Files (x86)\7-Zip\7z.exe",
    ):
        if candidate and Path(candidate).is_file():
            return candidate
    raise SystemExit("7-Zip not found (install 7-Zip or put 7z on PATH)")


def main() -> int:
    print(f"GET {API}")
    with urllib.request.urlopen(API) as resp:
        release = json.load(resp)
    assets = [
        a
        for a in release["assets"]
        if a["name"].startswith("mpv-dev-x86_64-") and "-v3-" not in a["name"]
    ]
    if not assets:
        raise SystemExit("no mpv-dev-x86_64 asset on latest release")
    asset = assets[0]
    url = asset["browser_download_url"]
    name = asset["name"]
    print(f"Downloading {name}")

    DEST.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="orchid-mpv-") as tmp:
        archive = Path(tmp) / name
        urllib.request.urlretrieve(url, archive)
        extract_dir = Path(tmp) / "out"
        extract_dir.mkdir()
        seven = find_7z()
        subprocess.check_call([seven, "x", "-y", f"-o{extract_dir}", str(archive)])
        dlls = list(extract_dir.rglob("*.dll"))
        if not dlls:
            raise SystemExit("archive contained no DLL")
        for dll in dlls:
            target = DEST / dll.name
            shutil.copy2(dll, target)
            print(f"  -> {target} ({target.stat().st_size} bytes)")
        (DEST / "VERSION").write_text(
            f"{release['tag_name']} {name}\n", encoding="utf-8"
        )
    print("Done. Rebuild orchid-app to stage DLLs next to orchid.exe.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
