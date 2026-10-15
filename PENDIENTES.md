# Pendientes

Regenerado el 2026-10-15 contrastando cada punto con el código actual y con los
planes vivos (`libs/cad/ROADMAP.md` y sus `PLAN_*.md`,
`libs/pinocchio/print3d/PLAN_PRINT3D.md`, `libs/PLAN_RENDER.md`,
`libs/orizon3d/ROADMAP.md`). Los `ROADMAP.md` de quadriflow, uv y repair ya no
existen: lo que seguía abierto de ellos está aquí. Lo ya hecho no se repite
(`git log --all -- '*ROADMAP*.md' '*PLAN*.md'`).

Leyenda: 🐞 error comprobado · ⬜ falta · 🟨 a medias · 🧪 falta probar ·
⚠ límite conocido (funciona así a propósito).

Verificado por lectura y búsqueda en el código; no se ejecutaron pruebas.

---

## 0. Antes de seguir

- 🧪 **Probar en la app real.** Es el hueco más grande: casi todo lo reciente
  solo tiene `cargo test`, `tsc`, `vite build` y mocks de IPC. Sin probar:
  print3d, GeppettoRay en Tauri (incluida la animación y Ctrl+Shift+R),
  escaneo→CAD E4.2–E10, engranes (F2 en adelante, incluido arrastrar piezas),
  panel de Orizon3D con el escáner real, captura con una persona real,
  abrir/guardar/cerrar con cambios/importar, constructor de cuerpo y variantes
  nuevas del esqueleto, cambios de UI de UV, panel de articulación con la
  diseñadora.
- 🧪 Correr `cargo test --workspace`. Hay Rust escrito sin compilar (nube, sesión
  del 2026-10-11 con `cargo` bloqueado). Para PLAN_OPERACIONES: `cargo test -p
  cad-model` (`--test operations --test hole`), luego `cad_http` + `vite` +
  `node apps/web/e2e/cad.mjs operaciones`. Dos pruebas de `cad-model` fallaban
  desde el commit `80f7a83` (`gear_part::avisos_y_errores`,
  `corona_helicoidal_y_cremallera`); sin confirmar.
- ⬜ `libs/converter/.ROADMAP.md.kate-swp` es un swap de Kate versionado en git:
  borrarlo.

## 1. Errores comprobados

- 🟨 Impresión 3D, campos sin usar (`libs/pinocchio/print3d`):
  - `SubdivideConfig::max_dimension` y `overlap` se declaran (`config.rs:14,17`)
    y nadie los lee.
  - `LabeledPiece.cut_faces` siempre `vec![]` (`slicer.rs`).
  - `LabelingScheme::Coordinate` genera `P{n}` (`slicer.rs:664`) y
    `From<SubdivideStrategy>` nunca lo elige.
  - `AssemblyInfo.total_volume` es 0 y `assembly_order` va por ID
    (`labeling.rs:73-81`, con TODO).
- ✅ Resuelto desde el último compilado: «Optimal (menos cortes)» (`cuts.rs`
  `optimal_planes`), cortes manuales, piezas en el visor, candado de proporción,
  puzzles y terraza (print3d Fases 9, 10 y 12).

## 2. Esqueleto

