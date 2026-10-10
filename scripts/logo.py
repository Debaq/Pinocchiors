#!/usr/bin/env python3
"""Genera el logo de Pinocchio: una marioneta articulada de madera, low-poly,
colgada de su cruceta. Mismo estilo que la portada del manual de Monteverde:
facetas con trazo oscuro, fondo de puntos y esquinas de visor naranjas.

También genera una lámina por cada forma de empezar de la pantalla de inicio
(abrir, escanear, animar, diseñar, fabricar): la misma marioneta haciendo esa
tarea. La pantalla de inicio elige una distinta en cada arranque.

    python3 scripts/logo.py   # escribe apps/web/src/assets/logo.svg y splash/*.svg
"""
import math
import random
from pathlib import Path

ASSETS = Path(__file__).resolve().parent.parent / "apps/web/src/assets"

W = 600
ORANGE = "#E9A93C"
CREAM = "#F6F1E8"
INK = "#2a1d0e"
LASER = "#e2412b"
BLUEPRINT = "#3f6fb0"
LIGHT = (-0.55, -0.83)  # luz desde arriba a la izquierda

WOOD = "#c98f55"
SHIRT = "#b5532e"
PANTS = "#4f6b88"
SHOE = "#3a2a1a"
HAT = "#d08a2a"
CARDBOARD = "#c9a26a"
METAL = "#5d6670"

out: list[str] = []
# Con FLAT cada pieza sale como un solo polígono de color plano: sirve para
# siluetas (fantasmas de animación) y para los recortes con la forma del cuerpo
FLAT: dict | None = None


def hex2rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


def shade(color, f):
    r, g, b = hex2rgb(color)
    c = lambda v: max(0, min(255, round(v * f)))
    return f"#{c(r):02x}{c(g):02x}{c(b):02x}"


def pts(ps):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in ps)


def capture(fn, flat=None):
    """Lo que dibuja `fn`, aparte de la salida principal"""
    global out, FLAT
    saved, saved_flat = out, FLAT
    out, FLAT = [], flat
    try:
        fn()
        return out
    finally:
        out, FLAT = saved, saved_flat


def flat_poly(poly):
    f = FLAT
    out.append(
        f'<polygon points="{pts(poly)}" fill="{f.get("fill", "none")}" stroke="{f.get("stroke", "none")}"'
        f' stroke-width="{f.get("width", 1)}" stroke-linejoin="round"{f.get("extra", "")}/>'
    )


def facets(poly, color, stroke=INK, pull=0.18, rings=1):
    """Polígono convexo en abanico desde un centro corrido hacia la luz; cada
    faceta se aclara u oscurece según mire a la luz"""
    if FLAT is not None:
        flat_poly(poly)
        return
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


def circle(c, r, fill, stroke="none", width=1.0):
    if FLAT is not None:
        out.append(f'<circle cx="{c[0]:.1f}" cy="{c[1]:.1f}" r="{r}" fill="{FLAT.get("fill", "none")}"/>')
        return
    out.append(f'<circle cx="{c[0]:.1f}" cy="{c[1]:.1f}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{width}"/>')


def joint(p, r=5.2):
    out.append(f'<circle cx="{p[0]:.1f}" cy="{p[1]:.1f}" r="{r}" fill="{CREAM}" stroke="{ORANGE}" stroke-width="2.2"/>')
    out.append(f'<circle cx="{p[0]:.1f}" cy="{p[1]:.1f}" r="1.6" fill="{INK}"/>')


