"""Round 2 TrontEQ icon variants: no headphones, refined bars/waveform/curve."""
import math
from PIL import Image, ImageDraw, ImageFilter

S = 512
FACE = Image.open(r"C:\trontstack\tronteq\gui\assets\icon.png").convert("RGB").resize((S, S), Image.LANCZOS)
OUT = r"C:\trontstack\tronteq\icon_variants"

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
    img = Image.new("RGB", (S, S), BG)
    border_draw_fn(img)
    fs = S - 2 * border
    white = Image.new("RGB", (fs, fs), (255, 255, 255))
    img.paste(white, (border, border), rounded_mask(fs, 56))
    inner = fs - 2 * frame
    f = face_img.resize((inner, inner), Image.LANCZOS)
    img.paste(f, (border + frame, border + frame), rounded_mask(inner, 44))
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(img, (0, 0), rounded_mask(S, 100))
    return out


def clip_round(img):
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(img.convert("RGB"), (0, 0))
    m = rounded_mask(S, 100)
    out.putalpha(m)
    return out


# chunky symmetric bar heights (mirror for symmetry)
CHUNK = [0.35, 0.6, 0.9, 0.7, 1.0, 0.55, 0.8, 0.45]
CHUNK = CHUNK + CHUNK[::-1]


def glow_layer(draw_fn, blur=8):
    """Draw into a layer, return (sharp, blurred glow)."""
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    draw_fn(ImageDraw.Draw(layer))
    return layer, layer.filter(ImageFilter.GaussianBlur(blur))


def draw_chunky_bars_border(img):
    """Fewer, fatter bars top+bottom with glow; gradient side rails."""
    base = ImageDraw.Draw(img)
    for y in range(S):
        c = lerp(GREEN, CYAN, y / S)
        base.line([(0, y), (16, y)], fill=c)
        base.line([(S - 16, y), (S, y)], fill=c)

    def bars(d):
        n = len(CHUNK)
        bw = S / n
        for i, h in enumerate(CHUNK):
            c = lerp(GREEN, CYAN, i / (n - 1)) + (255,)
            x0, x1 = i * bw + 5, (i + 1) * bw - 5
            d.rounded_rectangle([x0, 10, x1, 10 + 68 * h], radius=8, fill=c)
            d.rounded_rectangle([x0, S - 10 - 68 * h, x1, S - 10], radius=8, fill=c)
    sharp, glow = glow_layer(bars)
    rgba = img.convert("RGBA")
    rgba.alpha_composite(glow)
    rgba.alpha_composite(sharp)
    img.paste(rgba.convert("RGB"), (0, 0))


def draw_sine_border(img):
    """Smooth flowing sine waveform ring, thick with glow."""
    mid = 44

    def path_pts():
        pts = []
        e = S - 2 * mid
        per = 4 * e
        steps = 1200
        for i in range(steps + 1):
            t = i / steps * per
            if t < e:
                x, y, nx, ny = mid + t, mid, 0, -1
            elif t < 2 * e:
                x, y, nx, ny = S - mid, mid + (t - e), 1, 0
            elif t < 3 * e:
                x, y, nx, ny = S - mid - (t - 2 * e), S - mid, 0, 1
            else:
                x, y, nx, ny = mid, S - mid - (t - 3 * e), -1, 0
            # smooth single-frequency sine, whole number of periods so it closes
            amp = 22 * math.sin(2 * math.pi * 12 * t / per)
            pts.append((x + nx * amp, y + ny * amp))
        return pts

    pts = path_pts()

    def wave(d):
        for i in range(len(pts) - 1):
            c = lerp(GREEN, CYAN, 0.5 - 0.5 * math.cos(2 * math.pi * i / len(pts)))
            d.line([pts[i], pts[i + 1]], fill=c + (255,), width=13)
    sharp, glow = glow_layer(wave, blur=10)
    rgba = img.convert("RGBA")
    rgba.alpha_composite(glow)
    rgba.alpha_composite(sharp)
    img.paste(rgba.convert("RGB"), (0, 0))


BANDS = [(0.05, 0.80), (0.18, 0.72), (0.31, 0.84), (0.44, 0.68),
         (0.57, 0.76), (0.70, 0.64), (0.83, 0.74), (0.96, 0.82)]


def curve_pts(y_off=0.0, squash=1.0):
    pts = []
    for i in range(S):
        x = i / S
        y = tw = 0.0
        for bx, by in BANDS:
            w = math.exp(-((x - bx) ** 2) / 0.006)
            y += by * w
            tw += w
        pts.append((i, ((y / tw) - 0.75) * squash * S + (0.75 + y_off) * S))
    return pts


