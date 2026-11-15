#!/usr/bin/env python3
"""Generate branded installer artwork for macOS DMG and Windows NSIS/WiX bundles."""

from __future__ import annotations

import math
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
ICON_PATH = ROOT / "apps" / "desktop" / "src-tauri" / "icons" / "icon.png"
OUT_DIR = ROOT / "apps" / "desktop" / "src-tauri" / "installer"

# Kryptos brand palette (aligned with docs/sito/style.css)
BG_DARK = (11, 12, 14)       # #0b0c0e
BG_SURFACE = (20, 22, 26)    # #14161a
BG_ELEVATED = (26, 29, 34)   # #1a1d22
GOLD = (214, 161, 102)       # #d6a166
GOLD_LIGHT = (240, 198, 144) # #f0c690
GOLD_DARK = (168, 114, 56)   # #a87238
TEXT_LIGHT = (243, 239, 232) # #f3efe8
TEXT_MUTED = (185, 180, 170) # #b9b4aa
TEXT_DIM = (125, 122, 115)   # #7d7a73
PANEL_WHITE = (255, 255, 255) # Matches native Windows NSIS/WiX white background seamlessly

SCALE = 2


def load_font(size: int, weight: str = "regular") -> ImageFont.FreeTypeFont:
    """Load macOS system UI font at the requested pixel size."""
    candidates = {
        "bold": [
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            "/System/Library/Fonts/SFNS.ttf",
        ],
        "semibold": [
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        ],
        "regular": [
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
        ],
    }[weight]
    for path in candidates:
        if Path(path).exists():
            return ImageFont.truetype(path, size * SCALE)
    return ImageFont.load_default()


def add_radial_glow(
    canvas: Image.Image,
    center: tuple[int, int],
    radius: int,
    color: tuple[int, int, int],
    max_alpha: int,
) -> None:
    """Composite a smooth radial glow onto an RGBA canvas (coordinates in 1x px)."""
    cx, cy = center[0] * SCALE, center[1] * SCALE
    r = radius * SCALE
    glow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(glow)
    steps = 36
    for i in range(steps, 0, -1):
        frac = i / steps
        curr_r = int(r * frac)
        alpha = int(max_alpha * ((1.0 - frac) ** 2.1))
        if alpha <= 0 or curr_r <= 0:
            continue
        draw.ellipse(
            (cx - curr_r, cy - curr_r, cx + curr_r, cy + curr_r),
            fill=(*color, alpha),
        )
    canvas.alpha_composite(glow)


def draw_vault_rings(
    canvas: Image.Image,
    center: tuple[int, int],
    radii: list[int],
    alpha: int = 18,
) -> None:
    """Draw faint concentric vault-dial rings centered at the given 1x point."""
    cx, cy = center[0] * SCALE, center[1] * SCALE
    overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    for r in radii:
        rs = r * SCALE
        draw.ellipse(
            (cx - rs, cy - rs, cx + rs, cy + rs),
            outline=(*GOLD, alpha),
            width=1 * SCALE,
        )
    # Subtle cardinal tick marks on outermost ring
    if radii:
        outer = radii[-1] * SCALE
        for deg in range(0, 360, 15):
            rad = math.radians(deg)
            inner = outer - (6 * SCALE if deg % 45 == 0 else 3 * SCALE)
            x0, y0 = cx + math.cos(rad) * inner, cy + math.sin(rad) * inner
            x1, y1 = cx + math.cos(rad) * outer, cy + math.sin(rad) * outer
            draw.line((x0, y0, x1, y1), fill=(*GOLD, alpha + 8), width=1 * SCALE)
    canvas.alpha_composite(overlay)


