# Medir e inspeccionar

Objetivo: las herramientas de consulta que se usan todo el tiempo en Onshape: medir lo
elegido, propiedades de masa con material, vista de corte, vistas estándar, filtros de
selección y menú contextual.

## Qué tenemos hoy

- Sección "Sólido" (`BodySection` en `DesignStep.tsx`): volumen, área y tamaño de la caja
  envolvente del cuerpo entero.
- `Shape::mass()` en `cad-occt` (volumen, área, centro de masa, momentos) y `cad_face_info`
  (tipo de cara, normal, radio de cilindros).
- `cad_face_distance` / `cad_edge_distance` en el puente para resolver referencias.
- La pestaña Vista de la app y el cubo de vistas existen para el visor de mallas
  (`lib/ViewCube.ts`), no para `CadViewer`.
- `CadView.tsx` bloquea el menú contextual del navegador sin ofrecer uno propio.

## Qué falta

| Falta | En Onshape |
|---|---|
| Medir lo elegido | Al elegir dos cosas, abajo a la derecha aparece distancia mínima, distancia entre centros, ángulo, largo, área, radio |
| Masa con material | "Propiedades de masa": densidad por material, masa, centro de masa dibujado, momentos de inercia |
| Vista de corte | Plano de corte temporal, no cambia el modelo |
| Vistas estándar y cubo | Frente/arriba/derecha/isométrica, normal a la cara elegida |
| Zoom a lo elegido | Tecla F o "Ajustar a selección" |
| Filtros de selección | Solo caras, solo aristas, solo vértices, solo sketches |
| Selección por caja | Arrastrar un rectángulo |
| Menú contextual | Clic derecho: ocultar, aislar, editar operación, nuevo sketch en la cara, medir |
| Atajos | Shift+S (sketch en la cara), E (extruir), Shift+7 (isométrica), N (normal a) |
| Ocultar / aislar | Ocultar caras u objetos temporalmente |

## Diseño

### Medir

- Comando nuevo `cad_measure(refs: [Ref]) -> Measurement` en `apps/desktop/src/cad.rs`. Recibe
  lo elegido en `picks` (cara, arista, vértice, región de sketch, plano) y responde según qué y
  cuántos:
  - 1 cara: área, tipo, normal o radio. 1 arista: largo, radio si es circular. 1 vértice: coordenadas.
  - 2 entidades: distancia mínima (`BRepExtrema_DistShapeShape`, ya disponible en TKBRep),
    distancia entre centros, componentes ΔX ΔY ΔZ, ángulo si son caras planas o aristas rectas.
- Panel flotante en la esquina inferior derecha del visor, como Onshape. La distancia mínima se
  dibuja como segmento con los dos puntos más cercanos.
- Las unidades siguen la preferencia de la app (mm por defecto; ejes con Z arriba en la UI).

### Masa y material

- `Document.material: Option<Material { nombre, densidad_kg_m3 }>` (por pieza cuando exista
  [PLAN_PIEZAS.md](PLAN_PIEZAS.md)). Lista corta de materiales comunes (PLA, PETG, ABS,
  aluminio, acero, madera) más "personalizado".
- `BodySection` agrega masa, centro de masa (con botón para mostrarlo en el visor) y momentos
  principales. `mass()` ya calcula lo necesario.

### Vista de corte

- Solo de visualización: plano de corte con `clippingPlanes` de three.js sobre el material del
  sólido, más la tapa (relleno de la sección) con la técnica del stencil buffer.
- El plano se elige: plano base, cara plana, o se arrastra con un manipulador. No toca el modelo.

### Vistas y navegación

- Reusar `ViewCube.ts` en `CadViewer` (o factorizar lo común). Vistas estándar con Z arriba.
- "Normal a": con una cara plana o sketch elegido, gira la cámara perpendicular (ya se hace al
  entrar al sketch; se expone como acción).
- F = ajustar a lo elegido (caja envolvente de las caras/aristas elegidas); sin selección = todo.

### Filtros, caja y menú

- Filtro en la barra del visor: Todo / Caras / Aristas / Vértices / Sketches; `CadViewer.pick`
  ya ordena candidatos por tipo, el filtro descarta los demás.
- Selección por caja: arrastre con el botón izquierdo en vacío; elige lo que queda dentro
  (vértices de la malla de la cara proyectados) según el filtro activo.
- Menú contextual con `components/ui/ContextMenu.tsx`: acciones según lo que hay bajo el cursor.
- Atajos registrados en el gestor de atajos de la app, activos solo en el espacio Diseñar.

### Vértices

- Hoy solo se eligen caras y aristas. Agregar vértices del sólido (`TopAbs_VERTEX`) como
  candidatos de elección con prioridad sobre aristas; necesarios para medir y para anclajes.

## Fases

1. **Medir** (1 y 2 entidades) con panel flotante; vértices elegibles.
2. **Masa con material** y centro de masa visible.
3. **Vistas estándar, normal a, ajustar a lo elegido**, cubo de vistas.
4. **Menú contextual y atajos.**
5. **Filtros y selección por caja.**
6. **Vista de corte** con tapa.

## Pruebas

- Rust: `cad_measure` con una caja de 10×20×30: distancia entre caras opuestas = 30, ángulo
  entre caras adyacentes = 90°, largo de arista, radio de un cilindro.
- Masa: cubo de 10 mm de acero (7850 kg/m³) = 7,85 g.
- E2E: elegir dos caras y leer el panel; cambiar material y leer la masa; corte con el plano XZ
  y verificar en captura que hay tapa.

## Riesgos

- **Distancia mínima** en piezas complejas puede tardar; correrla en el hilo de cálculo y
  mostrar "midiendo…".
- **Tapa del corte** con stencil requiere sólidos cerrados en la malla de vista (lo son si la
  teselación es buena); en caso de falla, mostrar el corte sin tapa.