def curve_overlay(img, dots=True, y_off=0.05):
    """Glowing curve low on the face with a real dark backing gradient."""
    rgba = img.convert("RGBA")
    # dark gradient: draw on separate layer then composite
    grad = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    start = int(S * 0.50)
    for y in range(start, S):
        a = int(235 * ((y - start) / (S - start)) ** 1.2)
        gd.line([(0, y), (S, y)], fill=(6, 12, 16, a))
    rgba.alpha_composite(grad)

    pts = curve_pts(y_off=y_off)

    def line(d):
        for i in range(len(pts) - 1):
            c = lerp(GREEN, CYAN, i / len(pts))
            d.line([pts[i], pts[i + 1]], fill=c + (255,), width=9)
    sharp, glow = glow_layer(line, blur=12)
    rgba.alpha_composite(glow)
    rgba.alpha_composite(sharp)
    if dots:
        d = ImageDraw.Draw(rgba)
        for bx, _ in BANDS:
            x = min(S - 1, int(bx * S))
            y = pts[x][1]
            d.ellipse([x - 12, y - 12, x + 12, y + 12], fill=(255, 255, 255, 255))
            d.ellipse([x - 8, y - 8, x + 8, y + 8], fill=lerp(GREEN, CYAN, bx))
    return rgba


def bars_over_face():
    """Full-bleed face with big translucent spectrum bars rising from the bottom."""
    rgba = FACE.convert("RGBA")
    grad = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    start = int(S * 0.45)
    for y in range(start, S):
        a = int(215 * ((y - start) / (S - start)) ** 1.3)
        gd.line([(0, y), (S, y)], fill=(6, 12, 16, a))
    rgba.alpha_composite(grad)

    heights = [0.30, 0.55, 0.85, 0.65, 1.0, 0.75, 0.5, 0.9, 0.6, 0.35]

    def bars(d):
        n = len(heights)
        bw = S / n
        maxh = 0.42 * S
        for i, h in enumerate(heights):
            c = lerp(GREEN, CYAN, i / (n - 1)) + (235,)
            x0, x1 = i * bw + 7, (i + 1) * bw - 7
            d.rounded_rectangle([x0, S - 18 - maxh * h, x1, S - 18], radius=10, fill=c)
    sharp, glow = glow_layer(bars, blur=10)
    rgba.alpha_composite(glow)
    rgba.alpha_composite(sharp)
    return rgba


def framed_curve_bottom():
    """Family frame, gradient border, glowing curve running across the bottom border band."""
    img = Image.new("RGB", (S, S), BG)
    d = ImageDraw.Draw(img)
    for y in range(S):
        d.line([(0, y), (S, y)], fill=lerp((10, 40, 34), (10, 34, 48), y / S))
    fs = S - 2 * 88
    white = Image.new("RGB", (fs, fs), (255, 255, 255))
    img.paste(white, (88, 74), rounded_mask(fs, 56))
    inner = fs - 2 * 14
    f = FACE.resize((inner, inner), Image.LANCZOS)
    img.paste(f, (102, 88), rounded_mask(inner, 44))
    rgba = img.convert("RGBA")

    def line(dd):
        pts = []
        for i in range(S):
            x = i / S
            y = S - 60 + 34 * math.sin(x * math.pi * 3.1) * math.sin(x * math.pi * 1.3 + 0.8)
            pts.append((i, y))
        for i in range(len(pts) - 1):
            c = lerp(GREEN, CYAN, i / len(pts))
            dd.line([pts[i], pts[i + 1]], fill=c + (255,), width=10)
    sharp, glow = glow_layer(line, blur=10)
    rgba.alpha_composite(glow)
    rgba.alpha_composite(sharp)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(rgba.convert("RGB"), (0, 0))
    out.putalpha(rounded_mask(S, 100))
    return out


variants = {
    "7_chunky_bars": framed(FACE, draw_chunky_bars_border),
    "8_sine_border": framed(FACE, draw_sine_border),
    "9_curve_v2": clip_round(curve_overlay(FACE.copy(), dots=True)),
    "10_curve_clean": clip_round(curve_overlay(FACE.copy(), dots=False, y_off=0.08)),
    "11_bars_over_face": clip_round(bars_over_face()),
    "12_framed_curve": framed_curve_bottom(),
}

for name, img in variants.items():
    img.save(f"{OUT}\\{name}.png")
    print(name)
