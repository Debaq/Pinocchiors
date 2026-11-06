# Plan: laminar para impresoras de resina (SLA/MSLA)

Laminar en Pinocchio también para las impresoras de resina del laboratorio, con la misma
idea que el de filamento: preparar en Fabricar (orientar, vaciar, soportes), laminar y
mandar el archivo que la impresora lee. **Solo plan por ahora** (pedido del usuario,
2026-11-05).

> **Regla:** las impresoras no se tocan sin pedido explícito del usuario (memoria
> `no-tocar-impresiones`). Las pruebas reales, con el usuario al lado.

## Las impresoras

| Impresora | Pantalla | Volumen | Archivo | Cómo llega |
|---|---|---|---|---|
| **Creality Halot-Sky (CL-89)** | 8,9" mono 4K, **3840 × 2400** | 192 × 120 × 200 mm | **`.cxdlp`** (Creality; el modelo 2025 usa `.cxdlpv4`) | USB y **WiFi** (Halot Box / Creality Cloud): ver si se puede mandar por la red |
| **Creality LD-002H** | 6" mono 2K, ~**2560 × 1440** (a confirmar) | ~130 × 82 × 160 mm | **`.ctb`** (Chitubox; versión a confirmar) | USB |

Datos de fichas y foros, **sin confirmar en estas impresoras**: la resolución, la versión
del formato y si la imagen va espejada se sacan de un archivo real (fase SLA0). Si la
resolución del archivo no es la de la pantalla, la impresora no lo imprime.

## Por qué es viable

- **Lo básico es más simple que el filamento**: cada capa es una **imagen en blanco y
  negro** (la máscara que la pantalla LCD deja pasar), a la resolución de la pantalla. Cortar
  la malla en capas y pintar esos polígonos es rápido en Rust y en paralelo (rayon).
- **Pinocchio ya tiene media preparación**: vaciado con pared y **agujeros de drenaje** y
  respiradero (`hollow`), orientación, cortes, reparación de mallas.
