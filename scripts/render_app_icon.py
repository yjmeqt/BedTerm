#!/usr/bin/env python3
"""Render BedTerm app icon at 1024×1024 in Light / Dark / Tinted variants."""

from __future__ import annotations
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

SIZE = 1024
OUT = Path(__file__).resolve().parent.parent / "BedTerm" / "Assets.xcassets" / "AppIcon.appiconset"
OUT.mkdir(parents=True, exist_ok=True)

FONT_CANDIDATES = [
    "/System/Library/Fonts/SFNSMono.ttf",
    "/Library/Fonts/SF-Mono-Bold.otf",
    "/System/Library/Fonts/Supplemental/Menlo.ttc",
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Supplemental/Andale Mono.ttf",
]


def load_font(size: int) -> ImageFont.FreeTypeFont:
    for path in FONT_CANDIDATES:
        if Path(path).exists():
            try:
                return ImageFont.truetype(path, size=size)
            except OSError:
                continue
    return ImageFont.load_default()


def vertical_gradient(top: tuple[int, int, int], bottom: tuple[int, int, int]) -> Image.Image:
    img = Image.new("RGB", (SIZE, SIZE), top)
    px = img.load()
    for y in range(SIZE):
        t = y / (SIZE - 1)
        r = int(top[0] + (bottom[0] - top[0]) * t)
        g = int(top[1] + (bottom[1] - top[1]) * t)
        b = int(top[2] + (bottom[2] - top[2]) * t)
        for x in range(SIZE):
            px[x, y] = (r, g, b)
    return img


def radial_glow(center: tuple[int, int], radius: int, color: tuple[int, int, int], alpha_peak: int) -> Image.Image:
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(glow)
    cx, cy = center
    # Build glow via concentric circles for a soft falloff.
    steps = 32
    for i in range(steps, 0, -1):
        r = int(radius * i / steps)
        a = int(alpha_peak * (1 - (i / steps)) ** 1.6)
        draw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(*color, a))
    return glow.filter(ImageFilter.GaussianBlur(radius * 0.18))


PROMPT_CY_OFFSET = 170  # shift prompt down to make room for moon above


def _draw_prompt_shapes(layer: Image.Image, color: tuple[int, int, int], alpha: int = 255) -> None:
    """Draw '>_' as geometric primitives — centered horizontally, pushed below the moon."""
    d = ImageDraw.Draw(layer)
    cx = SIZE / 2
    cy = SIZE / 2 + PROMPT_CY_OFFSET

    chev_h = 360
    chev_w = 200
    stroke = 78
    chev_cx = cx - 130
    apex_x = chev_cx + chev_w / 2
    top    = (chev_cx - chev_w / 2, cy - chev_h / 2)
    apex   = (apex_x,                cy)
    bottom = (chev_cx - chev_w / 2, cy + chev_h / 2)
    fill = (*color, alpha)

    d.line([top, apex],    fill=fill, width=stroke, joint="curve")
    d.line([apex, bottom], fill=fill, width=stroke, joint="curve")
    r = stroke / 2
    for (px, py) in (top, apex, bottom):
        d.ellipse((px - r, py - r, px + r, py + r), fill=fill)

    bar_w = 280
    bar_h = 78
    bar_cx = cx + 180
    bar_cy = cy + chev_h / 2 - bar_h / 2
    d.rounded_rectangle(
        (bar_cx - bar_w / 2, bar_cy - bar_h / 2, bar_cx + bar_w / 2, bar_cy + bar_h / 2),
        radius=bar_h / 2,
        fill=fill,
    )


