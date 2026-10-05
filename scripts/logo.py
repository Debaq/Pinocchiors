#!/usr/bin/env python3
"""Genera el logo de Pinocchio: una marioneta articulada de madera, low-poly,
colgada de su cruceta. Mismo estilo que la portada del manual de Monteverde:
facetas con trazo oscuro, fondo de puntos y esquinas de visor naranjas.

    python3 scripts/logo.py > apps/web/src/assets/logo.svg
"""
import math
import random

random.seed(7)

W = 600
ORANGE = "#E9A93C"
CREAM = "#F6F1E8"
INK = "#2a1d0e"
LIGHT = (-0.55, -0.83)  # luz desde arriba a la izquierda

out: list[str] = []


def hex2rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


def shade(color, f):
    r, g, b = hex2rgb(color)
    c = lambda v: max(0, min(255, round(v * f)))
    return f"#{c(r):02x}{c(g):02x}{c(b):02x}"


def pts(ps):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in ps)


def facets(poly, color, stroke=INK, pull=0.18, rings=1):
    """Polígono convexo en abanico desde un centro corrido hacia la luz; cada
    faceta se aclara u oscurece según mire a la luz"""
    cx = sum(p[0] for p in poly) / len(poly)
    cy = sum(p[1] for p in poly) / len(poly)
    size = max(math.dist((cx, cy), p) for p in poly)
    c = (cx + LIGHT[0] * size * pull, cy + LIGHT[1] * size * pull)
    out.append(f'<polygon points="{pts(poly)}" fill="{shade(color, 0.8)}" stroke="{stroke}" stroke-width="1.2" stroke-linejoin="round"/>')
    layers = [poly]
    if rings > 1:
        # Anillo interior girado: más facetas en piezas grandes
        inner = []
        n = len(poly)
        for i in range(n):
            a, b = poly[i], poly[(i + 1) % n]
            m = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
            inner.append((c[0] + (m[0] - c[0]) * 0.55, c[1] + (m[1] - c[1]) * 0.55))
        for i in range(n):
            a, b = poly[i], poly[(i + 1) % n]
            tri(a, b, inner[i], color, c, stroke)
            tri(a, inner[i], inner[i - 1], color, c, stroke)
        layers = [inner]
    ring = layers[0]
    for i in range(len(ring)):
        tri(ring[i], ring[(i + 1) % len(ring)], c, color, c, stroke)


def tri(a, b, d, color, center, stroke):
    mx, my = (a[0] + b[0] + d[0]) / 3 - center[0], (a[1] + b[1] + d[1]) / 3 - center[1]
    l = math.hypot(mx, my) or 1
    lit = (mx * LIGHT[0] + my * LIGHT[1]) / l
    f = 1 + 0.16 * lit + random.uniform(-0.06, 0.06)
    out.append(
        f'<polygon points="{pts([a, b, d])}" fill="{shade(color, f)}" stroke="{stroke}" stroke-width=".7" stroke-linejoin="round"/>'
    )


def limb(a, b, wa, wb, color, bulge=1.12):
    """Segmento como hexágono facetado de `a` a `b`"""
    dx, dy = b[0] - a[0], b[1] - a[1]
    l = math.hypot(dx, dy)
    nx, ny = -dy / l, dx / l
    wm = (wa + wb) / 2 * bulge
    m = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
    poly = [
        (a[0] + nx * wa, a[1] + ny * wa),
        (m[0] + nx * wm, m[1] + ny * wm),
        (b[0] + nx * wb, b[1] + ny * wb),
        (b[0] + dx / l * wb * 0.6, b[1] + dy / l * wb * 0.6),
        (b[0] - nx * wb, b[1] - ny * wb),
        (m[0] - nx * wm, m[1] - ny * wm),
        (a[0] - nx * wa, a[1] - ny * wa),
        (a[0] - dx / l * wa * 0.6, a[1] - dy / l * wa * 0.6),
    ]
    facets(poly, color, pull=0.12)


