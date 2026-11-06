#!/usr/bin/env python3
# Analiza un .ctb (Chitubox v2, LD-002H) y un .cxdlp (Creality v2, Halot-Sky): cabecera,
# parámetros y capas decodificadas. Ver libs/pinocchio/print3d/PLAN_SLA.md (SLA0).
#   python3 -I scripts/analizar-resina.py pieza.ctb pieza.cxdlp
"""Analiza un .ctb (v2) y un .cxdlp (v2): cabecera, parámetros y capas decodificadas."""
import struct, sys
M32 = 0xFFFFFFFF

def ctb_crypt(seed, layer, data):
    if seed == 0: return data
    init = (seed * 0x2d83cdac + 0xd8a83423) & M32
    key = ((layer * 0x1e1530cd + 0xec3d47cd) & M32) * init & M32
    out = bytearray(len(data)); idx = 0
    for n, b in enumerate(data):
        k = (key >> (8 * idx)) & 0xFF
        idx += 1
        if idx & 3 == 0:
            key = (key + init) & M32; idx = 0
        out[n] = b ^ k
    return bytes(out)

def ctb_rle_decode(d, total):
    out = bytearray(); n = 0
    while n < len(d):
        code = d[n]; stride = 1
        if code & 0x80:
            code &= 0x7F; n += 1; s = d[n]
            if s & 0x80 == 0: stride = s
            elif s & 0xC0 == 0x80: stride = ((s & 0x3F) << 8) | d[n+1]; n += 1
            elif s & 0xE0 == 0xC0: stride = ((s & 0x1F) << 16) | (d[n+1] << 8) | d[n+2]; n += 2
            elif s & 0xF0 == 0xE0: stride = ((s & 0x0F) << 24) | (d[n+1] << 16) | (d[n+2] << 8) | d[n+3]; n += 3
            else: raise ValueError("RLE inválido")
        if code: code = (code << 1) | 1
        out += bytes([code]) * stride; n += 1
    return out

def bbox(img, w, h):
    xs=[];ys=[]
    for y in range(h):
        row = img[y*w:(y+1)*w]
        if any(row):
            ys.append(y); xs.append(next(i for i,v in enumerate(row) if v)); xs.append(w-1-next(i for i,v in enumerate(reversed(row)) if v))
    return (min(xs),min(ys),max(xs),max(ys)) if ys else None

def ctb(path):
    b = open(path,'rb').read()
    f = lambda o: struct.unpack_from('<f', b, o)[0]; u = lambda o: struct.unpack_from('<I', b, o)[0]
    print(f"== CTB {path}\nmagic {u(0):#x} versión {u(4)} | cama {f(8):.2f}×{f(12):.2f}×{f(16):.0f} mm | alto {f(28):.2f} mm | capa {f(32):.3f} mm")
    print(f"exposición {f(36)} s, base {f(40)} s × {u(48)} capas, luz apagada {f(44)} s | resolución {u(52)}×{u(56)} | capas {u(68)} | tiempo {u(76)} s")
    print(f"espejo/proyección {u(80)} | antialias {u(92)} | pwm {struct.unpack_from('<HH',b,96)} | clave {u(100):#x} ({b[100:104]})")
    w, h, nl, lt, key = u(52), u(56), u(68), u(64), u(100)
    pw, ph = f(8)/w, f(12)/h
    print(f"píxel {pw*1000:.1f} × {ph*1000:.1f} µm")
    for li in [0, 7, 8, 399, 400, 450, nl-1]:
        o = lt + li*36
        z, exp, off, adr, size = f(o), f(o+4), f(o+8), u(o+12), u(o+16)
        img = ctb_rle_decode(ctb_crypt(key, li, b[adr:adr+size]), w*h)
        bb = bbox(img, w, h)
        dims = f"{(bb[2]-bb[0]+1)*pw:.2f} × {(bb[3]-bb[1]+1)*ph:.2f} mm" if bb else "vacía"
        grays = sorted(set(img)) if len(set(img))<12 else f"{len(set(img))} grises"
        print(f"  capa {li:3d} z={z:.3f} exp={exp} {size} bytes → {len(img)} px ({'OK' if len(img)==w*h else 'MAL'}) caja {bb} = {dims} grises {grays}")

def cxdlp(path):
    b = open(path,'rb').read(); o = 0
    def rd(fmt):
        nonlocal o; v = struct.unpack_from('>'+fmt, b, o); o += struct.calcsize('>'+fmt); return v if len(v)>1 else v[0]
    ms = rd('I'); magic = b[o:o+ms]; o += ms
    ver = rd('H'); ps = rd('I'); model = b[o:o+ps]; o += ps
    nl, w, h = rd('H'), rd('H'), rd('H'); o += 64
    print(f"== CXDLP {path}\nmagic {magic} versión {ver} modelo {model} | capas {nl} | resolución {w}×{h}")
    o += 116*116*2 + 2 + 290*290*2 + 2 + 290*290*2 + 2
    def wstr():
        nonlocal o; n = rd('I'); s = b[o:o+n].decode('utf-16-be', 'replace'); o += n; return s
    px, py, lt = wstr(), wstr(), wstr()
    vals = [rd('H') for _ in range(11)]
    names = ["exposición","espera antes","exposición base","capas base","subida base","vel subida base","subida","vel subida","vel bajada","pwm base","pwm"]
    print(f"plataforma {px} × {py} mm | capa {lt} mm"); print("  " + ", ".join(f"{n}={v}" for n,v in zip(names, vals)))
    areas = [rd('I') for _ in range(nl)]; o += 2
    if ver >= 3: print("  (v3: hay bloque extra)")
    pw, ph = float(px)/w, float(py)/h
    print(f"píxel {pw*1000:.1f} × {ph*1000:.1f} µm | área capa 0 {areas[0]} capa 450 {areas[450] if nl>450 else '-'}")
    for li in range(nl):
        area, lines = rd('I'), rd('I')
        data = b[o:o+6*lines]; o += 6*lines + 2
        if li in (0, 7, 8, 399, 400, 450, nl-1):
            xs=[];ys=[];grays=set();px_count=0
            for k in range(lines):
                v = int.from_bytes(data[6*k:6*k+5],'big'); g = data[6*k+5]
                sy, ey, x = v >> 27, (v >> 14) & 0x1FFF, v & 0x3FFF
                xs.append(x); ys += [sy, ey]; grays.add(g); px_count += ey-sy+1
            bb = (min(xs),min(ys),max(xs),max(ys)) if xs else None
            dims = f"{(bb[2]-bb[0]+1)*pw:.2f} × {(bb[3]-bb[1]+1)*ph:.2f} mm" if bb else "vacía"
            print(f"  capa {li:3d}: área {area} líneas {lines} px≈{px_count} caja {bb} = {dims} grises {sorted(grays)[:8]}")
    fs = rd('I'); foot = b[o:o+fs]; o += fs; chk = struct.unpack_from('>I', b, o)[0]
    x = 0
    for c in b[:o]: x ^= c
    print(f"pie {foot} | checksum {chk:#x} XOR calculado {x:#x} | bytes leídos {o+4} de {len(b)}")

ctb(sys.argv[1]); cxdlp(sys.argv[2])