def _draw_crescent(layer: Image.Image, color: tuple[int, int, int], alpha: int = 255,
                   center: tuple[float, float] = (SIZE * 0.62, SIZE * 0.30),
                   radius: float = 200, offset: float = 70) -> None:
    """Crescent moon: large filled disc minus a smaller offset disc."""
    cx, cy = center
    moon = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    md = ImageDraw.Draw(moon)
    md.ellipse((cx - radius, cy - radius, cx + radius, cy + radius), fill=(*color, alpha))
    cut = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    cd = ImageDraw.Draw(cut)
    # Offset cut disc up-right so crescent opens to lower-left
    cd.ellipse(
        (cx - radius + offset, cy - radius - offset * 0.5,
         cx + radius + offset, cy + radius - offset * 0.5),
        fill=(0, 0, 0, 255),
    )
    # Subtract cut from moon via alpha
    moon_rgba = moon.split()
    cut_alpha = cut.split()[-1]
    # final alpha = moon_alpha * (1 - cut_alpha/255)
    new_alpha = Image.eval(cut_alpha, lambda v: 255 - v)
    final_alpha = ImageChops.multiply(moon_rgba[3], new_alpha)
    moon.putalpha(final_alpha)
    layer.alpha_composite(moon)


def _draw_stars(layer: Image.Image, color=(255, 255, 255)) -> None:
    """A few small sparkle dots in the night sky."""
    d = ImageDraw.Draw(layer)
    stars = [
        (SIZE * 0.18, SIZE * 0.22, 7, 200),
        (SIZE * 0.30, SIZE * 0.12, 5, 150),
        (SIZE * 0.22, SIZE * 0.40, 4, 130),
        (SIZE * 0.85, SIZE * 0.55, 6, 170),
        (SIZE * 0.78, SIZE * 0.20, 4, 140),
    ]
    for x, y, r, a in stars:
        d.ellipse((x - r, y - r, x + r, y + r), fill=(*color, a))


def draw_prompt(canvas: Image.Image, fg: tuple[int, int, int], glow_color: tuple[int, int, int] | None) -> None:
    """Render '>_' prompt with optional outer glow."""
    if glow_color is not None:
        for blur, alpha in ((60, 110), (24, 160)):
            glow_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
            _draw_prompt_shapes(glow_layer, glow_color, alpha=alpha)
            glow_layer = glow_layer.filter(ImageFilter.GaussianBlur(blur))
            canvas.alpha_composite(glow_layer)
    fg_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    _draw_prompt_shapes(fg_layer, fg, alpha=255)
    canvas.alpha_composite(fg_layer)


def add_inner_shadow(canvas: Image.Image, color=(0, 0, 0), alpha=70, blur=80) -> None:
    """Soft vignette around the edges to give depth (Liquid-Glass-ish)."""
    mask = Image.new("L", (SIZE, SIZE), 0)
    md = ImageDraw.Draw(mask)
    pad = 0
    md.rectangle((pad, pad, SIZE - pad, SIZE - pad), fill=255)
    md = mask.filter(ImageFilter.GaussianBlur(blur))
    inv = Image.eval(mask, lambda v: 255 - v)
    overlay = Image.new("RGBA", (SIZE, SIZE), (*color, alpha))
    overlay.putalpha(inv)
    canvas.alpha_composite(overlay)


MOON_CREAM = (248, 226, 168)
PROMPT_MINT = (170, 255, 220)
PROMPT_INK  = (28, 32, 70)        # deep navy for the daytime variant
SUN_YELLOW  = (255, 214, 96)
SUN_GLOW    = (255, 230, 150)


def _add_moon_with_glow(canvas: Image.Image, moon_color, glow_color, glow_alpha=180) -> None:
    center = (SIZE * 0.66, SIZE * 0.30)
    radius = 195
    offset = 68
    glow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    _draw_crescent(glow, glow_color, alpha=glow_alpha, center=center, radius=radius, offset=offset)
    canvas.alpha_composite(glow.filter(ImageFilter.GaussianBlur(46)))
    _draw_crescent(canvas, moon_color, alpha=255, center=center, radius=radius, offset=offset)


