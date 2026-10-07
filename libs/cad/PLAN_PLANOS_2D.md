# Planos 2D

Objetivo: lo que Onshape llama *Drawings*: una hoja con vistas proyectadas del diseño (frente,
planta, lateral, isométrica), cortes y detalles, cotas, notas y cajetín, que se actualiza
cuando cambia el modelo y se exporta a PDF, SVG y DXF.

## Qué tenemos hoy

- Nada de planos en Pinocchiors.
- En el addon anterior (`cad-blender/ROADMAP_DRAWINGS.md`) hubo un avance: proyección por
  aristas visibles/ocultas con prueba de caras frontales, hoja A4/A3 con cajetín, cotas lineales
  dibujadas; quedó pendiente el HLR real, cotas de radio/ángulo, cortes y la exportación. Las
  decisiones de ese documento se reutilizan (alcance mínimo, flujo de cotas tipo Onshape,
  cajetín como plantilla con campos `{{titulo}}`, `{{fecha}}`, `{{escala}}`…).
- OCCT tiene el algoritmo de líneas ocultas (`HLRBRep_Algo` / `HLRBRep_PolyAlgo`), pero el
  toolkit **TKHLR** no está en `TOOLKITS` (`occt/build.rs`) ni en `scripts/build-occt.sh`.

## Diseño

### Datos

- `Drawing` como documento aparte dentro del proyecto `.pinocchio` (uno o varios por diseño):
  - `sheet: { tamaño (A4, A3, A2, Carta), orientación, escala por defecto, plantilla }`;
  - `views: Vec<View { id, kind, posición en la hoja, escala, fuente }>` con
    `kind = Projected { dirección } | Isometric | Section { de, línea, dirección } | Detail { de, centro, radio, escala }`;
  - `annotations: Vec<Annotation>` — cotas, notas, ejes de centro, marcas de centro.
- Las cotas referencian aristas y vértices del modelo **por sus orígenes** (como `EdgeRef`), no
  por coordenadas de la hoja: al recalcular el modelo, la cota sigue a la arista y su valor se
  actualiza. Si la referencia se pierde, la cota queda marcada (como en
  [PLAN_EDICION.md](PLAN_EDICION.md)).

### Proyección (Rust)

- Puente: `cad_hlr(shape, dirección, arriba) -> { visibles, ocultas, siluetas, bordes_lisos }`
  como polilíneas 2D, usando `HLRBRep_Algo` (exacto, más lento) con opción `HLRBRep_PolyAlgo`
  (sobre la teselación, rápido) para la vista previa.
- Cada segmento lleva el origen de la arista 3D de la que sale, para poder acotarlo.
- Corte: `BRepAlgoAPI_Section`/`split_keep` (ya existe) para quedarse con la mitad, y las caras
  del plano de corte se rellenan con rayado a 45°.
- Detalle: recorte circular de una vista a otra escala.

### Hoja (frontend)

- Editor en SVG (no three.js): zoom y paneo, vistas arrastrables, elegir aristas y vértices de
  las vistas para acotar.
- Cotas al estilo Onshape: herramienta única que infiere según lo elegido (1 arista recta →
  largo; 2 paralelas → distancia; círculo → diámetro; arco → radio; 2 rectas no paralelas →
  ángulo), con líneas de referencia, flechas y texto; arrastrar para ubicarla.
- Tolerancia simétrica (±) y precisión decimal por cota.
- Cajetín: plantilla SVG con campos reemplazables; el usuario puede cambiar la plantilla.
- Espacio de trabajo: pestaña "Plano" dentro de Diseñar o su propio botón en el header.

### Exportación

- **SVG**: directo del editor.
- **PDF**: desde el SVG en Rust (`svg2pdf` + `usvg`, sin dependencias de sistema) o con la
  impresión de WebKit. Preferir Rust para que sea igual en todos los sistemas.
- **DXF**: escritor propio de las entidades básicas (LINE, ARC, CIRCLE, LWPOLYLINE, TEXT,
  DIMENSION como bloque) en R12/2000; útil para corte láser/CNC. Opción "exportar solo la
  vista elegida a escala 1:1" (para el láser).

## Fases

1. **Proyección**: agregar TKHLR al enlace (medir tamaño), `cad_hlr` con visibles y ocultas,
   tres vistas estándar + isométrica en una hoja, exportar SVG. *Hecha el 2026-10-07: TKHLR ya entraba
   por XCAF (sin costo extra). `cad_hlr` con `HLRBRep_Algo` exacto: aristas y contornos
   visibles y ocultos, aristas tangentes; las ocultas que caen sobre una visible se descartan
   (las de atrás de una caja vista de frente). `cad_drawing` proyecta las vistas pedidas;
   `lib/drawing.ts` (puro, `node --test apps/web/e2e/drawing.test.mjs`) acomoda frente,
   planta y lateral según el diedro (primero ISO por defecto, tercero ANSI) más la
   isométrica (sin ocultas), elige la escala normalizada más grande que entra y arma el SVG
   con recuadro y cajetín (título, autor, material, hoja, fecha, escala, proyección). La hoja
   se abre sobre el visor ("Plano 2D" en la sección Sólido) y se exporta con
   `cad_write_text`.*
2. **Hoja y cajetín**: tamaños, escala, plantilla, mover vistas, PDF. *En parte: tamaños A4–A2 y Carta, escala automática o
   elegida, cajetín fijo. Faltan plantilla propia, mover vistas y PDF.*
3. **Cotas** asociativas (largo, distancia, radio, diámetro, ángulo) y notas.
4. **Cortes** con rayado y línea de corte A-A; **detalles**.
5. **DXF** (vista 1:1 para láser primero, hoja completa después).
6. **Varias hojas**, ejes y marcas de centro automáticos, lista de piezas (cuando haya
   [PLAN_ENSAMBLES.md](PLAN_ENSAMBLES.md)).

## Pruebas

- Rust: HLR de una caja vista de frente = 4 segmentos visibles y 0 ocultos; con un agujero
  pasante desde el costado = 2 líneas ocultas.
- Cotas asociativas: acotar el ancho, cambiar el parámetro del modelo, el valor de la cota
  cambia.
- DXF: leerlo con `ezdxf` (Python) o con LibreCAD en una prueba manual; SVG/PDF comparados por
  captura.

## Riesgos

- **Rendimiento de HLR exacto** en piezas con muchos redondeos: usar `PolyAlgo` para la vista
  previa y el exacto al exportar.
- **Tamaño del binario** con TKHLR (estimado pequeño, medir).
- **Alcance**: los planos pueden crecer sin fin (GD&T, símbolos de soldadura, normas). Mantener
  el corte del addon: cotas básicas, cortes y detalles; lo demás después.