def string(a, b):
    out.append(f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="#3b3128" stroke-width="1.1" opacity=".75"/>')
    out.append(f'<circle cx="{b[0]:.1f}" cy="{b[1]:.1f}" r="2.4" fill="{ORANGE}"/>')


def line(a, b, color, width=1.0, extra=""):
    out.append(
        f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="{color}" stroke-width="{width}" stroke-linecap="round"{extra}/>'
    )


def diamond(c, r, fill=ORANGE):
    """Marca de fotograma clave, como en la línea de tiempo"""
    out.append(f'<polygon points="{pts([(c[0], c[1] - r), (c[0] + r, c[1]), (c[0], c[1] + r), (c[0] - r, c[1])])}" fill="{fill}" stroke="{INK}" stroke-width="1"/>')


# ─── Fondo, piso y esquinas de visor ───


def dots(center=(300, 470)):
    """Campo de puntos isométrico que se apaga hacia los bordes"""
    for row in range(-2, 30):
        for col in range(-2, 30):
            x = col * 22.5 + (row % 2) * 11.25
            y = row * 13 + 260
            d = math.hypot((x - center[0]) / 1.6, (y - center[1]) / 0.9)
            o = 0.22 - d / 900
            if o > 0.04 and 0 < x < W and 0 < y < W:
                out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="1.3" fill="{ORANGE}" opacity="{o:.2f}"/>')


FLOOR = [(300, 422), (450, 492), (300, 562), (150, 492)]


def iso(u, v):
    """Punto del piso: `u` va de la esquina de atrás a la derecha, `v` de atrás a la izquierda (0..150)"""
    return (300 + u - v, 422 + (u + v) * 70 / 150)


# Mismo mapeo como transformación SVG, para dibujar sobre el piso en coordenadas (u, v)
ISO_MATRIX = f"matrix(1 {70 / 150:.4f} -1 {70 / 150:.4f} 300 422)"


def floor(shadow=(305, 496, 62, 15)):
    """Losa isométrica con grilla, como el suelo del visor"""
    out.append(f'<polygon points="{pts(FLOOR)}" fill="{CREAM}"/>')
    for i in range(1, 6):
        t = i / 6
        a, b = iso(150 * t, 0), iso(150 * t, 150)
        out.append(f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="#d9cfbf" stroke-width="1"/>')
        a, b = iso(0, 150 * t), iso(150, 150 * t)
        out.append(f'<line x1="{a[0]:.1f}" y1="{a[1]:.1f}" x2="{b[0]:.1f}" y2="{b[1]:.1f}" stroke="#d9cfbf" stroke-width="1"/>')
    if shadow:
        cx, cy, rx, ry = shadow
        out.append(f'<ellipse cx="{cx}" cy="{cy}" rx="{rx}" ry="{ry}" fill="#000" opacity=".13"/>')


def corners():
    for (x, y, sx, sy) in [(118, 92, 1, 1), (482, 92, -1, 1), (118, 548, 1, -1), (482, 548, -1, -1)]:
        out.append(
            f'<polyline points="{x},{y + sy * 30} {x},{y} {x + sx * 30},{y}" fill="none" stroke="{ORANGE}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>'
        )


# ─── Marioneta ───

# Pose del logo: brazo derecho y rodilla derecha alzados por sus hilos
LOGO_POSE = dict(
    head=(302, 194),
    neck=(300, 226),
    sh_l=(268, 240),
    sh_r=(332, 240),
    el_l=(226, 276),
    el_r=(380, 222),
    ha_l=(196, 318),
    ha_r=(396, 160),
    hip_l=(284, 350),
    hip_r=(318, 350),
    kn_l=(280, 418),
    kn_r=(362, 404),
    an_l=(276, 478),
    an_r=(350, 462),
)


def pose(**changes):
    return {**LOGO_POSE, **changes}


def puppet(P, joints=True, before_front_arm=None):
    """Torso y cabeza quedan fijos; brazos y piernas siguen la pose"""
    # Pierna derecha (atrás)
    limb(P["hip_r"], P["kn_r"], 15, 12, PANTS)
    limb(P["kn_r"], P["an_r"], 9, 7, WOOD)
    an_r = P["an_r"]
    facets([(an_r[0] - 8, an_r[1] - 6), (an_r[0] + 30, an_r[1] - 4), (an_r[0] + 34, an_r[1] + 8), (an_r[0] - 6, an_r[1] + 10)], SHOE)
    # Pierna izquierda (de apoyo)
    limb(P["hip_l"], P["kn_l"], 15, 12, PANTS)
    limb(P["kn_l"], P["an_l"], 9, 7, WOOD)
    an_l = P["an_l"]
    facets([(an_l[0] + 8, an_l[1] - 6), (an_l[0] - 30, an_l[1] - 2), (an_l[0] - 33, an_l[1] + 11), (an_l[0] + 7, an_l[1] + 11)], SHOE)

    # Brazo derecho (atrás del torso)
    limb(P["sh_r"], P["el_r"], 12, 10, SHIRT)
    limb(P["el_r"], P["ha_r"], 8, 6, WOOD)
    blob(P["ha_r"], 10, 7, WOOD, rot=0.3)

    # Torso y pantalón corto
    facets([(264, 236), (300, 228), (336, 236), (330, 300), (322, 340), (278, 340), (270, 300)], SHIRT, rings=2)
    facets([(276, 334), (324, 334), (330, 362), (302, 368), (300, 356), (298, 368), (270, 362)], PANTS)
    facets([(306, 252), (312, 264), (306, 276), (300, 264)], "#e0b25a")  # botón

    if before_front_arm:
        before_front_arm()

    # Brazo izquierdo (adelante)
    limb(P["sh_l"], P["el_l"], 12, 10, SHIRT)
    limb(P["el_l"], P["ha_l"], 8, 6, WOOD)
    blob(P["ha_l"], 10, 7, WOOD, rot=0.9)

    # Cabeza, nariz larga hacia la izquierda, ojo y sombrero cónico
    blob(P["head"], 40, 10, WOOD, rings=2, squash=1.0, rot=0.2)
    facets([(276, 226), (300, 238), (324, 226), (316, 218), (300, 226), (284, 218)], CREAM, stroke="#8a7a66")  # collar
    facets([(266, 186), (266, 200), (176, 196)], "#d9a066", pull=0.0)  # nariz
    facets([(278, 208), (296, 214), (284, 218)], "#9a6236", pull=0.0)  # boca
    circle((286, 182), 6.5, CREAM, INK)
    circle((284, 182), 3.2, "#16110C")
    facets([(268, 162), (332, 154), (322, 98)], HAT, pull=0.1)  # copa
    facets([(254, 166), (348, 154), (352, 164), (258, 176)], HAT)  # ala
    facets([(322, 98), (332, 89), (343, 97), (333, 106)], SHIRT)  # pompón

    # Articulaciones: los huesos del rig
    if joints:
        for k in ["neck", "sh_l", "sh_r", "el_l", "el_r", "ha_l", "ha_r", "hip_l", "hip_r", "kn_l", "kn_r", "an_l", "an_r"]:
            joint(P[k])


def bar(a, b, w, color="#7a5430", segments=6):
    """Barra facetada como la rama de la portada"""
    dx, dy = b[0] - a[0], b[1] - a[1]
    l = math.hypot(dx, dy)
    nx, ny = -dy / l * w, dx / l * w
    for i in range(segments):
        t0, t1 = i / segments, (i + 1) / segments
        p0 = (a[0] + dx * t0, a[1] + dy * t0)
        p1 = (a[0] + dx * t1, a[1] + dy * t1)
        facets([(p0[0] + nx, p0[1] + ny), (p1[0] + nx, p1[1] + ny), (p1[0] - nx, p1[1] - ny), (p0[0] - nx, p0[1] - ny)], color, stroke=INK, pull=0.3)


BAR_L, BAR_R = (162, 46), (436, 46)
CROSS_A, CROSS_B = (252, 24), (348, 66)


def strings(P, hand_r=True):
    """Hilos de la cruceta a las manos, la cabeza, la rodilla y el pompón"""
    string(BAR_L, P["ha_l"])
    string(CROSS_A, (257, 170))
    string(CROSS_B, (349, 159))
    string(BAR_R, P["kn_r"])
    if hand_r:
        string((404, 46), P["ha_r"])
    string((300, 46), (332, 89))


def crossbar():
    bar(CROSS_A, CROSS_B, 6)
    bar((BAR_L[0] - 8, BAR_L[1]), (BAR_R[0] + 8, BAR_R[1]), 7)


def silhouette(P):
    """Polígonos del cuerpo sin trazo, para usar dentro de un clipPath"""
    return capture(lambda: puppet(P, joints=False), flat={"fill": "#000"})


def clip_path(id, shapes):
    out.append(f'<clipPath id="{id}">{"".join(shapes)}</clipPath>')


# ─── Láminas ───


def scene_logo():
    dots()
    floor()
    corners()
    P = LOGO_POSE
    puppet(P)
    strings(P)
    crossbar()


def scene_animate():
    """Papel cebolla: dos poses anteriores en silueta, arcos de movimiento y
    fotogramas clave en las manos y la rodilla que tiran los hilos"""
    dots()
    floor()
    corners()
    ghosts = [
        pose(el_r=(372, 262), ha_r=(400, 300), kn_r=(328, 418), an_r=(334, 478), el_l=(234, 290), ha_l=(214, 340)),
        pose(el_r=(380, 240), ha_r=(408, 228), kn_r=(346, 414), an_r=(344, 474), el_l=(230, 282), ha_l=(204, 328)),
    ]
    P = LOGO_POSE
    for g, opacity in zip(ghosts, [0.2, 0.34]):
        shapes = capture(lambda g=g: puppet(g, joints=False), flat={"fill": ORANGE})
        out.append(f'<g opacity="{opacity}">{"".join(shapes)}</g>')
    # Arcos de movimiento con un fotograma clave en cada pose
    for k in ["ha_r", "kn_r", "ha_l"]:
        path = [g[k] for g in ghosts] + [P[k]]
        out.append(
            f'<polyline points="{pts(path)}" fill="none" stroke="{ORANGE}" stroke-width="2" stroke-dasharray="2 5" stroke-linecap="round" opacity=".9"/>'
        )
    puppet(P)
    for k in ["ha_r", "kn_r", "ha_l"]:
        for g in ghosts:
            diamond(g[k], 6)
    strings(P)
    crossbar()


def scene_open():
    """Saca un modelo de una caja abierta"""
    dots()
    floor(shadow=(300, 492, 58, 14))
    corners()
    P = pose(
        el_r=(368, 280), ha_r=(382, 330),
        el_l=(232, 284), ha_l=(212, 330),
        kn_r=(326, 418), an_r=(330, 478),
    )
    box_back, box_front = open_box((412, 474), 36, 40)
    out.extend(box_back)
    # El modelo sube desde la caja: cubo facetado con estelas
    c = (406, 372)
    for dx, y0, y1 in [(-14, 420, 446), (0, 404, 438), (14, 418, 442)]:
        line((c[0] + dx, y0), (c[0] + dx, y1), ORANGE, 2.2, ' opacity=".8"')
    cube(c, 22)
    sparkle((440, 340), 7)
    sparkle((374, 388), 4)
    sparkle((452, 392), 5)
    out.extend(box_front)
    puppet(P)
    strings(P)
    crossbar()


def open_box(base, w, h):
    """Caja isométrica abierta con las solapas hacia afuera. Devuelve lo de
    atrás (solapas traseras e interior) y lo de adelante (caras y solapas)"""
    k = 70 / 150
    cx, cy = base
    b = [(cx, cy - w * k), (cx + w, cy), (cx, cy + w * k), (cx - w, cy)]  # atrás, der, frente, izq
    t = [(x, y - h) for x, y in b]
    flap = 0.75

    def outward(a, c, d):
        # Solapa sobre la arista a-c, abierta hacia afuera (lejos de d) y algo hacia arriba
        mx, my = (a[0] + c[0]) / 2 - d[0], (a[1] + c[1]) / 2 - d[1]
        l = math.hypot(mx, my)
        ox, oy = mx / l * w * flap, my / l * w * flap * 0.6 - w * 0.35
        return [a, c, (c[0] + ox, c[1] + oy), (a[0] + ox, a[1] + oy)]

    centre = (cx, cy - h)
    back = capture(lambda: (
        facets(outward(t[3], t[0], centre), CARDBOARD, pull=0.1),
        facets(outward(t[0], t[1], centre), CARDBOARD, pull=0.1),
        facets(t, shade(CARDBOARD, 0.45), pull=0.0),
    ))
    front = capture(lambda: (
        facets([b[3], b[2], t[2], t[3]], shade(CARDBOARD, 0.9), pull=0.1),
        facets([b[2], b[1], t[1], t[2]], shade(CARDBOARD, 0.75), pull=0.1),
        line(((b[2][0] + b[3][0]) / 2, (b[2][1] + b[3][1]) / 2 - h * 0.15), ((b[2][0] + b[3][0]) / 2, (b[2][1] + b[3][1]) / 2 - h * 0.85), "#8a6a3a", 3),
        facets(outward(t[2], t[3], centre), CARDBOARD, pull=0.1),
        facets(outward(t[1], t[2], centre), CARDBOARD, pull=0.1),
    ))
    return back, front


def cube(c, s):
    """Cubo isométrico con caras facetadas en naranja"""
    k = 0.5
    top = [(c[0], c[1] - s), (c[0] + s, c[1] - s * k), (c[0], c[1]), (c[0] - s, c[1] - s * k)]
    left = [(c[0] - s, c[1] - s * k), (c[0], c[1]), (c[0], c[1] + s), (c[0] - s, c[1] + s * (1 - k))]
    right = [(c[0], c[1]), (c[0] + s, c[1] - s * k), (c[0] + s, c[1] + s * (1 - k)), (c[0], c[1] + s)]
    facets(top, shade(ORANGE, 1.1), pull=0.2)
    facets(left, ORANGE, pull=0.2)
    facets(right, shade(ORANGE, 0.78), pull=0.2)


def sparkle(c, r):
    x, y = c
    q = r * 0.28
    p = [(x, y - r), (x + q, y - q), (x + r, y), (x + q, y + q), (x, y + r), (x - q, y + q), (x - r, y), (x - q, y - q)]
    out.append(f'<polygon points="{pts(p)}" fill="{ORANGE}"/>')


def scene_scan():
    """Sobre el plato giratorio con el láser barriendo: lo ya escaneado queda
    como nube de puntos"""
    dots()
    floor(shadow=None)
    corners()
    P = pose(
        el_l=(232, 282), ha_l=(208, 326),
        el_r=(368, 282), ha_r=(392, 326),
        kn_l=(282, 418), an_l=(280, 476),
        kn_r=(320, 418), an_r=(322, 476),
    )
    # Plato giratorio
    tc, rx, ry, th = (300, 486), 112, 44, 14
    out.append(f'<path d="M {tc[0] - rx},{tc[1]} A {rx} {ry} 0 0 0 {tc[0] + rx},{tc[1]} L {tc[0] + rx},{tc[1] + th} A {rx} {ry} 0 0 1 {tc[0] - rx},{tc[1] + th} Z" fill="{METAL}" stroke="{INK}" stroke-width="1.2"/>')
    out.append(f'<ellipse cx="{tc[0]}" cy="{tc[1]}" rx="{rx}" ry="{ry}" fill="#e7dfd2" stroke="{INK}" stroke-width="1.2"/>')
    out.append(f'<ellipse cx="{tc[0]}" cy="{tc[1]}" rx="{rx * 0.72}" ry="{ry * 0.72}" fill="none" stroke="#cfc4b2" stroke-width="1"/>')
    out.append(f'<ellipse cx="{tc[0] + 4}" cy="{tc[1] + 2}" rx="52" ry="13" fill="#000" opacity=".13"/>')
    # Flecha de giro por el frente del plato
    arc = [(tc[0] + math.cos(a) * (rx + 12), tc[1] + th + math.sin(a) * (ry + 8)) for a in [math.radians(d) for d in range(150, 40, -6)]]
    out.append(f'<polyline points="{pts(arc)}" fill="none" stroke="{ORANGE}" stroke-width="3" stroke-linecap="round"/>')
    e, p = arc[-1], arc[-2]
    ang = math.atan2(e[1] - p[1], e[0] - p[0])
    head = [(e[0] + math.cos(ang) * 9, e[1] + math.sin(ang) * 9), (e[0] + math.cos(ang + 2.2) * 8, e[1] + math.sin(ang + 2.2) * 8), (e[0] + math.cos(ang - 2.2) * 8, e[1] + math.sin(ang - 2.2) * 8)]
    out.append(f'<polygon points="{pts(head)}" fill="{ORANGE}"/>')

    # Mitad sin escanear normal; la otra mitad, nube de puntos
    sx = 306
    body = silhouette(P)
    clip_path("body", body)
    out.append(f'<clipPath id="unscanned"><rect x="0" y="0" width="{sx}" height="{W}"/></clipPath>')
    out.append(f'<clipPath id="scanned"><rect x="{sx}" y="0" width="{W}" height="{W}"/></clipPath>')
    shapes = capture(lambda: puppet(P, joints=False))
    out.append(f'<g clip-path="url(#unscanned)">{"".join(shapes)}</g>')
    # Nube de puntos como patrón: celdas de 4×4 puntos con tamaño y tono variados
    cell = []
    for row in range(4):
        for col in range(4):
            x, y = 0.85 + col * 3.4 + (row % 2) * 1.7, 0.85 + row * 3.4
            r = random.uniform(0.9, 1.45)
            cell.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.2f}" fill="{shade(PANTS, random.uniform(0.45, 0.9))}"/>')
    out.append(f'<pattern id="cloud" width="13.6" height="13.6" patternUnits="userSpaceOnUse">{"".join(cell)}</pattern>')
    cloud = [f'<rect x="{sx}" y="0" width="{W}" height="{W}" fill="url(#cloud)"/>']
    out.append(f'<g clip-path="url(#scanned)"><g opacity=".14">{"".join(capture(lambda: puppet(P, joints=False), flat={"fill": PANTS}))}</g>'
               f'<g clip-path="url(#body)">{"".join(cloud)}</g></g>')
    for k in ["neck", "sh_l", "el_l", "ha_l", "hip_l", "kn_l", "an_l"]:
        joint(P[k])

    # Escáner en su pie, a la derecha, con el abanico del láser
    dev = (462, 262)
    bar((dev[0], dev[1] + 14), (dev[0], 500), 4, METAL, segments=4)
    lens = (dev[0] - 16, dev[1])
    out.append(f'<polygon points="{pts([lens, (sx, 92), (sx, 500)])}" fill="{LASER}" opacity=".10"/>')
    out.append(f'<g clip-path="url(#body)"><line x1="{sx}" y1="80" x2="{sx}" y2="500" stroke="{LASER}" stroke-width="3"/></g>')
    line((sx, 486 - 2), (sx, 486 + 30), LASER, 2.4, ' opacity=".85"')
    facets([(dev[0] - 16, dev[1] - 16), (dev[0] + 16, dev[1] - 20), (dev[0] + 18, dev[1] + 14), (dev[0] - 14, dev[1] + 16)], "#3c434b")
    circle(lens, 6.5, LASER, INK, 1.2)
    circle((lens[0] - 1, lens[1] - 1), 2.4, "#ffd2c4")
    circle((dev[0] - 10, dev[1] - 22), 4.5, "#1d2227", INK)  # cámara

    strings(P)
    crossbar()


def scene_design():
    """Dibuja un boceto acotado sobre el piso con un lápiz, escuadra en mano"""
    dots()
    floor()
    corners()
    P = pose(
        el_l=(238, 298), ha_l=(226, 350),
        el_r=(374, 228), ha_r=(392, 180),
        kn_r=(340, 412), an_r=(344, 472),
    )
    # Boceto en coordenadas del piso
    nss = ' vector-effect="non-scaling-stroke"'
    c, r = (22, 100), 17
    tip_angle = 200
    arc_done = " ".join(
        f"{'M' if i == 0 else 'L'} {c[0] + r * math.cos(math.radians(a)):.2f},{c[1] + r * math.sin(math.radians(a)):.2f}"
        for i, a in enumerate(range(-60, tip_angle + 1, 5))
    )
    sketch = [
        # Líneas de construcción
        f'<line x1="2" y1="{c[1]}" x2="148" y2="{c[1]}" stroke="{BLUEPRINT}" stroke-width="1" stroke-dasharray="5 4"{nss}/>',
        f'<line x1="{c[0]}" y1="60" x2="{c[0]}" y2="140" stroke="{BLUEPRINT}" stroke-width="1" stroke-dasharray="5 4"{nss}/>',
        # Círculo: hecho hasta el lápiz, el resto apenas marcado
        f'<circle cx="{c[0]}" cy="{c[1]}" r="{r}" fill="none" stroke="{BLUEPRINT}" stroke-width="1" stroke-dasharray="2 3" opacity=".6"{nss}/>',
        f'<path d="{arc_done}" fill="none" stroke="{INK}" stroke-width="2.2" stroke-linecap="round"{nss}/>',
        # Placa con esquina redondeada y agujero
        f'<path d="M 92,46 L 140,46 L 140,96 Q 140,104 132,104 L 92,104 Z" fill="{ORANGE}" fill-opacity=".16" stroke="{INK}" stroke-width="2.2" stroke-linejoin="round"{nss}/>',
        f'<circle cx="116" cy="75" r="9" fill="none" stroke="{INK}" stroke-width="1.8"{nss}/>',
        # Cotas
        f'<line x1="148" y1="46" x2="148" y2="104" stroke="{ORANGE}" stroke-width="1.6"{nss}/>',
        f'<line x1="143" y1="46" x2="153" y2="46" stroke="{ORANGE}" stroke-width="1.6"{nss}/>',
        f'<line x1="143" y1="104" x2="153" y2="104" stroke="{ORANGE}" stroke-width="1.6"{nss}/>',
        f'<line x1="92" y1="38" x2="140" y2="38" stroke="{ORANGE}" stroke-width="1.6"{nss}/>',
        f'<line x1="92" y1="33" x2="92" y2="43" stroke="{ORANGE}" stroke-width="1.6"{nss}/>',
        f'<line x1="140" y1="33" x2="140" y2="43" stroke="{ORANGE}" stroke-width="1.6"{nss}/>',
        f'<text x="116" y="34" font-family="sans-serif" font-size="9" font-weight="700" fill="{INK}" text-anchor="middle">48</text>',
        f'<text x="0" y="0" transform="translate(156 75) rotate(-90)" dy="3" font-family="sans-serif" font-size="9" font-weight="700" fill="{INK}" text-anchor="middle">58</text>',
        f'<text x="{c[0] - 14}" y="{c[1] + r + 12}" font-family="sans-serif" font-size="9" font-weight="700" fill="{INK}">Ø 34</text>',
    ]
    out.append(f'<g transform="{ISO_MATRIX}">{"".join(sketch)}</g>')

    tip = iso(c[0] + r * math.cos(math.radians(tip_angle)), c[1] + r * math.sin(math.radians(tip_angle)))
    hand = P["ha_l"]

    def pencil():
        # Del extremo de arriba (goma) a la punta, pasando por la mano
        dx, dy = tip[0] - hand[0], tip[1] - hand[1]
        l = math.hypot(dx, dy)
        ux, uy = dx / l, dy / l
        top = (hand[0] - ux * 22, hand[1] - uy * 22)
        cone = (tip[0] - ux * 20, tip[1] - uy * 20)
        bar(top, cone, 5.5, HAT, segments=5)
        nx, ny = -uy * 5.5, ux * 5.5
        facets([(cone[0] + nx, cone[1] + ny), tip, (cone[0] - nx, cone[1] - ny)], "#e9c79a", pull=0.0)
        facets([(tip[0] - ux * 7 + nx * 0.35, tip[1] - uy * 7 + ny * 0.35), tip, (tip[0] - ux * 7 - nx * 0.35, tip[1] - uy * 7 - ny * 0.35)], INK, pull=0.0)
        eraser = (top[0] - ux * 9, top[1] - uy * 9)
        facets([(top[0] + nx, top[1] + ny), (top[0] - nx, top[1] - ny), (eraser[0] - nx, eraser[1] - ny), (eraser[0] + nx, eraser[1] + ny)], "#d9776b", pull=0.0)

    puppet(P, before_front_arm=pencil)

    # Escuadra en la mano alzada
    h = P["ha_r"]
    tri_outer = [(h[0] - 6, h[1] - 64), (h[0] + 52, h[1] + 6), (h[0] - 6, h[1] + 6)]
    tri_inner = [(h[0] + 6, h[1] - 30), (h[0] + 26, h[1] - 6), (h[0] + 6, h[1] - 6)]
    out.append(
        f'<path d="M {pts(tri_outer)} Z M {pts(tri_inner[::-1])} Z" fill="{ORANGE}" fill-opacity=".55" fill-rule="evenodd" stroke="{INK}" stroke-width="1.2" stroke-linejoin="round"/>'
    )
    for i in range(1, 8):
        y = h[1] + 6 - i * 8
        line((h[0] - 6, y), (h[0] - 6 + (5 if i % 2 else 9), y), INK, 1)
    blob(h, 10, 7, WOOD, rot=0.3)
    joint(h)

    strings(P)
    crossbar()


def scene_fabricate():
    """Impresión por capas: de la cintura para abajo ya impresa, arriba solo
    el contorno de lo que falta, y el cabezal en la capa actual"""
    dots()
    floor(shadow=None)
    corners()
    P = pose(
        el_l=(228, 268), ha_l=(268, 310),
        el_r=(372, 268), ha_r=(332, 310),
        kn_l=(282, 418), an_l=(280, 476),
        kn_r=(320, 418), an_r=(322, 476),
    )
    cut = 336
    # Pórtico de la impresora: postes atrás, travesaño arriba con la bobina
    bar((160, 498), (160, 64), 5, METAL, segments=8)
    bar((440, 498), (440, 64), 5, METAL, segments=8)
    bar((152, 64), (448, 64), 6, METAL, segments=8)
    spool = (396, 40)
    circle(spool, 24, "#3c434b", INK, 1.2)
    circle(spool, 17, ORANGE, INK, 1)
    circle(spool, 6, CREAM, INK, 1)

    # Cama: placa sobre el piso
    out.append(f'<ellipse cx="302" cy="490" rx="56" ry="13" fill="#000" opacity=".13"/>')

    body = silhouette(P)
    clip_path("body", body)
    out.append(f'<clipPath id="printed"><rect x="0" y="{cut}" width="{W}" height="{W}"/></clipPath>')
    out.append(f'<clipPath id="pending"><rect x="0" y="0" width="{W}" height="{cut}"/></clipPath>')
    shapes = capture(lambda: puppet(P, joints=False))
    layers = "".join(
        f'<line x1="100" y1="{y:.1f}" x2="500" y2="{y:.1f}" stroke="{INK}" stroke-width=".8" opacity=".22"/>'
        for y in [cut + 4.5 * i for i in range(1, 40)]
    )
    out.append(f'<g clip-path="url(#printed)">{"".join(shapes)}<g clip-path="url(#body)">{layers}</g></g>')
    outline = capture(lambda: puppet(P, joints=False), flat={"fill": "none", "stroke": ORANGE, "width": 1.5, "extra": ' stroke-dasharray="4 3"'})
    out.append(f'<g clip-path="url(#pending)">{"".join(outline)}</g>')
    # Capa que se está depositando
    out.append(f'<g clip-path="url(#body)"><rect x="100" y="{cut - 3}" width="400" height="4" fill="{ORANGE}"/></g>')

    # Eje X y cabezal: la boquilla toca la capa actual
    nozzle = (334, cut - 2)
    bar((150, cut - 52), (450, cut - 52), 5, METAL, segments=8)
    out.append(f'<path d="M {spool[0] - 8},{spool[1] + 16} C 380,140 360,200 {nozzle[0] + 8},{cut - 70}" fill="none" stroke="{ORANGE}" stroke-width="2.2"/>')
    facets([(nozzle[0] - 22, cut - 72), (nozzle[0] + 22, cut - 72), (nozzle[0] + 22, cut - 30), (nozzle[0] - 22, cut - 30)], "#3c434b")
    facets([(nozzle[0] - 9, cut - 30), (nozzle[0] + 9, cut - 30), (nozzle[0] + 3, cut - 10), (nozzle[0] - 3, cut - 10)], "#b08d57", pull=0.0)
    facets([(nozzle[0] - 3, cut - 10), (nozzle[0] + 3, cut - 10), nozzle], "#8a6d3f", pull=0.0)
    out.append(f'<circle cx="{nozzle[0]}" cy="{nozzle[1]}" r="6" fill="{ORANGE}" opacity=".45"/>')
    for k in ["hip_l", "hip_r", "kn_l", "kn_r", "an_l", "an_r"]:
        joint(P[k])


SCENES = {
    "logo": scene_logo,
    "splash/open": scene_open,
    "splash/scan": scene_scan,
    "splash/animate": scene_animate,
    "splash/design": scene_design,
    "splash/fabricate": scene_fabricate,
}


def render(fn):
    global out
    out = []
    random.seed(7)
    fn()
    # Encuadre ajustado a la cruceta, las esquinas del visor y el piso
    return "\n".join(['<svg xmlns="http://www.w3.org/2000/svg" viewBox="104 10 392 556" width="392" height="556">', *out, "</svg>"]) + "\n"


if __name__ == "__main__":
    for name, fn in SCENES.items():
        path = ASSETS / f"{name}.svg"
        path.parent.mkdir(exist_ok=True)
        path.write_text(render(fn))
        print(path.relative_to(ASSETS.parent.parent.parent.parent))