def _add_sun_with_glow(canvas: Image.Image, sun_color, glow_color, glow_alpha=180) -> None:
    """Solid sun disc with soft halo — mirrors the moon's placement."""
    center = (SIZE * 0.66, SIZE * 0.30)
    radius = 170
    # Outer halo
    halo = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    hd = ImageDraw.Draw(halo)
    hd.ellipse(
        (center[0] - radius - 70, center[1] - radius - 70,
         center[0] + radius + 70, center[1] + radius + 70),
        fill=(*glow_color, glow_alpha),
    )
    canvas.alpha_composite(halo.filter(ImageFilter.GaussianBlur(60)))
    # Sun body
    body = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    bd = ImageDraw.Draw(body)
    bd.ellipse(
        (center[0] - radius, center[1] - radius, center[0] + radius, center[1] + radius),
        fill=(*sun_color, 255),
    )
    canvas.alpha_composite(body)


def render_light() -> Image.Image:
    # Daytime sky: soft blue at top, warm cream at horizon
    bg = vertical_gradient((150, 200, 240), (255, 230, 195)).convert("RGBA")
    # Soft sun-warm overlay in the upper-right
    warm = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(warm).ellipse(
        (SIZE * 0.35, -SIZE * 0.15, SIZE * 1.15, SIZE * 0.65),
        fill=(255, 220, 170, 80),
    )
    bg.alpha_composite(warm.filter(ImageFilter.GaussianBlur(160)))
    _add_sun_with_glow(bg, SUN_YELLOW, SUN_GLOW, glow_alpha=200)
    # Soft cool wash behind the prompt so it reads cleanly
    bg.alpha_composite(
        radial_glow((SIZE // 2, SIZE // 2 + PROMPT_CY_OFFSET), 380, (220, 235, 255), 100)
    )
    draw_prompt(bg, fg=PROMPT_INK, glow_color=None)
    add_inner_shadow(bg, color=(60, 40, 0), alpha=60, blur=130)
    return bg


def render_dark() -> Image.Image:
    bg = vertical_gradient((6, 8, 22), (16, 14, 44)).convert("RGBA")
    vig = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(vig).ellipse(
        (SIZE * 0.05, SIZE * 0.05, SIZE * 0.95, SIZE * 0.95), fill=(60, 40, 130, 80)
    )
    bg.alpha_composite(vig.filter(ImageFilter.GaussianBlur(150)))
    _draw_stars(bg, color=(220, 220, 255))
    _add_moon_with_glow(bg, MOON_CREAM, MOON_CREAM, glow_alpha=200)
    bg.alpha_composite(
        radial_glow((SIZE // 2, SIZE // 2 + PROMPT_CY_OFFSET), 400, (120, 230, 180), 100)
    )
    draw_prompt(bg, fg=(160, 255, 215), glow_color=(120, 230, 180))
    add_inner_shadow(bg, color=(0, 0, 0), alpha=160, blur=150)
    return bg


def render_tinted() -> Image.Image:
    bg = vertical_gradient((38, 38, 38), (92, 92, 92)).convert("RGBA")
    _draw_stars(bg, color=(220, 220, 220))
    _add_moon_with_glow(bg, (235, 235, 235), (235, 235, 235), glow_alpha=100)
    draw_prompt(bg, fg=(235, 235, 235), glow_color=None)
    return bg


def save(img: Image.Image, name: str) -> None:
    path = OUT / name
    img.convert("RGB").save(path, format="PNG", optimize=True)
    print(f"wrote {path}")


def main() -> None:
    save(render_light(), "AppIcon-Light.png")
    save(render_dark(), "AppIcon-Dark.png")
    # Tinted typically needs alpha; iOS will composite with chosen tint.
    tinted = render_tinted()
    tinted.save(OUT / "AppIcon-Tinted.png", format="PNG", optimize=True)
    print(f"wrote {OUT / 'AppIcon-Tinted.png'}")


if __name__ == "__main__":
    main()
