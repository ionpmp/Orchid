#!/usr/bin/env python3
"""Export Orchid logo PNGs from SVG with transparency and rebuild the Windows ICO."""

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SVG = ROOT / "assets/logo/orchid-logo.svg"


def export_pngs() -> None:
    import cairosvg

    targets = [
        ROOT / "assets/logo/orchid-logo.png",
        ROOT / "crates/orchid-ui/ui/assets/orchid-icon.png",
        ROOT / "assets/logo/options/orchid-logo-j-compass.png",
    ]
    svg_sources = [
        SVG,
        SVG,
        ROOT / "assets/logo/options/orchid-logo-j-compass.svg",
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


def build_ico(png_path: Path, ico_path: Path) -> None:
    img = Image.open(png_path).convert("RGBA")
    sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    icons = [img.resize(s, Image.Resampling.LANCZOS) for s in sizes]
    icons[0].save(
        ico_path,
        format="ICO",
        sizes=[(i.width, i.height) for i in icons],
    )
    print(f"wrote {ico_path}")


def main() -> None:
    export_pngs()
    icon_png = ROOT / "crates/orchid-ui/ui/assets/orchid-icon.png"
    build_ico(icon_png, ROOT / "assets/logo/orchid-icon.ico")


if __name__ == "__main__":
    main()
