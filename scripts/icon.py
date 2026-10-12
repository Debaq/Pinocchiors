#!/usr/bin/env python3
"""Genera el ícono de la app: la cabeza de la marioneta del logo (nariz larga y
gorro cónico con pompón), low-poly, sobre un cuadrado oscuro con las esquinas
naranjas del visor.

Hay dos dibujos: el facetado para 64 px y más, y uno plano sin trazos para
16–48 px, donde las facetas se vuelven ruido. Escribe los SVG en
apps/desktop/icons/ y, con rsvg-convert y ImageMagick, todos los PNG, el .ico y
el .icns que usa Tauri.

    python3 scripts/icon.py
"""
import random
import struct
import subprocess
import tempfile
from pathlib import Path

import logo
from logo import CREAM, HAT, INK, ORANGE, SHIRT, WOOD, blob, circle, facets, pts

ICONS = Path(__file__).resolve().parent.parent / "apps/desktop/icons"

S = 512
BG_TOP = "#2a241e"
BG_BOTTOM = "#0d0b0a"
INSET = 16  # margen del cuadrado redondeado
RADIUS = 104

# Cabeza del logo (coordenadas de logo.py) llevada al centro del ícono
HEAD_CENTER = (264, 160)  # centro de cabeza + nariz + gorro en logo.py
ICON_CENTER = (256, 262)


def background(out):
    out.append(
        f'<defs><linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">'
        f'<stop offset="0" stop-color="{BG_TOP}"/><stop offset="1" stop-color="{BG_BOTTOM}"/>'
        f"</linearGradient></defs>"
    )
    w = S - 2 * INSET
    rect = f'<rect x="{INSET}" y="{INSET}" width="{w}" height="{w}" rx="{RADIUS}"/>'
    out.append(f'<clipPath id="tile">{rect}</clipPath>')
    out.append(rect.replace("/>", ' fill="url(#bg)"/>'))


def corners(out, width=12, inset=62, length=54):
    a, b = inset, S - inset
    for x, y, sx, sy in [(a, a, 1, 1), (b, a, -1, 1), (a, b, 1, -1), (b, b, -1, -1)]:
        out.append(
            f'<polyline points="{x},{y + sy * length} {x},{y} {x + sx * length},{y}" fill="none" stroke="{ORANGE}"'
            f' stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"/>'
        )


def to_icon(p, s, center=HEAD_CENTER):
    """Punto de logo.py a coordenadas del ícono"""
    return (ICON_CENTER[0] + (p[0] - center[0]) * s, ICON_CENTER[1] + (p[1] - center[1]) * s)


def head_transform(s, center=HEAD_CENTER):
    tx, ty = to_icon((0, 0), s, center)
    return f"translate({tx:.1f} {ty:.1f}) scale({s})"


def string(out, s):
    """Hilo de la marioneta desde arriba hasta el pompón"""
    x, y = to_icon((332, 92), s)
    out.append(f'<line clip-path="url(#tile)" x1="{x:.1f}" y1="{INSET}" x2="{x:.1f}" y2="{y:.1f}" stroke="#8a7a66" stroke-width="3.5" stroke-linecap="round"/>')


def head_faceted():
    """La misma cabeza de logo.puppet, sin el collar"""
    blob((302, 194), 40, 10, WOOD, rings=2, squash=1.0, rot=0.2)
    facets([(266, 186), (266, 200), (176, 196)], "#d9a066", pull=0.0)  # nariz
    facets([(278, 208), (296, 214), (284, 218)], "#9a6236", pull=0.0)  # boca
    circle((286, 182), 6.5, CREAM, INK)
    circle((284, 182), 3.2, "#16110C")
    facets([(268, 162), (332, 154), (322, 98)], HAT, pull=0.1)  # copa
    facets([(254, 166), (348, 154), (352, 164), (258, 176)], HAT)  # ala
    facets([(322, 98), (332, 89), (343, 97), (333, 106)], SHIRT)  # pompón


