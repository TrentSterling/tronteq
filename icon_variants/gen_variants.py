"""Generate TrontEQ icon variants from the face crop. Run: python gen_variants.py"""
import math
from PIL import Image, ImageDraw, ImageFilter

S = 512
FACE = Image.open(r"C:\trontstack\tronteq\gui\assets\icon.png").convert("RGB").resize((S, S), Image.LANCZOS)
OUT = r"C:\trontstack\tronteq\icon_variants"

# TrontEQ palette: dark bg, green->cyan
BG = (12, 20, 24)
GREEN = (46, 230, 122)
CYAN = (54, 208, 244)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def rounded_mask(size, radius):
    m = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(m)
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=255)
    return m


def framed(face_img, border_draw_fn, border=88, frame=14):
    """TrontSnap family layout: colored outer border, white inner frame, face."""
    img = Image.new("RGB", (S, S), BG)
    border_draw_fn(img)
    # white frame
    fs = S - 2 * border
    white = Image.new("RGB", (fs, fs), (255, 255, 255))
    img.paste(white, (border, border), rounded_mask(fs, 56))
    # face
    inner = fs - 2 * frame
    f = face_img.resize((inner, inner), Image.LANCZOS)
    img.paste(f, (border + frame, border + frame), rounded_mask(inner, 44))
    # round outer corners
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(img, (0, 0), rounded_mask(S, 100))
    return out


BAR_H = [0.35, 0.6, 0.85, 1.0, 0.7, 0.45, 0.65, 0.9, 1.0, 0.75, 0.5, 0.3,
         0.55, 0.8, 0.95, 0.6, 0.4, 0.7, 0.85, 0.5]


def draw_eq_border(img):
    """Dark border filled with vertical EQ bars top and bottom, gradient sides."""
    d = ImageDraw.Draw(img)
    n = len(BAR_H)
    bw = S / n
    for i, h in enumerate(BAR_H):
        c = lerp(GREEN, CYAN, i / (n - 1))
        x0 = i * bw + 3
        x1 = (i + 1) * bw - 3
        # top bars grow downward, bottom bars grow upward, max 78px
        d.rounded_rectangle([x0, 6, x1, 6 + 78 * h], radius=6, fill=c)
        d.rounded_rectangle([x0, S - 6 - 78 * h, x1, S - 6], radius=6, fill=c)
    # side gradient strips
    for y in range(S):
        c = lerp(GREEN, CYAN, y / S)
        d.line([(0, y), (14, y)], fill=c)
        d.line([(S - 14, y), (S, y)], fill=c)


def draw_gradient_border(img):
    d = ImageDraw.Draw(img)
    for y in range(S):
        d.line([(0, y), (S, y)], fill=lerp(GREEN, CYAN, y / S))


def draw_wave_border(img):
    d = ImageDraw.Draw(img)
    # waveform ring along the border midline (a rounded-rect path approximated)
    mid = 44
    pts = []
    per = 4 * (S - 2 * mid)
    steps = 900
    for i in range(steps + 1):
        t = i / steps * per
        e = S - 2 * mid
        if t < e:
            x, y, nx, ny = mid + t, mid, 0, -1
        elif t < 2 * e:
            x, y, nx, ny = S - mid, mid + (t - e), 1, 0
        elif t < 3 * e:
            x, y, nx, ny = S - mid - (t - 2 * e), S - mid, 0, 1
        else:
            x, y, nx, ny = mid, S - mid - (t - 3 * e), -1, 0
        amp = 26 * math.sin(t * 0.11) * math.sin(t * 0.023)
        pts.append((x + nx * amp, y + ny * amp))
    for i in range(len(pts) - 1):
        c = lerp(GREEN, CYAN, i / len(pts))
        d.line([pts[i], pts[i + 1]], fill=c, width=9)


