# Plan: imprimir en varios colores en la Flashforge AD5X

Sigue a `PLAN_IMPRESORAS.md` (la AD5X ya imprime desde Pinocchio en **un** color, eligiendo
el carrete al enviar). Objetivo: piezas con hasta **4 colores o materiales** (los 4
carretes del IFS), asignados en Pinocchio, laminados con nuestro motor y mandados con
cada color en su carrete.

> **Regla:** las impresoras no se tocan sin pedido explícito del usuario (ver memoria
> `no-tocar-impresiones`). Todo se desarrolla contra impresoras falsas y el motor local;
> las pruebas reales, solo con el usuario al lado.

## Lo que ya sabemos

**La AD5X** (perfil de Orca 2.4.2, `Flashforge AD5X 0.4 nozzle`):
- `single_extruder_multi_material = 1`: una boquilla, 4 carretes. El **cambio lo hace el
  firmware** al leer `T0`…`T3` (`change_filament_gcode` vacío): corta, descarga, carga el
  nuevo y purga en el conducto de atrás. En el log se vio la carga: `F23 C3`, `F39 C3`,
  `fourInOneLoad…`.
- Cada cambio cuesta **~39 s de carga + 39 s de descarga** (`machine_load/unload_filament_time`)
  más la purga: muchos cambios = mucho tiempo y mucho plástico tirado.
- Orca deja la **torre de purga activada** (`enable_prime_tower = 1`, 45 mm de ancho,
  `purge_in_prime_tower = 0`, `filament_minimal_purge_on_wipe_tower = 15`).
- Al subir por la API 8898, `materialMappings` asigna cada herramienta a un carrete
  (`toolId` desde 0 = la T del gcode, `slotId` desde 1) y `gcodeToolCnt` = cuántas. Probado
  con una herramienta; con varias es el mismo formato (así lo arma Orca).
- Los gcodes multicolor que ya imprimió el laboratorio (Orca 2.3.1: `Ensamblaje.gcode`,
  `tm_logo.gcode`, `cindywhite.gcode`) empiezan en cualquier T (`T2 T0 T1`).

**El motor (libslic3r de Orca)**:
- Color **por pieza**: `ModelObject::config["extruder"]` (o por volumen).
- Color **pintado por caras**: `ModelVolume::mm_segmentation_facets` (un `TriangleSelector`
  con estado por triángulo: `EnforcerBlockerType::Extruder1…`); `MultiMaterialSegmentation`
  corta cada capa por color. Es lo que usa el pincel de colores de Orca.
- Color **por altura**: rangos de capas por objeto (`layer_config_ranges` con `extruder`).
- Hoy nuestro motor (`engine/main.cpp`) lee una pieza por STL y deja todo en el filamento 1;
  `filament_map` (filamento → boquilla) se queda en 1 porque hay una sola boquilla.
- La cantidad de purga entre colores (`flush_volumes_matrix`) Orca la **recalcula desde los
  colores** en su GUI (`FlushVolCalc` en libslic3r): hay que hacerlo nosotros.

**Pinocchio ya tiene**:
- Piezas separadas en Fabricar (`print3d_pieces`, cortes, uniones).
- Selección de caras de «Pieles» (`zones.rs`, `print_skin.rs`): elegir, crecer por ángulo,
  invertir, área. Sirve casi tal cual para pintar colores.
- Colores por vértice en los modelos importados (`scene/src/world.rs`: `colors`) y texturas.
- Vista previa del gcode por tipo de línea (`lib/gcodeView.ts`, `parse_gcode`).

## Fases

