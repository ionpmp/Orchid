#!/usr/bin/env python3
"""Export Orchid logo PNGs from SVG with transparency and rebuild the Windows ICO."""

from __future__ import annotations

import struct
from io import BytesIO
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SVG = ROOT / "assets/logo/orchid-logo.svg"


def export_pngs() -> None:
    import cairosvg

    targets = [
        ROOT / "assets/logo/orchid-logo.png",
        ROOT / "crates/orchid-ui/ui/assets/orchid-icon.png",
        ROOT / "assets/logo/options/orchid-logo-n-crest.png",
    ]
    svg_sources = [
        SVG,
        SVG,
        ROOT / "assets/logo/options/orchid-logo-n-crest.svg",
    ]
    for svg_path, png_path in zip(svg_sources, targets, strict=True):
        png_path.parent.mkdir(parents=True, exist_ok=True)
        cairosvg.svg2png(
            url=str(svg_path),
            write_to=str(png_path),
            output_width=512,
            output_height=512,
            background_color="transparent",
        )
        print(f"exported {png_path}")


def _bmp_xor_and(img: Image.Image) -> bytes:
    """Windows ICO BMP payload: BITMAPINFOHEADER + BGRA XOR + AND mask."""
    w, h = img.size
    rgba = img.convert("RGBA")
    pixels = list(rgba.getdata())
    xor = bytearray()
    for y in range(h - 1, -1, -1):
        for r, g, b, a in pixels[y * w : (y + 1) * w]:
            xor.extend((b, g, r, a))
    row_bytes = ((w + 31) // 32) * 4
    and_mask = bytes(row_bytes * h)
    header = struct.pack(
        "<IIIHHIIIIII",
        40,
        w,
        h * 2,
        1,
        32,
        0,
        len(xor),
        0,
        0,
        0,
        0,
    )
    return header + bytes(xor) + and_mask


def _png_bytes(img: Image.Image) -> bytes:
    buf = BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue()


def build_ico(png_path: Path, ico_path: Path) -> None:
    """Write a real multi-size ICO (BMP 16-48 + PNG 64-256).

    Pillow's ``img.save(..., format="ICO")`` often emits a single tiny frame when
    fed a pre-resized image — Windows then keeps showing the old desktop glyph.
    """
    base = Image.open(png_path).convert("RGBA")
    entries: list[tuple[int, int, bytes]] = []
    for size in (16, 32, 48):
        im = base.resize((size, size), Image.Resampling.LANCZOS)
        entries.append((size, size, _bmp_xor_and(im)))
    for size in (64, 128, 256):
        im = base.resize((size, size), Image.Resampling.LANCZOS)
        entries.append((size, size, _png_bytes(im)))

    count = len(entries)
    offset = 6 + 16 * count
    header = struct.pack("<HHH", 0, 1, count)
    directory = bytearray()
    blobs = bytearray()
    for w, h, data in entries:
        directory.extend(
            struct.pack(
                "<BBBBHHII",
                w if w < 256 else 0,
                h if h < 256 else 0,
                0,
                0,
                1,
                32,
                len(data),
                offset + len(blobs),
            )
        )
        blobs.extend(data)

    ico_path.write_bytes(header + directory + blobs)
    print(f"wrote {ico_path} ({ico_path.stat().st_size} bytes, {count} images)")


def main() -> None:
    export_pngs()
    icon_png = ROOT / "crates/orchid-ui/ui/assets/orchid-icon.png"
    build_ico(icon_png, ROOT / "assets/logo/orchid-icon.ico")


if __name__ == "__main__":
    main()
