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
| SLA1 | **Núcleo** (Rust, `libs/slicer/src/sla.rs`): cortar la malla en capas a media altura, pintar cada capa por filas a la resolución de la pantalla (regla de giro distinto de cero: cáscaras que se pisan se suman, huecos de vaciado quedan huecos), en paralelo; volumen y tiempo estimado | ✅ hecho (2026-11-05) para las dos pantallas. Benchy: 960 capas en ~20 ms. **Falta**: confirmar el espejo con una pieza asimétrica (`flip_x`, hoy en `false`) |
| SLA2 | **Formatos**: **CTB v2** para la LD-002H (`src/ctb.rs`: escribir y leer, cifrado «mngo» + RLE de 7 bits) y después **CXDLP v2** para la Halot-Sky | CTB ✅ (2026-11-05), detalle abajo. CXDLP espera la unidad de exposición (el usuario la confirma) |
| SLA3 | **Preparar para resina**: elevar sobre la placa; **islas** por capa; **ventosas** (huecos encerrados sin salida hacia la placa, que succionan al despegar) y **huecos cerrados** (resina atrapada); estrategias: **venteo** automático en el piso de cada ventosa y comparar **inclinaciones** (área por capa, succión, altura). El vaciado con drenaje ya existe (`print3d::hollow`) | ✅ núcleo (2026-11-05): `sla_check.rs` (componentes por tramos, unión entre capas), `sla_prepare.rs` (`prepare`, `tilt_options`) |
| SLA4 | **Soportes**: puntos automáticos (caras en voladizo más planas que 45°, repartidos a `spacing`; uno por isla), punta + columna + pie + balsa (envolvente convexa); apoyados en la pieza si abajo hay pieza; densidad liviana/media/pesada; puntos editables (se pasan a `prepare`). Propio en Rust (no PrusaSlicer) | ✅ núcleo (2026-11-05): `sla_supports.rs`. Benchy: 4 venteos, 223 soportes, quedan 0 islas y 0 huecos, en ~0,7 s. Editar puntos en el visor ✅ (2026-11-06: «Editar puntos», clic en la pieza agrega, clic en un punto quita, «Un punto en cada isla», «Rehacer soportes»). **Falta**: refuerzos cruzados y ramas (árbol), punta inclinada según la normal |
| SLA5 | **Perfiles**: por impresora (`sla::printers`) y por resina (`sla_profiles.rs`: estándar, 0,03 mm, lavable, tipo ABS, tenaz; puntos de partida desde los archivos de referencia) y revisión de parámetros (`validate`). Calibrar exposición con una pieza de prueba | ✅ perfiles y revisión (2026-11-05); pieza de calibración ✅ (2026-11-06, `sla_calibration.rs`: pines, agujeros, ranuras y paredes de 0,1 a 2,5 mm; «Guardar las pruebas» escribe un archivo por exposición y «Usar» fija la elegida) |
| SLA6 | **Interfaz**: LD-002H y Halot-Sky en la lista de máquinas de Fabricar (`lib/machines.ts`, `kind: "resin"`, material «Resina»); en Imprimir, si la máquina es de resina, `ResinSection.tsx` en lugar del laminador de Orca: perfil de resina, exposición, subida, soportes (densidad y medidas), venteos, comparar inclinaciones y usar una; vista previa con resumen (tiempo, ml, g, área máx.), revisión (islas, huecos, puntos descartados) y **máscara de la capa** en un lienzo; en el visor la pieza elevada con soportes cortada en la capa (`lib/resinView.ts`), venteos e islas marcados. Backend `apps/desktop/src/resin.rs` (`resin_presets`, `resin_validate`, `resin_slice`, `resin_tilts`, `resin_layer`, `resin_view`, `save_resin`) | ✅ en código (2026-11-05), probado en Chromium sin Tauri. **Falta**: probar en la app real (la edición de puntos en el visor no se probó: el arnés no dibuja 3D), fotos de las dos impresoras |
| SLA7 | **Enviar**: LD-002H por USB (guardar el `.ctb` desde la pestaña Enviar: hecho); Halot-Sky: investigar su protocolo de red (WiFi / Halot Box) sin tocarla hasta que el usuario diga | pendiente |
| SLA8 | **Pruebas reales con el usuario**: pieza de calibración en cada una, comparando con lo que sale de Chitubox/Halot Box | pendiente |

### Lo que tienen los programas de pago (Chitubox Pro, Lychee Pro/Premium, UVtools, PreForm)

