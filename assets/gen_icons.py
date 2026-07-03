import math
from PIL import Image, ImageDraw

S = 1024  # supersample canvas
OUT = "."

def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

def rounded_rect_gradient(size, radius, top_color, bottom_color):
    """Vertical gradient background clipped to a rounded rect, with alpha mask."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    grad = Image.new("RGB", (1, size))
    for y in range(size):
        t = y / (size - 1)
        grad.putpixel((0, y), lerp(top_color, bottom_color, t))
    grad = grad.resize((size, size))
    mask = Image.new("L", (size, size), 0)
    mdraw = ImageDraw.Draw(mask)
    mdraw.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=255)
    img.paste(grad, (0, 0), mask)
    return img, mask

def draw_monitor_glyph(draw, cx, cy, screen_w, screen_h, radius, screen_fill,
                        border_color, border_w, arrow_color, arrow_scale=1.0,
                        with_stand=True, stand_fill=None):
    left = cx - screen_w / 2
    right = cx + screen_w / 2
    top = cy - screen_h / 2
    bottom = cy + screen_h / 2

    if with_stand:
        stand_fill = stand_fill or screen_fill
        neck_w = screen_w * 0.16
        neck_h = screen_h * 0.16
        base_w = screen_w * 0.46
        base_h = screen_h * 0.09
        neck_top = bottom - border_w * 0.3
        neck_bottom = neck_top + neck_h
        draw.rectangle(
            [cx - neck_w / 2, neck_top, cx + neck_w / 2, neck_bottom],
            fill=stand_fill,
        )
        base_top = neck_bottom
        base_bottom = base_top + base_h
        draw.rounded_rectangle(
            [cx - base_w / 2, base_top, cx + base_w / 2, base_bottom],
            radius=base_h * 0.4,
            fill=stand_fill,
        )

    # screen (drawn after stand so it sits on top / connects cleanly)
    if border_w > 0:
        draw.rounded_rectangle(
            [left - border_w, top - border_w, right + border_w, bottom + border_w],
            radius=radius + border_w,
            fill=border_color,
        )
    draw.rounded_rectangle([left, top, right, bottom], radius=radius, fill=screen_fill)

    # double-headed horizontal switch arrow, centered in screen
    aw = screen_w * 0.62 * arrow_scale
    ah = screen_h * 0.30 * arrow_scale
    shaft_h = ah * 0.34
    head_w = aw * 0.30

    x0 = cx - aw / 2
    x1 = cx + aw / 2
    shaft_x0 = x0 + head_w
    shaft_x1 = x1 - head_w
    y_mid = cy
    y0 = y_mid - ah / 2
    y1 = y_mid + ah / 2

    # shaft
    draw.rectangle([shaft_x0, y_mid - shaft_h / 2, shaft_x1, y_mid + shaft_h / 2], fill=arrow_color)
    # left arrowhead (pointing left)
    draw.polygon([(x0, y_mid), (shaft_x0 + 2, y0), (shaft_x0 + 2, y1)], fill=arrow_color)
    # right arrowhead (pointing right)
    draw.polygon([(x1, y_mid), (shaft_x1 - 2, y0), (shaft_x1 - 2, y1)], fill=arrow_color)


BADGE_TOP = (74, 144, 205)     # lighter blue
BADGE_BOTTOM = (21, 63, 110)   # deep navy blue
SCREEN_FILL = (247, 250, 253)
SCREEN_BORDER = (13, 42, 74)
ARROW_COLOR = (21, 63, 110)
STAND_FILL = (219, 231, 242)

def build_full_icon():
    img, mask = rounded_rect_gradient(S, radius=int(S * 0.22), top_color=BADGE_TOP, bottom_color=BADGE_BOTTOM)
    draw = ImageDraw.Draw(img)
    cx, cy = S / 2, S * 0.46
    draw_monitor_glyph(
        draw, cx, cy,
        screen_w=S * 0.60, screen_h=S * 0.40, radius=int(S * 0.045),
        screen_fill=SCREEN_FILL, border_color=SCREEN_BORDER, border_w=S * 0.016,
        arrow_color=ARROW_COLOR, with_stand=True, stand_fill=STAND_FILL,
    )
    # subtle top highlight for depth
    highlight = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    hd = ImageDraw.Draw(highlight)
    hd.rounded_rectangle([S*0.06, S*0.05, S*0.94, S*0.34], radius=int(S*0.18), fill=(255, 255, 255, 26))
    hmask = Image.new("L", (S, S), 0)
    hmdraw = ImageDraw.Draw(hmask)
    hmdraw.rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.22), fill=255)
    img.paste(Image.alpha_composite(img, highlight), (0, 0), hmask)
    return img

def build_tray_icon():
    # Simplified: no stand, bigger/bolder screen+arrow for legibility at small sizes.
    img, mask = rounded_rect_gradient(S, radius=int(S * 0.24), top_color=BADGE_TOP, bottom_color=BADGE_BOTTOM)
    draw = ImageDraw.Draw(img)
    cx, cy = S / 2, S / 2
    draw_monitor_glyph(
        draw, cx, cy,
        screen_w=S * 0.72, screen_h=S * 0.50, radius=int(S * 0.06),
        screen_fill=SCREEN_FILL, border_color=SCREEN_BORDER, border_w=S * 0.022,
        arrow_color=ARROW_COLOR, arrow_scale=1.05, with_stand=False,
    )
    return img

def save_png_sizes(img, sizes, prefix):
    for s in sizes:
        im = img.resize((s, s), Image.LANCZOS)
        im.save(f"{prefix}_{s}.png")

def save_ico(img, sizes, path):
    base = img.resize((max(sizes), max(sizes)), Image.LANCZOS)
    base.save(path, format="ICO", sizes=[(s, s) for s in sizes])

if __name__ == "__main__":
    full = build_full_icon()
    tray = build_tray_icon()

    full.save("logo_full_1024.png")
    tray.save("tray_full_1024.png")

    save_png_sizes(full, [16, 32, 48, 64, 128, 256, 512], "logo")
    save_png_sizes(tray, [16, 20, 24, 32, 40, 48, 64, 128, 256], "tray")

    save_ico(full, [16, 32, 48, 256], "app_icon.ico")
    save_ico(tray, [16, 20, 24, 32, 40, 48, 64, 256], "tray_icon.ico")

    print("done")
