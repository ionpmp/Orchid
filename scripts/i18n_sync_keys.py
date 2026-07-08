#!/usr/bin/env python3
"""Report Fluent keys present in en-US but missing from other Orchid locales.

Usage (from repo root):
    python scripts/i18n_sync_keys.py

Does not rewrite locale files — translate and insert missing keys manually (or
with a careful editor). Placeholders like `{ $name }` must stay intact.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCALES_DIR = ROOT / "crates" / "orchid-i18n" / "locales"
EN_PATH = LOCALES_DIR / "en-US" / "main.ftl"
KEY_RE = re.compile(r"^([A-Za-z0-9._-]+)\s*=", re.MULTILINE)


def keys_in(path: Path) -> set[str]:
    return set(KEY_RE.findall(path.read_text(encoding="utf-8")))


def main() -> int:
    if not EN_PATH.is_file():
        print(f"missing {EN_PATH}", file=sys.stderr)
        return 1

    en = keys_in(EN_PATH)
    print(f"en-US: {len(en)} keys")
    missing_any = False
    for locale_dir in sorted(p for p in LOCALES_DIR.iterdir() if p.is_dir()):
        if locale_dir.name == "en-US":
            continue
        ftl = locale_dir / "main.ftl"
        if not ftl.is_file():
            print(f"{locale_dir.name}: missing main.ftl")
            missing_any = True
            continue
        have = keys_in(ftl)
        missing = sorted(en - have)
        extra = sorted(have - en)
        if missing or extra:
            missing_any = True
            print(f"\n{locale_dir.name}: {len(have)} keys")
            if missing:
                print(f"  missing ({len(missing)}):")
                for k in missing:
                    print(f"    - {k}")
            if extra:
                print(f"  extra ({len(extra)}):")
                for k in extra:
                    print(f"    - {k}")
        else:
            print(f"{locale_dir.name}: OK ({len(have)} keys)")
    return 1 if missing_any else 0


if __name__ == "__main__":
    raise SystemExit(main())
