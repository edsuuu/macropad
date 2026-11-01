"""Gera assets/icon.ico (janela e .exe) e assets/icon.png: a placa vista de cima, com o knob e as 3 teclas
de LED azul, num quadrado escuro arredondado. Desenha em 1024 px e reduz, para as bordas ficarem lisas.
Uso: python tools/icon.py
"""
import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ASSETS = Path(__file__).resolve().parent.parent / "assets"
S = 1024
LED = (90, 169, 255)


def draw() -> Image.Image:
    img = Image.new("RGBA", (S, S))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((24, 24, S - 24, S - 24), 230, fill=(24, 24, 27))
    # Corpo de acrílico fosco.
    d.rounded_rectangle((112, 330, 912, 694), 70, fill=(176, 179, 186))
    d.rounded_rectangle((112, 330, 912, 694), 70, outline=(210, 212, 218), width=10)
    # Brilho dos LEDs embaixo das teclas, borrado numa camada à parte.
    glow = Image.new("RGBA", (S, S))
    for x in (500, 650, 800):
        ImageDraw.Draw(glow).ellipse((x - 70, 560, x + 70, 680), fill=LED + (230,))
    img.alpha_composite(glow.filter(ImageFilter.GaussianBlur(34)))
    d = ImageDraw.Draw(img)
    for x in (500, 650, 800):
        d.rounded_rectangle((x - 62, 400, x + 62, 560), 26, fill=(14, 14, 16))
        d.rounded_rectangle((x - 48, 410, x + 48, 520), 20, fill=(44, 44, 50))
        d.rounded_rectangle((x - 26, 596, x + 26, 620), 8, fill=(220, 238, 255))
    # Knob escuro com o traço quase reto, um pouco à esquerda, como na placa.
    cx, cy, r = 280, 512, 140
    d.ellipse((cx - r - 14, cy - r - 14, cx + r + 14, cy + r + 14), fill=(14, 14, 16))
    d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(58, 59, 66))
    d.ellipse((cx - r + 22, cy - r + 22, cx + r - 22, cy + r - 22), fill=(36, 37, 42))
    a = math.radians(-7)
    p = lambda k: (cx + math.sin(a) * k, cy - math.cos(a) * k)
    d.line((p(56), p(100)), fill=(240, 240, 244), width=26)
    return img


def main() -> None:
    img = draw()
    ASSETS.mkdir(exist_ok=True)
    img.resize((512, 512), Image.LANCZOS).save(ASSETS / "icon.png")
    sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
    img.resize((256, 256), Image.LANCZOS).save(ASSETS / "icon.ico", sizes=[(s, s) for s in sizes])


if __name__ == "__main__":
    main()