def blob(c, r, n, color, rings=1, squash=1.0, rot=0.0):
    poly = []
    for i in range(n):
        t = rot + 2 * math.pi * i / n
        rr = r * random.uniform(0.94, 1.04)
        poly.append((c[0] + math.cos(t) * rr, c[1] + math.sin(t) * rr * squash))
    facets(poly, color, rings=rings)


def joint(p, r=5.2):
    out.append(f'<circle cx="{p[0]:.1f}" cy="{p[1]:.1f}" r="{r}" fill="{CREAM}" stroke="{ORANGE}" stroke-width="2.2"/>')
    out.append(f'<circle cx="{p[0]:.1f}" cy="{p[1]:.1f}" r="1.6" fill="{INK}"/>')


def string(a, b):
    out.append(f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="#3b3128" stroke-width="1.1" opacity=".75"/>')
    out.append(f'<circle cx="{b[0]:.1f}" cy="{b[1]:.1f}" r="2.4" fill="{ORANGE}"/>')


# ─── Fondo: campo de puntos isométrico que se apaga hacia los bordes ───
for row in range(-2, 30):
    for col in range(-2, 30):
        x = col * 22.5 + (row % 2) * 11.25
        y = row * 13 + 260
        d = math.hypot((x - 300) / 1.6, (y - 470) / 0.9)
        o = 0.22 - d / 900
        if o > 0.04 and 0 < x < W and 0 < y < W:
            out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="1.3" fill="{ORANGE}" opacity="{o:.2f}"/>')

# ─── Piso: losa isométrica con grilla, como el suelo del visor ───
floor_c, fw, fh = (300, 492), 150, 70
floor = [(floor_c[0], floor_c[1] - fh), (floor_c[0] + fw, floor_c[1]), (floor_c[0], floor_c[1] + fh), (floor_c[0] - fw, floor_c[1])]
out.append(f'<polygon points="{pts(floor)}" fill="{CREAM}"/>')
for i in range(1, 6):
    t = i / 6
    a = (floor[0][0] + (floor[1][0] - floor[0][0]) * t, floor[0][1] + (floor[1][1] - floor[0][1]) * t)
    b = (floor[3][0] + (floor[2][0] - floor[3][0]) * t, floor[3][1] + (floor[2][1] - floor[3][1]) * t)
    out.append(f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="#d9cfbf" stroke-width="1"/>')
    a = (floor[0][0] + (floor[3][0] - floor[0][0]) * t, floor[0][1] + (floor[3][1] - floor[0][1]) * t)
    b = (floor[1][0] + (floor[2][0] - floor[1][0]) * t, floor[1][1] + (floor[2][1] - floor[1][1]) * t)
    out.append(f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="#d9cfbf" stroke-width="1"/>')
out.append('<ellipse cx="305" cy="496" rx="62" ry="15" fill="#000" opacity=".13"/>')

# ─── Esquinas de visor ───
for (x, y, sx, sy) in [(118, 92, 1, 1), (482, 92, -1, 1), (118, 548, 1, -1), (482, 548, -1, -1)]:
    out.append(
        f'<polyline points="{x},{y + sy * 30} {x},{y} {x + sx * 30},{y}" fill="none" stroke="{ORANGE}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>'
    )

# ─── Esqueleto de la marioneta ───
WOOD = "#c98f55"
SHIRT = "#b5532e"
PANTS = "#4f6b88"
SHOE = "#3a2a1a"
HAT = "#d08a2a"

head = (302, 194)
neck = (300, 226)
sh_l, sh_r = (268, 240), (332, 240)
el_l, el_r = (226, 276), (380, 222)
ha_l, ha_r = (196, 318), (396, 160)
hip_l, hip_r = (284, 350), (318, 350)
kn_l, kn_r = (280, 418), (362, 404)
an_l, an_r = (276, 478), (350, 462)

# Pierna derecha (levantada, atrás)
limb(hip_r, kn_r, 15, 12, PANTS)
limb(kn_r, an_r, 9, 7, WOOD)
facets([(an_r[0] - 8, an_r[1] - 6), (an_r[0] + 30, an_r[1] - 4), (an_r[0] + 34, an_r[1] + 8), (an_r[0] - 6, an_r[1] + 10)], SHOE)
# Pierna izquierda (de apoyo)
limb(hip_l, kn_l, 15, 12, PANTS)
limb(kn_l, an_l, 9, 7, WOOD)
facets([(an_l[0] + 8, an_l[1] - 6), (an_l[0] - 30, an_l[1] - 2), (an_l[0] - 33, an_l[1] + 11), (an_l[0] + 7, an_l[1] + 11)], SHOE)

# Brazo derecho (alzado por su hilo, atrás del torso)
limb(sh_r, el_r, 12, 10, SHIRT)
limb(el_r, ha_r, 8, 6, WOOD)
blob(ha_r, 10, 7, WOOD, rot=0.3)

# Torso y pantalón corto
facets([(264, 236), (300, 228), (336, 236), (330, 300), (322, 340), (278, 340), (270, 300)], SHIRT, rings=2)
facets([(276, 334), (324, 334), (330, 362), (302, 368), (300, 356), (298, 368), (270, 362)], PANTS)
facets([(306, 252), (312, 264), (306, 276), (300, 264)], "#e0b25a")  # botón

# Brazo izquierdo (adelante)
limb(sh_l, el_l, 12, 10, SHIRT)
limb(el_l, ha_l, 8, 6, WOOD)
blob(ha_l, 10, 7, WOOD, rot=0.9)

# Cabeza, nariz larga hacia la izquierda, ojo y sombrero cónico
blob(head, 40, 10, WOOD, rings=2, squash=1.0, rot=0.2)
facets([(276, 226), (300, 238), (324, 226), (316, 218), (300, 226), (284, 218)], CREAM, stroke="#8a7a66")  # collar
facets([(266, 186), (266, 200), (176, 196)], "#d9a066", pull=0.0)  # nariz
facets([(278, 208), (296, 214), (284, 218)], "#9a6236", pull=0.0)  # boca
out.append(f'<circle cx="286" cy="182" r="6.5" fill="{CREAM}" stroke="{INK}" stroke-width="1"/>')
out.append(f'<circle cx="284" cy="182" r="3.2" fill="#16110C"/>')
facets([(268, 162), (332, 154), (322, 98)], HAT, pull=0.1)  # copa
facets([(254, 166), (348, 154), (352, 164), (258, 176)], HAT)  # ala
facets([(322, 98), (332, 89), (343, 97), (333, 106)], SHIRT)  # pompón

# Articulaciones: los huesos del rig
for p in [neck, sh_l, sh_r, el_l, el_r, ha_l, ha_r, hip_l, hip_r, kn_l, kn_r, an_l, an_r]:
    joint(p)

# ─── Cruceta e hilos ───
bar_l, bar_r = (162, 46), (436, 46)
cross_a, cross_b = (252, 24), (348, 66)
string(bar_l, ha_l)
string(cross_a, (257, 170))
string(cross_b, (349, 159))
string(bar_r, kn_r)
string((404, 46), ha_r)
string((300, 46), (332, 89))

# Barra principal y travesaño, facetados como la rama de la portada
def bar(a, b, w):
    dx, dy = b[0] - a[0], b[1] - a[1]
    l = math.hypot(dx, dy)
    nx, ny = -dy / l * w, dx / l * w
    n = 6
    for i in range(n):
        t0, t1 = i / n, (i + 1) / n
        p0 = (a[0] + dx * t0, a[1] + dy * t0)
        p1 = (a[0] + dx * t1, a[1] + dy * t1)
        facets([(p0[0] + nx, p0[1] + ny), (p1[0] + nx, p1[1] + ny), (p1[0] - nx, p1[1] - ny), (p0[0] - nx, p0[1] - ny)], "#7a5430", stroke=INK, pull=0.3)


bar(cross_a, cross_b, 6)
bar((bar_l[0] - 8, bar_l[1]), (bar_r[0] + 8, bar_r[1]), 7)

# Encuadre ajustado a la cruceta, las esquinas del visor y el piso
print('<svg xmlns="http://www.w3.org/2000/svg" viewBox="104 10 392 556" width="392" height="556">')
print("\n".join(out))
print("</svg>")