- ⬜ Limitación de la cadera en el ajuste (#19): sin señal geométrica nueva
  (`embedding/src/fit.rs`); el humanoide sintético queda en y≈0.42 (real 0.47).
- ⬜ Raíz de trompa del gonfoterio (la cabeza domina); anotado en la memoria, sin
  confirmar que esté hecho.
- 🧪 Variantes nuevas (reptiles, artrópodos, mono, rana), constructor de cuerpo
  y galería: compilados y con tipos verificados, nunca probados a mano ni
  ajustados a modelos reales.

## 3. Pose y animación

Principios vigentes: los formatos de salida solo entienden FK (todo se hornea al
exportar); hueso de deformación ≠ control; una sola pila por cuadro (keys FK →
restricciones → IK → resortes → límites); todo se deshace y se guarda en el
`.pinocchio`; genérico por especie.

Hecho (no repetir): restricciones, BVH y retargeting, captura MediaPipe y con
cámara en vivo (`LivePuppet`, `enable_camera` en Linux), resortes y ragdoll
desde la pose, piel con cuaterniones duales, exportación con mezcla de capas,
25 animaciones básicas, editor dividido RigEditor, panel de articulación.

Pendiente:
- ⬜ Captura: detección en un worker (hoy `detectForVideo` en el hilo
  principal, `captureRunner.ts`), cámara del Revopoint y elegir dispositivo,
  varias cámaras (`numPoses` = 1), cara y manos (Face/Hand Landmarker).
- ⬜ Estirar hacia sin preservación de volumen (`constraints.ts`, `stretch`).
- ⬜ Terreno irregular (el suelo es plano, `ragdoll.ts`), parpadeo (sin
  párpados), colisiones del ragdoll con el propio cuerpo.
- ⬜ Correctivos por ángulo, shape keys, huesos segmentados, preservación de
  volumen por escala. Shape keys: la escena no modela morph targets; el
  import de glTF solo lee los pesos y USD los descarta.
- ⬜ Exportar los controles como nodos vacíos (hoy van en `extras`).
- 🧪 Detección real con una persona delante: el modelo de MediaPipe sí baja y
  arranca (probado en Chromium con cámara falsa), falta probar con una persona.

## 4. Interfaz

Hecho: Outliner, Orizon3D en panel lateral con edición de nube, varios
escaneos y comparación con referencia, atajos reasignables por área
(`shortcuts.ts`), E2E propios.

- ⬜ Nube: selección solo de lo visible (sin oclusión), recortar por caja o
  plano con gizmo, alineación manual por puntos, malla por Poisson.
- ⬜ Arrastrar y soltar para importar, archivos recientes, vista previa de la
  información antes de importar.
- ⬜ Exportar USDA en texto (existe `Format::Usda`; `ExportStep.tsx` solo ofrece
  USDZ). 🟨 Opciones USD en la interfaz: el backend tiene `scale_factor` y
  `split_orm_channels`, la UI no las expone; revisar «eje arriba».
- ⬜ Impresión 3D en la interfaz: ver sección 6 y `PLAN_PRINT3D.md`.
- ⬜ Cuatro vistas; menús Editar/Vista/Malla (`Header.tsx` solo tiene el de
  archivo). 🟨 Ajuste a grilla/vértices: el visor 3D solo tiene snap de giro del
  gizmo (el del sketch CAD sí existe).
- ⬜ Tags y categorías en la biblioteca de poses (`StoredPose`).
- ⬜ Overlay lado a lado original/retopología.
- ⬜ Rendimiento: decimar la vista previa de mallas > 100k, workers, LOD.
- 🟨 Pruebas: hay E2E con `node --test` y un arnés CDP en `apps/web/e2e`, no
  Playwright ni E2E de la app Tauri completa; el CI no los corre.
- ⬜ CI sin clippy (`ci.yml` solo corre `cargo test`, `tsc` y `build`); ~30
  avisos de estilo pendientes (hace falta red hacia crates.io).
- PLAN_OBJETOS sección C: probar el cráneo y un modelo de una sola pieza en la
  app real; 🟨 en curso: Luces e Historial en Hint y revisar párrafos
  `leading-relaxed` sueltos.

## 5. Librerías

### Retopología (`libs/quadriflow`, pausada desde 2026-09-27)
- ⬜ Probar personajes reales (humanoide con manos); el banco `bench.sh` se
  perdió y hay que recrearlo.
- ⬜ Propuestas, en orden:
  1. Encarecer en `integer.rs` las cargas de posición que cruzan zonas con dos
     aristas vivas a menos de dos quads.
  2. (2A) Pares 3-5 junto a singularidades de orientación (🟨: `cleanup.rs`
     colapsa diagonales 3-5, sin lógica ligada a singularidades).
  3. (4) Densidad adaptativa por niveles (1×, ½×, ¼×) con transiciones 2:1
     (`sizing.rs` sigue continua).
  4. (3) Dual contouring con QEF en `rebuild.rs`.
  5. (5) Chaflanes angostos en el criterio de tamaño (después de 4).
- 🟨 Costuras UV: `mark_seams` descarta islas angostas pero no las une a la
  vecina ni suaviza la dirección.
- 🟨 «Determinista con cualquier número de hilos»: el test no varía los hilos.
- ⚠ Una esfera necesita 8 irregulares; un rasgo más angosto que un quad no se
  representa; toda transición de tamaño necesita irregulares.

### UV (`libs/uv`)
- 🧪 Distribución por partes solo medida en personajes sintéticos.
- ⬜ Lado escondido: la cabeza se abre por abajo (`parts.rs:214`); usar el frente
  del esqueleto.
- ⬜ Las guías pierden las tildes (`converter/layers/raster.rs`).
- ⬜ Bug del «−0» del BVH; costuras alineadas a las viejas; importar PLY; pasos
  2-4 de «para pintar» (Fase 3b; el paso 1 `Layout::Paintable` está hecho).

### Conversor (`libs/converter`)
- ⬜ FBX; morph targets reales; emulación de doble cara en USD por duplicado
  (hoy `doubleSided` nativo); exportar PLY (importa ASCII y binario, solo falta
  exportar).

### Reparación (`libs/pinocchio/repair`)
- ✅ Cuerpos solapados y auto-intersecciones: `resolve_intersections`, opción
  «Unir cuerpos solapados» (desactivada por defecto, se revierte en mallas
  abiertas). Quedan: lámina suelta, el banco completo (Guanaco, 1,2M caras)
  usa mucha RAM, y cuatro modelos de la lista no salen sanos.
- ⬜ Uniones en T entre piezas.
- 🟨 Parches de agujeros sin UV; confirmar que skin y jerarquía se conservan.
- ⬜ Índices `u32` (`trimesh.rs` usa `usize`).

### Auto-rigging (`libs/pinocchio`)
- ⬜ Validación contra el Pinocchio C++, suite estándar de mallas, métricas de
  skinning, benchmarks con `criterion` (declarado, sin uso).
- ⬜ API WASM propia (la feature `wasm` no tiene código), GPU (`wgpu`), FBX.

## 6. Impresión 3D (`libs/pinocchio/print3d`)

Detalle en `PLAN_PRINT3D.md`, «Pendientes y límites».
- ⬜ Contracción distinta por eje en la interfaz (los presets las traen iguales).
- ⬜ Máscara de borde suave pintada, en pieles.
- ⬜ Uniones y roscas se protegen a mano, no se detectan solas.
- 🧪 Nada probado con la app abierta ni con impresora; sin e2e de la sincronía
  impresora↔diseño, la selección de zona y el asistente de calibración; uniones
  y modelos de calibración sin verificar con un slicer; presets de holgura y
  contracción son valores típicos, no medidos; sombreado del relieve sin ver con
  una malla real.
- ⚠ Pared fina medida con 1 500 caras; soporte estimado en columnas verticales;
  orientación sugerida para el modelo entero; uniones solo en cortes planos y
  riel recto (no se pone si la cara tiene un hueco); vaciado deja macizo lo más
  fino que el doble de la pared; distribución por celdas con giros de 90°;
  compensación de agujeros solo en CAD; pieles con una sola escala de UV.

## 7. Render (`libs/geppettoray`, plan en `libs/PLAN_RENDER.md`)

Hechas las fases 1, 2 y 3a–3h (motor, BSDF glTF, luces y HDRI, vidrio esmerilado
y volúmenes anidados, cámara real, denoise espacial, animación, suelo con
sombra, HDRI elegible, ACES, luz de área texturizada) y 4a (fondo de estudio).
- ⬜ **4b** luces de estudio (softbox, aro, contraluz, 3 puntos) y **4c** presets
  de fotografía (cámara, lente, profundidad de campo, luces, fondo).
- 🧪 Todo lo de la app: GeppettoRay en Tauri real, animación, Ctrl+Shift+R,
  guardado del render del visor.
- ⬜ Interfaz: tamaño del sol/foco editable (`SUN_ANGULAR_RADIUS` fijo), esfera
  de la luz puntual/foco sin pasar al motor, exposición fija en 1.0.
- ⬜ Animación: cámara fija, sin desenfoque con cámara animada, una animación
  por render (no la secuencia de clips), sin denoise temporal, BVH reconstruido
  por cuadro.
- ⬜ Motor: cáusticas, `attenuationColor/Distance` (la escena no los trae),
  mipmaps, MikkTSpace, solo UV 0, luces del visor→motor no coinciden exacto,
  BVH sin instanciado, sin CLI (`apps/geppettoray`) ni WASM. Denoise: reflejos
  nítidos y vidrio no se separan del fondo; la profundidad de campo queda como
  ruido. Mapa de oclusión ignorado a propósito.

## 8. Escáner (`libs/orizon3d`)

- ✅ Fase 1 (banco sintético y láser) y `scanear.rs` (plato giratorio, probado
  contra un Arduino simulado).
- ⬜ Fase 2: calibración (Zhang, Levenberg-Marquardt, esquinas de tablero); no
  existe `calibrate.rs`.
- ⬜ Fase 3: `FrameSource` V4L2 de la webcam (foco, exposición, ganancia, balance
  manuales; descartar cuadros viejos); comparar escaneo real con el sintético;
  🟨 los comandos `PY:LASER` y `PY:LIGHT` son propuestos, falta el firmware.
- 🟨 Fase 4: `mesh.rs` es genérico; faltan filtro por ángulo rasante y visual
  hull del plato.
- ⬜ Fase 5: color y fotogrametría (proyección de fotos, estéreo multivista).
- ⬜ No simulado: interreflexiones, moteado, artefactos JPEG, superficies
  brillantes.

## 9. CAD (`libs/cad`)

Fases F0–F6 y F8–F10 cerradas; E0–E10 del escaneo con sección ✅ (el cuadro
«Punto de partida» de `PLAN_ESCANEO.md` está viejo y marca E4–E10 como ❌/🟡:
corregirlo).

- 🟨 **F7 distribución:** `release-cad.yml` en Windows y macOS escrito, nunca
  corrió; pueden faltar librerías. Linux probado en local.
- ⬜ **Ensambles y chapa (F9, sin hacer):** relación tangente; arrastre libre de
  instancias con rótula, plana o paralela; sub-ensambles; instancias de STEP
  importados; pestaña en arista curva (el desarrollo no entiende conos); volver
  a doblar el desarrollo; dobladillo cerrado sin radio; canvas único del visor.
- ⬜ **Operaciones:** 🟨 desfasar/mover cara solo en caras planas; reemplazar cara
  solo hasta un plano; omitir instancias de patrón solo por diálogo (falta
  clic en el visor); recubrimiento tangente a la cara vecina; redondeo de cara
  en esquinas de tres redondeos y cantos vivos; redondeo completo general;
  revolución a dos ángulos.
- ⬜ **Escaneo → CAD** (todo sin probar en la app real salvo E0–E3):
  - E1: centro de esfera como referencia; tramos abiertos del corte.
  - E2: superficie si el escaneo no cierra (OCCT falla en algunos casos).
  - E3: desmolde «hasta el escaneo»; caras no planas; elegir los triángulos
    (fijo en 20 000).
  - E4: la caja menos el conejo a −0,5 mm cortado a media altura falla; sin e2e
    de la región pintada; campo por vértice con borde suave; borrar o editar
    toques; molde: partir por contorno, respiraderos, bebedero que llegue a la
    cavidad, rebajes sobre el plano, llaves en el contorno.
  - E5: curvas como borde de región; barrer perfil sobre un escaneo real; curva
    por puntos con geodésica; geodésica exacta.
  - E6: corte vivo pieza+escaneo; informe por pieza; holgura por la normal.
  - E7: el redondeo no tiene botón de agregar.
  - E8: plano de corte a mano; varios lazos por corte; perfil con líneas y
    arcos reconocidos; revolución parcial.
  - E9: una sola base; agujeros ciegos y escalones como primitivas sueltas; sin
    patrones ni simetrías ni redondeos; sin varias caras de apoyo.
  - E10: ajuste por relleno de OCCT; borde ≤ 40 tramos; solo lazo exterior;
    sin tangencia con las vecinas.
- ⬜ **Plano 2D (fuera de foco por decisión del 2026-10-10):** arrastrar
  manipuladores, partir splines, redondeo/chaflán entre entidades sin punto
  común, mover arrastrando varias cosas, aviso al arrastrar lo restringido,
  cerrar huecos y quitar entidades muy cortas; 🟨 resaltado bajo el cursor,
  campo flotante de cotas, recortar en intersecciones, vista previa dinámica,
  spline por polos cerrada, fusionar puntos, cotas de la hoja (solo largo,
  distancia, H/V, radio, diámetro, ángulo). Hoja: las cotas no esquivan líneas
  de vistas ni de otras cotas; lo anclado a un vértice no sigue a la vista.
- ⬜ **Engranes:** paso diametral; hipoides, sin fin globoide, rueda por
  simulación, no circulares, helicoidales cruzados, cicloidal interno y
  cremallera cicloidal, resistencia del sin fin; planetario solo con satélites
  iguales y un brazo; cota entre bolas de la corona; rotular círculo base y de
  pie; perfil de la rueda de sin fin como bloque; sin fin como segundo del par.
  Avisos de ejes no paralelos solo al «Detectar engranes».
- ⚠ **Límites:** booleanas OCCT lentas con muchas caras (se trabaja a 20 000
  triángulos); carcasa con espesor ≥ 2,5 vóxeles y tope de 640 por eje;
  `Defeaturing` falla con caras que tocan muchas vecinas; copiar y pegar
  operaciones con referencias por geometría puede apuntar a otras caras;
  `ThruSections` sin cierre suave; patrón «variable» solo cambia el tamaño;
  cónico con extremos cónicos falla sobre 50°; espina de pescado con canal es
  lenta; E7 no segmenta con triángulos menores que la tolerancia (0,2 % de la
  diagonal); E9 solo piezas prismáticas o de revolución simples.
