#!/usr/bin/env python3
"""Рендерит JSON-кадр из теста dump_frame_json в PNG (подобие скриншота терминала).

Использование: python3 docs/render_frame.py /tmp/ytmt_frame.json docs/screenshot.png
"""
import json
import sys

from PIL import Image, ImageDraw, ImageFont

CW, CH = 10, 20          # размер клетки в пикселях
PAD = 16                 # поля вокруг «окна» терминала
BG = (18, 18, 22)
FG = (232, 232, 238)
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"

HALF_TOP = "\u2580"      # ▀
HALF_BOT = "\u2584"      # ▄
FULL = "\u2588"          # █


def col(c, default):
    if c is None:
        return default
    if isinstance(c, list):
        return tuple(c)
    return default


def main(src, dst):
    frame = json.load(open(src))
    w, h = frame["w"], frame["h"]
    img = Image.new("RGB", (w * CW + PAD * 2, h * CH + PAD * 2), (58, 58, 70))
    d = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONT_PATH, 17)
    font_b = ImageFont.truetype(FONT_BOLD, 17)

    for idx, cell in enumerate(frame["cells"]):
        x, y = idx % w, idx // w
        px, py = PAD + x * CW, PAD + y * CH
        fg = col(cell["f"], FG)
        bg = col(cell["b"], BG)
        s = cell["s"]

        if s == HALF_TOP:                       # верхняя половина цветом fg
            d.rectangle([px, py, px + CW - 1, py + CH // 2 - 1], fill=fg)
            d.rectangle([px, py + CH // 2, px + CW - 1, py + CH - 1], fill=bg)
        elif s == HALF_BOT:                     # нижняя половина цветом fg
            d.rectangle([px, py, px + CW - 1, py + CH // 2 - 1], fill=bg)
            d.rectangle([px, py + CH // 2, px + CW - 1, py + CH - 1], fill=fg)
        elif s == FULL:                         # закрашено целиком
            d.rectangle([px, py, px + CW - 1, py + CH - 1], fill=fg)
        else:
            d.rectangle([px, py, px + CW - 1, py + CH - 1], fill=bg)
            if s.strip():
                f = font_b if cell.get("bold") else font
                d.text((px + CW / 2, py + CH / 2), s, font=f, fill=fg, anchor="mm")

    img.save(dst)
    print("saved", dst, img.size)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