| Fase | Contenido | Estado |
|---|---|---|
| MC0 | **Estudiar un gcode multicolor real de Orca para la AD5X**: secuencia en cada cambio (T, purga, torre, temperaturas), qué dejó Orca en la cabecera y al final, `materialMappings` con varias herramientas. Pedirle al usuario un `.3mf`/`.gcode` multicolor hecho en Orca (o permiso para copiar uno de la impresora, solo leyendo) | pendiente |
| MC1 | **Motor**: cada pieza con su herramienta y, opcional, un color por triángulo (archivo aparte, 1 byte por cara: 0 = el de la pieza, 1–4 = carrete) → `config["extruder"]` y `mm_segmentation_facets`. Rangos de altura por pieza. Devolver gramos y metros **por herramienta**, cantidad de cambios, purga total y torre | pendiente |
| MC2 | **Laminador (Rust)**: `SliceSettings` con **un filamento por herramienta** (perfil y color propios: PLA blanco, PETG negro…) en vez de 4 copias iguales; `flush_volumes_matrix` calculada desde los colores (portar `FlushVolCalc`); torre de purga y «purgar dentro del relleno/objeto» como opciones | pendiente |
| MC3 | **Asignar colores en Pinocchio** (Fabricar): (a) **por pieza** en la lista de piezas; (b) **pintar caras** en el visor con las herramientas de zonas de «Pieles» (pincel, crecer por ángulo, invertir, rellenar); (c) **desde los colores del modelo** (por vértice, textura o 3MF) reducidos a ≤ 4 con k-medias en Lab, con vista previa; (d) **por altura** (desde la capa X, otro color: textos y logos en relieve). Se guarda en el proyecto `.pinocchio` | pendiente |
| MC4 | **Vista previa y resumen**: colorear las líneas por herramienta con el color real del filamento, leyenda por color, torre de purga visible; resumen con gramos por color, cambios, plástico purgado y tiempo de cambios (~78 s cada uno). Aviso si el desperdicio pasa de un umbral | pendiente |
| MC5 | **Enviar**: tabla herramienta → carrete con los colores y materiales que informa el IFS (`/detail`); se propone sola por color/material más parecido; no deja enviar si falta un carrete cargado o si el material no coincide (avisa si solo el color difiere); `materialMappings` con N entradas y `gcodeToolCnt = N` | pendiente |
| MC6 | **Pruebas reales con el usuario**: cubo de 2 colores por pieza, luego uno pintado y un logo por altura. Comparar contra Orca con el mismo modelo (gramos, cambios, tiempo) | pendiente |
| MC7 | **Menos desperdicio**: orden de colores por capa que minimice cambios, purgar en el relleno o en piezas de sacrificio, ajustar la torre; avisar cuando una pieza chica con muchos cambios no conviene | pendiente |

## Preguntas abiertas (decide el usuario)

1. **Qué forma de asignar color va primero**: por pieza (lo más simple y sirve para
   ensamblajes), pintar caras, desde los colores del modelo o por altura.
2. **¿Los colores pintados sobreviven a los cortes?** Si se pinta antes de cortar, el
   cortador tiene que llevar el color de cada cara a las piezas (como el pendiente de UV de
   «Pieles»). Si no, se pinta después de cortar, pieza por pieza.
3. **Purga**: torre (como Orca, segura) o además purgar dentro del relleno/objeto (menos
   desperdicio, más riesgo de mezcla visible).
4. Si un modelo trae **más de 4 colores**: ¿reducir solo a 4 o preguntar cuáles juntar?

## Riesgos

- **Desperdicio y tiempo**: cada cambio son ~78 s más la purga. Un logo de 2 colores en 50
  capas son ~100 cambios: más de 2 h solo cambiando. Hay que mostrarlo antes de imprimir.
- **Firmware de Flashforge frágil**: la primera prueba por la API quedó esperando tras
  cargar el carrete (venía de dos cancelaciones). Con varios cambios puede haber más
  sorpresas: probar de a poco y con el usuario.
- **Materiales distintos** (PLA + PETG) pegan mal entre sí: avisar al mezclar.
- **Purga por colores**: sin la matriz recalculada, de claro a oscuro sobra y de oscuro a
  claro falta (manchas). MC2 tiene que portar bien `FlushVolCalc`.

## Dónde mirar

- Motor: `libs/slicer/engine/main.cpp`; Orca: `src/libslic3r/Model.hpp`
  (`mm_segmentation_facets`, `layer_config_ranges`), `TriangleSelector.hpp`,
  `MultiMaterialSegmentation.cpp`, `FlushVolCalc.cpp`, `GCode/WipeTower*.cpp`.
- Envío: `apps/desktop/src/flashforge.rs` (`upload_headers`) y `src/slic3r/GUI/PrintHostDialogs.cpp`
  de Orca (`FlashforgePrintHostSendDialog::extendedInfo`, asignación herramienta → carrete).
- Zonas de caras: `libs/pinocchio/print3d/src/zones.rs`, `apps/desktop/src/print_skin.rs`,
  `SkinPanel.tsx`.