def draw_headphones(img, box):
    """Vector headphones over the face. box = (x0,y0,x1,y1) face area."""
    d = ImageDraw.Draw(img, "RGBA")
    x0, y0, x1, y1 = box
    w = x1 - x0
    cx = (x0 + x1) / 2
    band_top = y0 + 0.02 * w
    band_w = int(0.085 * w)
    dark = (24, 28, 34, 255)
    # headband arc
    d.arc([x0 + 0.06 * w, band_top, x1 - 0.06 * w, y0 + 0.95 * w],
          start=195, end=345, fill=dark, width=band_w)
    # ear cups
    cup_w, cup_h = 0.16 * w, 0.30 * w
    cy = y0 + 0.52 * w
    for ex, side in ((x0 + 0.045 * w, 1), (x1 - 0.045 * w, -1)):
        d.rounded_rectangle([ex - cup_w / 2, cy - cup_h / 2, ex + cup_w / 2, cy + cup_h / 2],
                            radius=cup_w / 2.2, fill=dark)
        # accent ring on cup
        d.rounded_rectangle([ex - cup_w / 2 + 6, cy - cup_h / 2 + 6, ex + cup_w / 2 - 6, cy + cup_h / 2 - 6],
                            radius=cup_w / 2.8, outline=lerp(GREEN, CYAN, 0.5) + (255,), width=5)


def face_with_headphones():
    f = FACE.copy()
    draw_headphones(f, (0, 0, S, S))
    return f


def draw_curve_overlay(img):
    """Glowing 8-band EQ curve across the lower third, full-bleed face."""
    overlay = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(overlay)
    # dark gradient bottom for contrast
    for y in range(int(S * 0.55), S):
        a = int(200 * (y - S * 0.55) / (S * 0.45))
        d.line([(0, y), (S, y)], fill=(8, 14, 18, a))
    # curve
    bands = [(0.06, 0.78), (0.19, 0.70), (0.32, 0.82), (0.45, 0.66),
             (0.58, 0.74), (0.71, 0.62), (0.84, 0.72), (0.95, 0.80)]
    pts = []
    for i in range(S):
        x = i / S
        # smooth interp through band points
        y = 0.0
        tw = 0.0
        for bx, by in bands:
            wgt = math.exp(-((x - bx) ** 2) / 0.006)
            y += by * wgt
            tw += wgt
        pts.append((i, (y / tw) * S))
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    for i in range(len(pts) - 1):
        c = lerp(GREEN, CYAN, i / len(pts))
        gd.line([pts[i], pts[i + 1]], fill=c + (255,), width=14)
    glow = glow.filter(ImageFilter.GaussianBlur(10))
    overlay.alpha_composite(glow)
    for i in range(len(pts) - 1):
        c = lerp(GREEN, CYAN, i / len(pts))
        d.line([pts[i], pts[i + 1]], fill=c + (255,), width=7)
    # band dots
    for bx, by in bands:
        x, y = bx * S, 0
        # sample the curve at bx
        y = pts[min(S - 1, int(bx * S))][1]
        d.ellipse([x - 11, y - 11, x + 11, y + 11], fill=(255, 255, 255, 255))
        d.ellipse([x - 7, y - 7, x + 7, y + 7], fill=lerp(GREEN, CYAN, bx))
    img.alpha_composite(overlay)


def fullbleed(face_img, overlay_fn=None):
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.paste(face_img, (0, 0))
    if overlay_fn:
        overlay_fn(img)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(img, (0, 0), rounded_mask(S, 100))
    return out


variants = {
    "1_eqbars_headphones": framed(face_with_headphones(), draw_eq_border),
    "2_eqbars_only": framed(FACE, draw_eq_border),
    "3_headphones_gradient": framed(face_with_headphones(), draw_gradient_border),
    "4_waveform_border": framed(FACE, draw_wave_border),
    "5_curve_overlay": fullbleed(FACE, draw_curve_overlay),
    "6_curve_headphones": fullbleed(face_with_headphones(), draw_curve_overlay),
}

for name, img in variants.items():
    img.save(f"{OUT}\\{name}.png")
    img.resize((32, 32), Image.LANCZOS).save(f"{OUT}\\{name}_32.png")
    print(name)