def poly(out, ps, fill):
    out.append(f'<polygon points="{pts(ps)}" fill="{fill}" stroke-linejoin="round"/>')


FLAT_CENTER = (284, 160)  # la nariz corta corre el centro a la derecha


def head_flat(out):
    """Silueta plana, cabeza más grande y nariz más corta: se lee a 16 px"""
    import math

    head = [(302 + math.cos(0.2 + 2 * math.pi * i / 10) * 42, 194 + math.sin(0.2 + 2 * math.pi * i / 10) * 42) for i in range(10)]
    poly(out, head, WOOD)
    poly(out, [(268, 180), (268, 206), (212, 196)], "#e0aa70")  # nariz
    poly(out, [(266, 160), (334, 151), (320, 92)], HAT)  # copa
    poly(out, [(250, 164), (350, 151), (354, 166), (254, 180)], "#b87622")  # ala
    poly(out, [(318, 96), (331, 84), (346, 95), (333, 108)], SHIRT)  # pompón
    out.append(f'<circle cx="286" cy="186" r="8" fill="{CREAM}"/>')
    out.append('<circle cx="284" cy="186" r="4.2" fill="#16110C"/>')


def svg(small=False):
    out = []
    background(out)
    if small:
        # Sin esquinas: a 16 px serían ruido; la cabeza ocupa el cuadrado
        scale, center = 2.45, FLAT_CENTER
        body = []
        head_flat(body)
    else:
        corners(out)
        scale, center = 1.9, HEAD_CENTER
        string(out, scale)
        random.seed(7)
        body = logo.capture(head_faceted)
    out.append(f'<g transform="{head_transform(scale, center)}">')
    out.extend(body)
    out.append("</g>")
    return "\n".join([f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {S} {S}" width="{S}" height="{S}">', *out, "</svg>"]) + "\n"


# Tipos del .icns con PNG adentro: (tipo, lado en píxeles)
ICNS = [
    ("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128), ("ic08", 256), ("ic09", 512), ("ic10", 1024),
    ("ic11", 32), ("ic12", 64), ("ic13", 256), ("ic14", 512),
]


def write_icns(dst, tmp):
    """Contenedor 'icns': cada entrada es tipo + largo + PNG"""
    body = b""
    for kind, size in ICNS:
        data = (tmp / f"icns-{size}.png").read_bytes()
        body += kind.encode() + struct.pack(">I", len(data) + 8) + data
    dst.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def run(*args):
    subprocess.run(args, check=True)


def png(src, size, dst):
    run("rsvg-convert", "-w", str(size), "-h", str(size), str(src), "-o", str(dst))


if __name__ == "__main__":
    big = ICONS / "icon.svg"
    small = ICONS / "icon-small.svg"
    big.write_text(svg())
    small.write_text(svg(small=True))

    def source(size):
        return small if size <= 48 else big

    # Tauri (bundle.icon) y el acceso directo de Linux
    for name, size in [("32x32", 32), ("64x64", 64), ("128x128", 128), ("128x128@2x", 256), ("icon", 512)]:
        png(source(size), size, ICONS / f"{name}.png")
    # Windows (tienda / tiles)
    for size in [30, 44, 71, 89, 107, 142, 150, 284, 310]:
        png(source(size), size, ICONS / f"Square{size}x{size}Logo.png")
    png(source(50), 50, ICONS / "StoreLogo.png")

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        # .ico: cada tamaño desde su propio dibujo
        sizes = [16, 24, 32, 48, 64, 256]
        for size in sizes:
            png(source(size), size, tmp / f"{size}.png")
        run("magick", *[str(tmp / f"{s}.png") for s in sizes], str(ICONS / "icon.ico"))
        # .icns: 16–1024
        for size in {size for _, size in ICNS}:
            png(source(size), size, tmp / f"icns-{size}.png")
        write_icns(ICONS / "icon.icns", tmp)

    for f in sorted(ICONS.iterdir()):
        print(f.relative_to(ICONS.parent.parent.parent))