def draw_centered_text(
    draw: ImageDraw.ImageDraw,
    center_x: int,
    y: int,
    text: str,
    font: ImageFont.FreeTypeFont,
    fill: tuple[int, int, int] | tuple[int, int, int, int],
) -> None:
    """Draw horizontally centered text at 1x coordinates."""
    bbox = draw.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    draw.text((center_x * SCALE - tw // 2, y * SCALE), text, font=font, fill=fill)


def draw_pill(
    canvas: Image.Image,
    center_x: int,
    y: int,
    text: str,
    font: ImageFont.FreeTypeFont,
    fill_bg: tuple[int, int, int, int],
    outline: tuple[int, int, int, int],
    text_color: tuple[int, int, int],
    pad_x: int = 12,
    pad_y: int = 5,
) -> None:
    """Draw a rounded pill badge alpha-composited cleanly centered at center_x (1x px)."""
    probe = ImageDraw.Draw(canvas)
    bbox = probe.textbbox((0, 0), text, font=font)
    tw = bbox[2] - bbox[0]
    th = bbox[3] - bbox[1]
    w = tw + pad_x * 2 * SCALE
    h = th + pad_y * 2 * SCALE
    x0 = center_x * SCALE - w // 2
    y0 = y * SCALE
    overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    odraw = ImageDraw.Draw(overlay)
    odraw.rounded_rectangle(
        (x0, y0, x0 + w, y0 + h),
        radius=h // 2,
        fill=fill_bg,
        outline=outline,
        width=1 * SCALE,
    )
    canvas.alpha_composite(overlay)
    tdraw = ImageDraw.Draw(canvas)
    tdraw.text(
        (x0 + pad_x * SCALE - bbox[0], y0 + pad_y * SCALE - bbox[1]),
        text,
        font=font,
        fill=text_color,
    )


def load_icon(size_1x: int) -> Image.Image:
    """Load Kryptos PNG icon scaled cleanly for 2x supersampled drawing."""
    icon = Image.open(ICON_PATH).convert("RGBA")
    return icon.resize((size_1x * SCALE, size_1x * SCALE), Image.Resampling.LANCZOS)


def build_dmg_background() -> None:
    """Create the 660x420 macOS DMG window background PNG."""
    w, h = 660, 420
    canvas = Image.new("RGBA", (w * SCALE, h * SCALE), (*BG_DARK, 255))

    # Ambient golden radial glows + faint vault dial rings
    add_radial_glow(canvas, (330, 175), 320, GOLD, max_alpha=36)
    add_radial_glow(canvas, (180, 210), 120, GOLD_LIGHT, max_alpha=24)
    add_radial_glow(canvas, (480, 210), 110, (95, 165, 225), max_alpha=18)
    draw_vault_rings(canvas, (330, 210), [115, 170, 225, 285], alpha=16)

    overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)

    # Drop zone glass cards aligned behind Finder icon centers (180, 210) & (480, 210)
    for cx in (180, 480):
        card_w, card_h = 164, 172
        x0 = (cx - card_w // 2) * SCALE
        y0 = (212 - card_h // 2) * SCALE
        x1 = (cx + card_w // 2) * SCALE
        y1 = (212 + card_h // 2) * SCALE
        draw.rounded_rectangle(
            (x0, y0, x1, y1),
            radius=22 * SCALE,
            fill=(*BG_SURFACE, 205),
            outline=(*GOLD, 62),
            width=1 * SCALE,
        )
        # Subtle inner top highlight line
        draw.line(
            (x0 + 26 * SCALE, y0 + 1 * SCALE, x1 - 26 * SCALE, y0 + 1 * SCALE),
            fill=(*GOLD_LIGHT, 55),
            width=1 * SCALE,
        )
        # Bright white/ivory label pill behind Finder's black icon label text (y=274..285)
        lbl_w, lbl_h = 124, 25
        lx0 = (cx - lbl_w // 2) * SCALE
        ly0 = 266 * SCALE
        lx1 = (cx + lbl_w // 2) * SCALE
        ly1 = (266 + lbl_h) * SCALE
        draw.rounded_rectangle(
            (lx0, ly0, lx1, ly1),
            radius=(lbl_h // 2) * SCALE,
            fill=(246, 243, 236, 248),
            outline=(*GOLD, 210),
            width=1 * SCALE,
        )

    # Center arrow linking Kryptos.app -> Applications
    arrow_y = 204 * SCALE
    ax0, ax1 = 276 * SCALE, 384 * SCALE
    # Soft glow line under arrow
    draw.line((ax0, arrow_y, ax1 - 6 * SCALE, arrow_y), fill=(*GOLD, 210), width=3 * SCALE)
    head = [
        (ax1, arrow_y),
        (ax1 - 12 * SCALE, arrow_y - 8 * SCALE),
        (ax1 - 9 * SCALE, arrow_y),
        (ax1 - 12 * SCALE, arrow_y + 8 * SCALE),
    ]
    draw.polygon(head, fill=(*GOLD_LIGHT, 240))

    canvas.alpha_composite(overlay)

    # Small brand emblem + header wordmark at top
    draw = ImageDraw.Draw(canvas)
    title_font = load_font(22, "bold")
    badge_font = load_font(11, "bold")
    sub_font = load_font(12, "regular")
    hint_font = load_font(11, "semibold")
    foot_font = load_font(11, "regular")

    # Header row: small icon + "Kryptos" + gold "v0.2" pill centered as a unit
    small_icon = load_icon(26)
    title_text = "Kryptos"
    tb = draw.textbbox((0, 0), title_text, font=title_font)
    title_w = tb[2] - tb[0]
    pill_text = "v0.2"
    pb = draw.textbbox((0, 0), pill_text, font=badge_font)
    pill_w = (pb[2] - pb[0]) + 16 * SCALE
    gap = 10 * SCALE
    total_w = small_icon.width + gap + title_w + 8 * SCALE + pill_w
    start_x = (w * SCALE - total_w) // 2
    top_y = 32 * SCALE

    canvas.alpha_composite(small_icon, (start_x, top_y))
    draw = ImageDraw.Draw(canvas)
    draw.text(
        (start_x + small_icon.width + gap, top_y + 1 * SCALE - tb[1]),
        title_text,
        font=title_font,
        fill=TEXT_LIGHT,
    )
    pill_x = start_x + small_icon.width + gap + title_w + 10 * SCALE
    pill_h = 20 * SCALE
    pill_y = top_y + (small_icon.height - pill_h) // 2
    pill_overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    pdraw = ImageDraw.Draw(pill_overlay)
    pdraw.rounded_rectangle(
        (pill_x, pill_y, pill_x + pill_w, pill_y + pill_h),
        radius=pill_h // 2,
        fill=(*GOLD, 34),
        outline=(*GOLD, 165),
        width=1 * SCALE,
    )
    canvas.alpha_composite(pill_overlay)
    draw = ImageDraw.Draw(canvas)
    draw.text(
        (pill_x + 8 * SCALE - pb[0], pill_y + (pill_h - (pb[3] - pb[1])) // 2 - pb[1]),
        pill_text,
        font=badge_font,
        fill=GOLD_LIGHT,
    )

    draw_centered_text(
        draw,
        330,
        74,
        "Drag Kryptos to Applications to install",
        sub_font,
        TEXT_MUTED,
    )

    # Small label under arrow
    draw_centered_text(draw, 330, 222, "INSTALL", hint_font, (*GOLD, 210))

    # Footer security badge safely inside both 660x400 and 660x420 heights
    draw_pill(
        canvas,
        330,
        342,
        "100% Offline  ·  Zero Server  ·  Argon2id + XChaCha20-Poly1305",
        foot_font,
        fill_bg=(*BG_ELEVATED, 215),
        outline=(*GOLD, 55),
        text_color=TEXT_MUTED,
        pad_x=16,
        pad_y=7,
    )

    out = canvas.resize((w, h), Image.Resampling.LANCZOS).convert("RGB")
    out.save(OUT_DIR / "dmg-background.png", format="PNG", optimize=True)


def build_sidebar(width: int = 164, height: int = 314) -> Image.Image:
    """Create the 164x314 vertical dark-obsidian Kryptos sidebar in 2x RGBA."""
    canvas = Image.new("RGBA", (width * SCALE, height * SCALE), (*BG_DARK, 255))
    add_radial_glow(canvas, (width // 2, 96), 118, GOLD, max_alpha=46)
    draw_vault_rings(canvas, (width // 2, 96), [46, 68, 92], alpha=20)

    icon = load_icon(74)
    canvas.alpha_composite(icon, ((width * SCALE - icon.width) // 2, 58 * SCALE))

    draw = ImageDraw.Draw(canvas)
    name_font = load_font(18, "bold")
    ver_font = load_font(10, "bold")
    item_font = load_font(10, "semibold")
    foot_font = load_font(9, "regular")

    draw_centered_text(draw, width // 2, 146, "KRYPTOS", name_font, TEXT_LIGHT)
    draw_pill(
        canvas,
        width // 2,
        175,
        "v0.2",
        ver_font,
        fill_bg=(*GOLD, 34),
        outline=(*GOLD, 165),
        text_color=GOLD_LIGHT,
        pad_x=12,
        pad_y=4,
    )

    # Gold divider line
    line_overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ldraw = ImageDraw.Draw(line_overlay)
    ldraw.line(
        (26 * SCALE, 208 * SCALE, (width - 26) * SCALE, 208 * SCALE),
        fill=(*GOLD, 90),
        width=1 * SCALE,
    )
    canvas.alpha_composite(line_overlay)
    draw = ImageDraw.Draw(canvas)

    bullets = [
        "100% Offline",
        "Zero Server",
        "Argon2id + AEAD",
    ]
    y = 224
    for label in bullets:
        dot_x = 26 * SCALE
        dot_y = (y + 3) * SCALE
        draw.ellipse(
            (dot_x, dot_y, dot_x + 5 * SCALE, dot_y + 5 * SCALE),
            fill=GOLD_LIGHT,
        )
        draw.text((38 * SCALE, y * SCALE), label, font=item_font, fill=TEXT_MUTED)
        y += 22

    draw_centered_text(draw, width // 2, 294, "GPL-3.0  ·  Open Source", foot_font, TEXT_DIM)

    # Gold accent border on the right seam
    border_overlay = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    ImageDraw.Draw(border_overlay).line(
        ((width - 1) * SCALE, 0, (width - 1) * SCALE, height * SCALE),
        fill=(*GOLD, 160),
        width=1 * SCALE,
    )
    canvas.alpha_composite(border_overlay)
    return canvas


def build_nsis_assets() -> None:
    """Generate NSIS 164x314 sidebar and 150x57 header BMPs (24-bit RGB)."""
    sidebar = build_sidebar(164, 314).resize((164, 314), Image.Resampling.LANCZOS).convert("RGB")
    sidebar.save(OUT_DIR / "nsis-sidebar.bmp", format="BMP")

    # NSIS header image (150 x 57): white surface matching NSIS header + dark brand badge
    w, h = 150, 57
    canvas = Image.new("RGBA", (w * SCALE, h * SCALE), (*PANEL_WHITE, 255))
    draw = ImageDraw.Draw(canvas)

    # Compact obsidian brand pill on right side of header box
    pill_x0, pill_y0, pill_x1, pill_y1 = 18 * SCALE, 9 * SCALE, 142 * SCALE, 46 * SCALE
    draw.rounded_rectangle(
        (pill_x0, pill_y0, pill_x1, pill_y1),
        radius=11 * SCALE,
        fill=(*BG_DARK, 255),
        outline=(*GOLD, 210),
        width=1 * SCALE,
    )
    icon = load_icon(26)
    canvas.alpha_composite(icon, (pill_x0 + 7 * SCALE, pill_y0 + 5 * SCALE))
    draw = ImageDraw.Draw(canvas)
    font = load_font(13, "bold")
    tb = draw.textbbox((0, 0), "Kryptos", font=font)
    draw.text(
        (pill_x0 + 40 * SCALE, pill_y0 + ((pill_y1 - pill_y0) - (tb[3] - tb[1])) // 2 - tb[1]),
        "Kryptos",
        font=font,
        fill=TEXT_LIGHT,
    )

    # Bottom gold hairline across header
    draw.rectangle((0, (h - 2) * SCALE, w * SCALE, h * SCALE), fill=(*GOLD, 255))

    hdr = canvas.resize((w, h), Image.Resampling.LANCZOS).convert("RGB")
    hdr.save(OUT_DIR / "nsis-header.bmp", format="BMP")


def build_wix_assets() -> None:
    """Generate WiX 493x312 dialog and 493x58 banner BMPs (24-bit RGB)."""
    # WiX Welcome/Finish Dialog (493 x 312): 164px dark sidebar + 329px pure-white text panel
    w, h = 493, 312
    dialog = Image.new("RGBA", (w * SCALE, h * SCALE), (*PANEL_WHITE, 255))
    sidebar = build_sidebar(164, 312).convert("RGB").convert("RGBA")
    dialog.alpha_composite(sidebar, (0, 0))
    draw = ImageDraw.Draw(dialog)
    # Crisp bottom hairline on white panel so WiX black welcome/finish text stays sharp
    draw.rectangle((164 * SCALE, 0, w * SCALE, 3 * SCALE), fill=(*GOLD, 255))
    draw.rectangle((164 * SCALE, (h - 2) * SCALE, w * SCALE, h * SCALE), fill=(*GOLD, 200))

    dialog.resize((w, h), Image.Resampling.LANCZOS).convert("RGB").save(
        OUT_DIR / "wix-dialog.bmp", format="BMP"
    )

    # WiX Banner (493 x 58): clean white left for black title/subtitle, branded emblem on right
    bw, bh = 493, 58
    banner = Image.new("RGBA", (bw * SCALE, bh * SCALE), (*PANEL_WHITE, 255))
    draw = ImageDraw.Draw(banner)
    pill_x0, pill_y0, pill_x1, pill_y1 = 362 * SCALE, 9 * SCALE, 481 * SCALE, 46 * SCALE
    draw.rounded_rectangle(
        (pill_x0, pill_y0, pill_x1, pill_y1),
        radius=11 * SCALE,
        fill=(*BG_DARK, 255),
        outline=(*GOLD, 210),
        width=1 * SCALE,
    )
    icon = load_icon(26)
    banner.alpha_composite(icon, (pill_x0 + 7 * SCALE, pill_y0 + 5 * SCALE))
    draw = ImageDraw.Draw(banner)
    font = load_font(12, "bold")
    tb = draw.textbbox((0, 0), "Kryptos", font=font)
    draw.text(
        (pill_x0 + 39 * SCALE, pill_y0 + ((pill_y1 - pill_y0) - (tb[3] - tb[1])) // 2 - tb[1]),
        "Kryptos",
        font=font,
        fill=TEXT_LIGHT,
    )
    # Bottom gold hairline across banner
    draw.rectangle((0, (bh - 2) * SCALE, bw * SCALE, bh * SCALE), fill=(*GOLD, 255))

    banner.resize((bw, bh), Image.Resampling.LANCZOS).convert("RGB").save(
        OUT_DIR / "wix-banner.bmp", format="BMP"
    )


def main() -> None:
    """Generate all macOS and Windows installer image assets."""
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    build_dmg_background()
    build_nsis_assets()
    build_wix_assets()
    for path in sorted(OUT_DIR.iterdir()):
        with Image.open(path) as img:
            print(f"{path.name}: {img.size[0]}x{img.size[1]} ({img.mode}, {img.format})")


if __name__ == "__main__":
    main()