Pedido del usuario (2026-11-06): ponerse a la altura de los laminadores de pago. En orden
de impacto en la pieza:

| Fase | Contenido | Estado |
|---|---|---|
| SLA9 | **Antialiasing**: bordes en grises (CTB v2 y CXDLP los admiten). Cada fila se muestrea en `n` subfilas y en x se mide la cobertura exacta de cada píxel (`sla::slice_aa`, `Layer::gray`); los tramos en blanco y negro (centro del píxel) se quedan para la revisión y los soportes. Niveles 1 (apagado), 2, 4, 8 («Suavizar bordes» en Exposición; `antialias` en el MCP). Benchy con nivel 8: 0,24 s, archivo 6,3 MB (3,7 sin grises); el `.ctb` con grises se decodifica bien en `scripts/analizar-resina.py` | ✅ (2026-11-06). **Falta**: ver en la LD-002H que lista el archivo con grises (sin imprimir) y comparar una pieza con y sin suavizado |
| SLA10 | **Compensaciones** (`sla_morph.rs`, panel «Compensar medidas»): sobre los tramos continuos de cada línea de muestreo, antes de pasar a píxeles (exacto en x, sirve igual con grises): **XY** (agrandar/achicar con un disco), **pata de elefante** (las capas de base se achican además), **agujeros** (aire encerrado de la capa, que no llega al borde, de hasta `hole_max` mm: se agranda y se resta). La revisión y los soportes ven la pieza ya compensada. Benchy con AA 8 + XY 0,2 mm: 2,3 s | ✅ (2026-11-06). Atenuar el interior de áreas grandes pasa a SLA15. **Falta**: medir la pieza de calibración impresa para sacar valores reales |
| SLA11 | **Gráfico de área por capa** (picos de fuerza al despegar) y **subida dinámica** (más lenta en capas con mucha área); exposición o alto de capa por tramos de altura | ✅ (2026-11-06) salvo alto de capa por tramos. `sla_layers.rs`: área por capa, `layer_params` (subida dinámica: recta entre `area_min` y `area_max` mm² hasta `speed_min`/`lift_max`; exposición por tramos de z, sin tocar la base), tiempo por capa. **El CTB v2 no lleva valores por capa** (UVtools no los permite en v2: la impresora usa los generales), así que van en **CTB v3** (opción «Formato del archivo» en el panel «Valores por capa»; por defecto v2): tabla con `TableSize` 84 y antes de cada capa un bloque de 84 bytes (copia de la entrada, tamaño, subida, velocidad, bajada, PWM), `0x30` en «parámetros por capa». Con v2 se ignoran y avisa. Gráfico en Vista previa › Capas (área, velocidad de subida, ventosas; clic = ir a la capa). Arreglado de paso: la bandera de grises de los datos del laminador era 7 siempre; con grises va 15. **Falta**: ver que la LD-002H abre un v3 (sin imprimir); alto de capa por tramos |
| SLA12 | **Soportes avanzados**: refuerzos cruzados, ramas (árbol), punta según la normal, soportes laterales, balsa con borde biselado, grosor por punto, pincel de zonas y «todo bajo X°» | ✅ 12a (2026-11-06), en `sla_supports.rs`: **punta según la normal** de la cara más cercana (tubo con tapas perpendiculares, hasta `tip_max_angle`, solo si desde ahí la columna baja libre a la balsa; si no, vertical como antes); **refuerzos** en zigzag a 45° entre cada columna y su vecina más cercana (hasta `brace_max_distance`); **ramas**: columnas a menos de `tree_radius` (hasta `tree_branches`) se juntan a 45° en un tronco más grueso, sacando las ramas que chocan; **balsa con chaflán** (`raft_bevel`, misma envolvente con círculos más chicos arriba); **grosor por punto** (`SupportPoint.tip`, «Punta de los puntos nuevos» al editar). Todo lo nuevo se prueba contra la pieza con rayos verticales cada 0,25 mm (eje y costados) y contra los venteos. «Todo bajo X°» = el «Ángulo límite» que ya existía. Benchy: 0 islas con y sin ramas; 129 columnas y 231 refuerzos para 223 puntos con ramas. **Falta (12b)**: pincel de zonas en el visor (sin soportes / más densos) y soportes laterales automáticos en paredes |
| SLA13 | **Orientación automática**: buscar el giro (no solo 15/30/45° en X o Y) puntuando islas, succión, área por capa y altura | ✅ (2026-11-06), `sla_orient.rs` (`auto_orient`, reemplaza a `tilt_options`): 260 direcciones de la esfera puntuadas sin laminar (voladizo proyectado + altura), las 20 mejores separadas 15° se laminan en grueso (0,3 mm) y se revisan, y se afina ±6° y ±3° alrededor de las tres mejores; si no entra, prueba girar en Z. Puntaje: área máx + 2 × succión + voladizo (relativos al giro 0) + 0,25 × alto/diagonal + 0,01 por isla. El giro pasa a ser `Turn {x, y, z}` (X, después Y, después Z) en la app, el MCP (`tilt: {x, y, z}`; acepta la forma vieja) y los proyectos (la forma vieja `{axis, degrees}` se convierte). Botón «Buscar la mejor orientación» en Preparar. Benchy: 1,6 s; área máx 1084 → 452 mm², succión 636 → 20, voladizo 977 → 236 mm². Un cubo queda de punta (sin voladizo). **Falta**: ver en la app real que el giro se ve bien en el visor |
| SLA14 | **Vaciado para resina**: relleno interior (panal/rejilla), agujeros de drenaje a mano con tapón imprimible | pendiente |
| SLA15 | **Operaciones sobre capas** (estilo UVtools): quitar islas de menos de N píxeles, cerrar huecos chicos, doble exposición, alivio bajo la balsa, atenuar el interior de áreas grandes (menos luz lejos del borde) | pendiente |
| SLA16 | **Nombre grabado en la balsa** (placa, resina, fecha) | pendiente |
| SLA17 | **CXDLP** (Halot-Sky) y envío por WiFi (= SLA2/SLA7 pendientes) | espera al usuario |

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