- **Referencias abiertas**:
  - **mslicer** (connorslade): laminador MSLA **en Rust**, escribe `.ctb`. La mejor base
    para el núcleo y el formato Chitubox.
  - **UVtools** (sn4k3, C#): lee y escribe casi todos los formatos, entre ellos **CXDLP**
    (desde v2.9.0, con perfil de la Halot-Sky CL-89; ojo: el tiempo de exposición va
    redondeado ×10 y las velocidades en mm/s) y CTB/CBDDLP. Sirve para verificar nuestros
    archivos abriéndolos ahí.
  - catibo (doc del formato cbddlp/ctb), uv3dp (Go).
  - **PrusaSlicer** (`libslic3r/SLA/`): puntos de soporte y árbol de soportes. Orca lo
    quitó, así que no está en el motor que ya compilamos.

## Fases

| Fase | Contenido | Estado |
|---|---|---|
| SLA0 | **Archivos reales** del usuario (cubo de 20 mm + escalón de 10 mm, 0,05 mm) | ✅ hecha (2026-11-05): ambos formatos decodificados por completo y verificados contra la pieza (`scripts/analizar-resina.py`); detalle abajo |
| SLA1 | **Núcleo `libs/sla`** (Rust): cortar la malla en capas (intersección con planos; reusar lo de cortes de print3d), rellenar polígonos con agujeros a la resolución de la pantalla, **antialias** (sobremuestreo, niveles de gris), espejo/rotación por impresora, en paralelo. Volumen de resina (ml y g) y tiempo | pendiente |
| SLA2 | **Formatos**: escribir **CTB** (LD-002H, partiendo de mslicer) y **CXDLP** (Halot-Sky, partiendo de UVtools), con miniaturas y parámetros. Prueba: el archivo se abre igual en UVtools que uno de Chitubox/Halot Box con la misma pieza | pendiente |
| SLA3 | **Preparar para resina** (Fabricar): inclinar 30–45° para que despegue mejor, elevar sobre la placa, vaciar con drenaje y respiradero (ya existe), detectar **islas** por capa (zonas que nacen en el aire) y **ventosas** (huecos cerrados que hacen succión) | pendiente |
| SLA4 | **Soportes**: puntos automáticos (islas, voladizos, bordes), soportes con punta fina, ramas y base (balsa); editar puntos a mano en el visor. Decidir entre portar a Rust o compilar la parte SLA de PrusaSlicer como otro motor aparte (como hicimos con Orca) | pendiente |
| SLA5 | **Perfiles**: por impresora (resolución, tamaño de píxel, volumen, espejo) y por resina (exposición normal y de base, capas de base, subida y velocidades, espera). Calibrar exposición con una pieza de prueba | pendiente |
| SLA6 | **Interfaz**: las dos SLA en la lista de máquinas de Fabricar con su foto; pestaña de laminar para resina; **vista previa capa por capa** mostrando la máscara; estimación de resina y tiempo | pendiente |
| SLA7 | **Enviar**: LD-002H por USB (guardar el archivo); Halot-Sky: investigar su protocolo de red (WiFi / Halot Box) sin tocarla hasta que el usuario diga | pendiente |
| SLA8 | **Pruebas reales con el usuario**: pieza de calibración en cada una, comparando con lo que sale de Chitubox/Halot Box | pendiente |

## Formatos verificados (SLA0)

**LD-002H — `.ctb` Chitubox v2** (little-endian): firma `0x12FD0086`, versión 2. Cama
82,62 × 130,56 × 160 mm, **1620 × 2560 px (51 µm)**, proyección espejada (`1`), antialias 1,
PWM 255. Del archivo de ejemplo: capa 0,05 mm, exposición 3,5 s, base 20 s × 8 capas.
Tabla de capas de 36 bytes (z, exposición, luz apagada, offset, tamaño, …). Cada capa:
**cifrada** con la clave de la cabecera (`0x6F676E6D`, «mngo»: XOR con un flujo que sale
de `seed*0x2d83cdac+0xd8a83423` y `(capa*0x1e1530cd+0xec3d47cd)*init`, como UVtools
`LayerRleCryptBuffer`) y después **RLE de 7 bits** (bit alto = racha, largo en 1–4 bytes;
gris de 7 bits extendido a 8). Decodificada da exactamente 1620 × 2560 px; el cubo mide
19,99 mm y el escalón 10,00 mm, centrados.

**Halot-Sky — `.cxdlp` Creality v2** (big-endian): `CXSW3DV2`, versión 2, modelo `CL-89`,
**3840 × 2400 px (50 µm)**, 64 bytes en cero, 3 miniaturas RGB565 (116², 290², 290²,
cada una seguida de `\r\n`), plataforma y capa como texto UTF-16 («192.00», «120.00»,
«0.050»), 11 parámetros u16 (exposición 3, espera 1, base 30, 6 capas de base, subida 7,
velocidades 1, PWM 255; el archivo lo hizo **Lychee**). **Unidad de la exposición sin resolver**: UVtools (desde v3.2.0) la guarda en **décimas** (3 = 0,3 s) y la de base en segundos, pero Lychee escribió 3 y 30, que en segundos (3 s / 30 s) son los valores típicos de la Halot-Sky. Se confirma sin imprimir: seleccionar el archivo en la impresora y ver qué exposición muestra la pantalla. Hacerlo antes de imprimir el primer `.cxdlp` de Pinocchio),
tabla de áreas por capa, y cada capa: área, cantidad de líneas y **líneas verticales de 6
bytes** (inicio Y 13 bits, fin Y 13 bits **inclusivo**, X 14 bits, gris). Pie `CXSW3DV2` y
un **checksum XOR** de todo el archivo (verificado: coincide). El cubo da 20,00 × 20,00 mm.

## Riesgos

- **Formatos exactos**: una cabecera mal escrita = la impresora no lo abre o imprime mal.
  Por eso SLA0 (archivos reales) va primero y cada archivo se verifica en UVtools.
- **Versiones de CTB**: las nuevas (v4/v5) van cifradas; las impresoras viejas como la
  LD-002H suelen usar v2/v3 sin cifrar. Confirmar con el archivo real.
- **Soportes**: son lo que decide si la impresión sale; malos soportes = resina perdida y
  piezas despegadas. Es la fase más larga.
- **Seguridad con resina**: guantes y ventilación; los tiempos de exposición dependen de la
  resina: siempre calibrar.

## Fuentes

- [Halot-Sky: 3840×2400, 192×120×200 mm (tabla de LIQCreate)](https://www.liqcreate.com/?p=7397)
- [Halot-Sky 2025: archivo `.cxdlpv4` (wiki de Creality)](https://wiki.creality.com/en/halot-series/halot-sky-2025/FAQ)
- [UVtools v2.9.0: soporte CXDLP y perfil Halot-Sky CL-89](https://newreleases.io/project/github/sn4k3/UVtools/release/v2.9.0) · [v3.2.0: arreglos CXDLP](https://newreleases.io/project/github/sn4k3/UVtools/release/v3.2.0)
- [mslicer: formato Chitu CTB (Rust)](https://www.mintlify.com/connorslade/mslicer/formats/chitu-ctb)
- [catibo: formato cbddlp/ctb](https://github.blesdmm.dns-dynamic.net/cbiffle/catibo/blob/master/doc/cbddlp-ctb.adoc)
- [uv3dp: tabla de impresoras y formatos (Go)](https://pkg.go.dev/github.com/ezrec/uv3dp@v1.2.12)
- [LD-002H: 2K mono, archivos .ctb con Chitubox](https://core-electronics.com.au/creality-ld-002h-uv-resin-3d-printer.html.plain.md)
