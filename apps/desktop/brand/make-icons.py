#!/usr/bin/env python3
"""Regenerates every Kryptos icon from one full-bleed square logo.

Usage (from apps/desktop):
    python3 brand/make-icons.py path/to/logo.(png|webp|jpg)
    npx tauri icon icon.png -o src-tauri/icons   # desktop + iOS sets from the macOS-style icon.png
    python3 brand/make-icons.py --android-only   # re-apply full-bleed Android icons (tauri icon overwrites them)
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
APP = HERE.parent
MASTER = HERE / "logo-source.png"
ANDROID_RES = APP / "src-tauri/gen/android/app/src/main/res"
EXT_ICONS = APP.parent.parent / "extension/icons"


def rounded(img: Image.Image, size: int, radius_ratio: float) -> Image.Image:
    s = 4  # supersampled mask for smooth corners
    m = Image.new("L", (size * s, size * s), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, size * s - 1, size * s - 1), radius=int(size * s * radius_ratio), fill=255)
    out = img.resize((size, size), Image.LANCZOS).convert("RGBA")
    out.putalpha(m.resize((size, size), Image.LANCZOS))
    return out


def android(full: Image.Image) -> None:
    # Adaptive icons are 108dp layers of which only the central ~72dp is visible (then masked
    # to a circle/squircle). Scale the artwork to 76dp so the whole logo sits in the visible area;
    # the gradient-coloured background layer fills the rest.
    for d, px in {"mdpi": 108, "hdpi": 162, "xhdpi": 216, "xxhdpi": 324, "xxxhdpi": 432}.items():
        folder = ANDROID_RES / f"mipmap-{d}"
        if folder.exists():
            art = round(px * 76 / 108)
            layer = Image.new("RGBA", (px, px), (0, 0, 0, 0))
            layer.paste(full.resize((art, art), Image.LANCZOS), ((px - art) // 2, (px - art) // 2))
            layer.save(folder / "ic_launcher_foreground.png")


def main() -> None:
    if sys.argv[1:] == ["--android-only"]:
        android(Image.open(MASTER).convert("RGB"))
        return
    full = Image.open(sys.argv[1]).convert("RGB")
    w, h = full.size
    side = min(w, h)  # center-crop to square
    full = full.crop(((w - side) // 2, (h - side) // 2, (w + side) // 2, (h + side) // 2)).resize((1024, 1024), Image.LANCZOS)
    full.save(MASTER)

    # macOS Big Sur+ grid: 824px body in a 1024 canvas, ~22.5% corner radius.
    icon = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    icon.alpha_composite(rounded(full, 824, 0.225), (100, 100))
    icon.save(APP / "icon.png")

    ext = rounded(full, 1024, 0.22)
    for n in (16, 32, 48, 128):
        ext.resize((n, n), Image.LANCZOS).save(EXT_ICONS / f"{n}.png")

    full.resize((256, 256), Image.LANCZOS).save(APP / "src/assets/logo.png", optimize=True)
    android(full)


if __name__ == "__main__":
    main()