## CTB de Pinocchio contra el de referencia (SLA2)

`cargo run --release -p pinocchio-slicer --example sla_ctb -- pieza.stl salida.ctb` lamina el
STL centrado en la pantalla de la LD-002H. Con el cubo de SLA0, comparado byte a byte con el
`.ctb` de Lychee:

- **Mismo tamaño** (1 247 428 bytes), mismo orden de secciones y offsets.
- **Cabecera idéntica** salvo el tiempo estimado (6301 s nuestro, 6335 s el suyo).
- **Tabla de capas idéntica** bit a bit (z, exposición, offsets, tamaños).
- **Las 500 máscaras con la misma forma** (mismos píxeles, 392 × 392 px), corridas 1–2 px
  porque Lychee centra distinto; en la app la posición la da Fabricar.
- Parámetros: mismo volumen (8,49 ml); peso y costo salen de la densidad y precio de la resina.
- Miniaturas 210² y 200² en RGB565 sin rachas, como las de Lychee.

**Falta (con el usuario)**: abrir el `.ctb` de Pinocchio en la LD-002H por USB y ver que lo
lista y muestra la miniatura y los tiempos (sin imprimir); después una pieza **asimétrica**
(una «F» en relieve) laminada en Chitubox/Lychee para fijar el espejo.

## Revisión y soportes (SLA3–SLA5), cómo funciona

- **Laminado**: se pinta donde el giro es **positivo**: cáscaras que se pisan se suman y una
  cáscara dada vuelta resta; así el venteo es un cilindro al revés, sin booleanas. Cada
  arista se interpola siempre desde el mismo extremo (si no, una fila pierde un cruce y un
  hueco cerrado se «abre»: pasó con la caja hueca).
- **Revisión** (`check`): por capa, componentes de tramos encendidos y de aire dentro de la
  caja de la capa (aire que toca el borde = afuera). Entre capas: isla = componente sin
  apoyo abajo (< 0,01 mm² se cuenta aparte); el aire encerrado se une con el de abajo; si
  abajo hay aire de afuera, está venteado; si arriba hay aire de afuera, se abrió. Ventosa =
  ≥ 1 mm² y más de 2 capas sin salida; hueco cerrado = se tapa sin salida (> 1 mm³).
- **Flujo** (`prepare`): elevar (balsa + 5 mm) → laminar y revisar → venteo de 2,5 mm desde
  el piso de cada hueco hasta el primer aire de abajo → puntos (los que caen en un venteo se
  corren a su borde) → soportes → laminado final y revisión. Cada punto descartado dice por
  qué (`SkipReason`: muy cerca de la pieza, dentro de la pieza, sobre venteo…).
- **Orientar** (`sla_orient::auto_orient`, SLA13): busca en toda la esfera; ver la fila SLA13.
- Ejemplo: `cargo run --release -p pinocchio-slicer --example sla_ctb -- pieza.stl salida.ctb --preparar`
  (con `VISTA=archivo.ppm` guarda una vista de la pieza con soportes).

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
