# Pendientes

Regenerado el 2026-10-15 contrastando cada punto con el código actual y con los
planes vivos (`libs/cad/ROADMAP.md` y sus `PLAN_*.md`,
`libs/pinocchio/print3d/PLAN_PRINT3D.md`, `libs/PLAN_RENDER.md`,
`libs/orizon3d/ROADMAP.md`). Actualizado el 2026-11-06 con lo nuevo desde
entonces (secciones 10–14): laminador de filamento (`PLAN_SLICER.md`), resina
(`PLAN_SLA.md`), impresoras (`PLAN_IMPRESORAS.md`, `PLAN_MULTICOLOR.md`),
conector MCP y chat (`apps/PLAN_MCP.md`), Android (`apps/PLAN_ANDROID.md`),
pestañas y sketch del CAD (`libs/cad/PLAN_PESTANAS.md`, `PLAN_SKETCH_URGENTE.md`). Los `ROADMAP.md` de quadriflow, uv y repair ya no
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
  diseñadora. Desde el 2026-10-15 se sumó, también sin probar en la app: pestañas
  del CAD (F3–F8), plan urgente del sketch (U0–U8), laminador de filamento y de
  resina dentro de Fabricar, manejo de impresoras, chat y modo en vivo del MCP
  (el modo en vivo sí se probó una vez), actualizaciones automáticas y el APK.
- 🐞 Abrir/guardar proyectos: arreglado (`005e530`, `535c2b8`), quedan pruebas y
  revisiones en `roadmap_emergencia_2026-11-03.md` («Pendiente»), entre ellas el
  arnés e2e de Diseñar roto por `LiveAiBadge` (`mcp_live_status` devuelve `null`
  en `cad_http`).
- 🧪 Correr `cargo test --workspace`. Hay Rust escrito sin compilar (nube, sesión
  del 2026-10-11 con `cargo` bloqueado). Para PLAN_OPERACIONES: `cargo test -p
  cad-model` (`--test operations --test hole`), luego `cad_http` + `vite` +
  `node apps/web/e2e/cad.mjs operaciones`. Las dos pruebas de `cad-model` que
  fallaban desde `80f7a83` ya no existen o cambiaron (`gear_part::avisos_y_errores`
  ahora espera la cremallera helicoidal tallada); falta correrlas.

## 1. Errores comprobados

- ✅ Impresión 3D, campos sin usar (2026-10-16): `max_dimension` ahora limita el
  volumen útil por eje; `overlap` y `cut_faces` se quitaron (nadie los llenaba);
  `LabelingScheme::Coordinate` genera `X{i}Y{j}Z{k}` por la grilla de cortes
  (sin `From<SubdivideStrategy>`, no hacía falta); `AssemblyInfo` ordena el
  ensamble de abajo hacia arriba y suma `total_volume`.
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

- 🧪 Nube (2026-10-16, solo compilado y con pruebas unitarias; sin ver en el
  visor): selección solo de lo visible (casilla «Solo lo visible»), recortar por
  caja o plano con gizmo, alineación manual por puntos (`align_by_points`) y malla
  por Poisson (`poisson.rs`; probado con esfera sintética, no con escaneos reales).
- 🧪 Arrastrar y soltar para importar (proyectos y modelos, también sobre la
  ventana con trabajo abierto), archivos recientes (menú Archivo e inicio) e
  información previa a importar (`peek_model`, opcional en Configuración ▸
  Archivos): escritos, sin probar con la app Tauri real.
- 🧪 USDA en texto exportable (texturas al lado) y opciones de USD en Exportar
  (escala, separar ORM, AR Quick Look). «Eje arriba» se revisó: sale de
  `scene.y_up`, sin cambios.
- ⬜ Impresión 3D en la interfaz: la contracción por eje ya está (Impresora y material); el resto, en la sección 6 y `PLAN_PRINT3D.md`.
- 🧪 Cuatro vistas (Vista ▸ Cuatro vistas, Ctrl+Alt+Q: principal + superior, frontal
  y derecha ortográficas; rueda acerca, arrastrar desplaza, doble clic lleva la
  principal a esa dirección), menús Editar/Vista/Malla en la cabecera y ajuste al
  mover con el gizmo a la grilla o a los vértices (Vista ▸ Ajuste al mover).
  Cuatro vistas probado en Chromium con un prisma; el resto, solo con tipos.
  ⬜ Las vistas ortográficas no son editables (solo mirar) y no hay vistas
  nombradas propias; el ajuste a vértices pega las articulaciones a la malla y el
  modelo entero a los otros objetos.
- ✅ Tags y categorías en la biblioteca de poses (`StoredPose.category/tags`, búsqueda y filtro).
- 🧪 Comparar original y retopología lado a lado (casilla en Retopología): sin ver en el visor real.
- 🟨 Rendimiento: la vista previa liviana ahora sale desde 100 000 caras (antes 400 000); faltan decodificar en workers y LOD.
- 🟨 Pruebas: hay E2E con `node --test` y un arnés CDP en `apps/web/e2e`, no
  Playwright ni E2E de la app Tauri completa. El único workflow es `release.yml`
  (manual o tag `pre.*`; `ci.yml` se quitó el 2026-10-20): corre `cargo test
  --workspace`, no las pruebas de lógica `e2e/*.test.mjs` ni el arnés CDP.
- 🟨 Clippy: el job informativo se fue con `ci.yml`. ~120 avisos de estilo en 12
  crates (`cargo clippy --workspace --offline` del 2026-10-16); falta limpiarlos
  para poder exigirlos.
- PLAN_OBJETOS sección C: probar el cráneo y un modelo de una sola pieza en la
  app real. ✅ Luces e Historial ya usan Hint y los párrafos `leading-relaxed`
  de los paneles laterales pasaron a Hint (quedan avisos de estado y los de
  diálogos, Escáner y Diseñar, fuera del plan).
- ✅ **Paneles angostos pisaban rótulos con sus campos** (visto el 2026-10-16; primer
  arreglo ese día; revisado el 2026-10-24 montando los paneles reales en Chromium a 192,
  240, 288 y 448 px de contenido, con un detector de cajas encimadas o fuera del panel):
  las filas X/Y/Z con rótulo de eje se salían hasta 380 px (volumen de impresión,
  posición y rotación del esqueleto, medidas en Fabricar) y las grillas de 3 columnas se
  encimaban. `NumberInput` y `Num` (Diseñar): con rótulo de eje, campo compacto que
  reparte la fila (`flex-1 min-w-0`); con rótulo largo, el campo y su unidad van juntos
  (la unidad ya no baja sola). Contracción por eje: el «%» pasa al rótulo. «Auto-fit /
  Volver al preset» envuelven. Sin encimados en esos anchos; falta verlo en la app Tauri.
- ✅ Cámara de Diseñar: un sólido de otra escala que la vista heredada del visor
  principal ahora se encuadra solo (`frameIfLost`); falta ver el caso real
  engranes mm + humanoide.
- ✅ Render: exposición editable en el motor GeppettoRay (control «Exposición»,
  ×0,25–×4); el material por defecto ya era gris 0,8, así que «muy claro» se
  corrige con la exposición.

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
  (hoy `doubleSided` nativo). PLY ya importa (ASCII y binario) y exporta
  (`export_ply`).

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
  compensación de agujeros en CAD y en el laminador de resina (no en filamento);
  pieles con una sola escala de UV.

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

Fases F0–F6 y F8–F10 cerradas (F9 se cerró el 2026-10-15); E0–E10 del escaneo
con sección ✅.

- 🧪 **Pestañas** (`PLAN_PESTANAS.md`, 2026-10-22/24): F0–F2 hechas y probadas
  con fixtures; F3–F8 (barra de pestañas, objetos por pestaña, ensamble propio,
  pieza derivada, planos 2D del ensamble, exportar pestañas, variables del
  proyecto, subensambles) en código, falta probarlas en la app.
- 🧪 **Sketch urgente** (`PLAN_SKETCH_URGENTE.md`, 2026-11-03/05): U0–U8 hechos con
  pruebas `node --test`; falta probar cada uno en la app. Queda `nullspace` con
  `max_niter = 0` (U1) y «Reparar» de U5.
- 🟨 **F7 distribución:** un solo `release.yml` (2026-10-17) sale siempre con CAD
  (OCCT estático) y corre todos los tests con OCCT real. Linux funcionaba en la
  variante anterior; Windows tenía el C++ sin compilar con MSVC (arreglado con
  `extern "C++"`, sin verificar) y macOS fallaba
  `cremallera_helicoidal_con_piñon_de_las_dos_manos` (choque 0,74 mm³ que en
  Linux no aparece).
- ⬜ **Ensambles y chapa (queda de F9, ya cerrada):** relación tangente; arrastre
  libre de instancias con rótula, plana o paralela; sub-ensambles; instancias de
  STEP importados; pestaña en arista curva (el desarrollo no entiende conos);
  volver a doblar el desarrollo; dobladillo cerrado sin radio.
- ⬜ **Operaciones:** 🟨 desfasar/mover cara solo en caras planas; reemplazar cara
  solo hasta un plano; omitir instancias de patrón solo por diálogo (falta
  clic en el visor); recubrimiento tangente a la cara vecina; redondeo de cara
  en esquinas de tres redondeos y cantos vivos; redondeo completo general.
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

## 10. Laminador de filamento (`libs/slicer`, plan en `print3d/PLAN_SLICER.md`)

Motor de OrcaSlicer 2.4.2 como proceso aparte, embebido en el ejecutable. S0–S8
hechas (laminar dentro de Fabricar, vista previa, miniaturas, envío por
Moonraker, motor en CI para Linux; Windows y macOS escritos).
- 🧪 Comparar contra la GUI de Orca 2.4.2 con el mismo STL; probar el panel en la
  app abierta (la vista previa solo se vio en Chromium); ver las miniaturas en la
  pantalla de la K1.
- 🧪 Motor en Windows y macOS: los workflows están, falta correrlos (macOS sin
  runners propios).
- ⬜ «Preparar todo» no termina laminando (S3).

## 11. Resina (`libs/slicer/src/sla*.rs`, plan en `print3d/PLAN_SLA.md`)

Hecho: laminado por filas en paralelo, CTB v2 para la LD-002H (igual byte a byte
al de Lychee salvo el tiempo), revisión de islas/ventosas/huecos (con un venteo
por bolsillo), venteos, soportes automáticos editables, perfiles, pieza de
calibración, interfaz en Fabricar, MCP, y lo de los laminadores de pago (SLA9–SLA16,
2026-11-06): bordes en grises, compensación de medidas, gráfico de área por capa y
subida dinámica (CTB v3), soportes avanzados (punta según la normal, refuerzos,
ramas, balsa biselada, grosor por punto, pincel de zonas, laterales en partes
delgadas), orientación automática, vaciado con relleno y drenajes con tapón,
operaciones sobre capas (islas y huecos chicos, alivio de la balsa, atenuar el
interior, doble exposición) y nombre en relieve en la balsa.
- 🧪 En la LD-002H, sin imprimir: que liste y muestre un `.ctb` de Pinocchio
  (miniatura, tiempos), uno con grises y uno v3; cómo toma la doble exposición
  (capas repetidas a la misma z). Fijar el espejo con una pieza asimétrica.
  Imprimiendo: la pieza de calibración (exposición y compensaciones reales) y una
  pieza con y sin bordes en grises.
- 🧪 En la app real (el arnés no dibuja 3D): editar puntos, pincel de zonas,
  drenajes y el giro elegido en el visor.
- 🧪 CXDLP para la Halot-Sky: escritor hecho (igual byte a byte al de Lychee); falta
  la prueba con el usuario (`~/Escritorio/prueba-halot-sky.cxdlp`: la pantalla dice
  3.5 s = décimas o 35 s = segundos; impresa, la esquina dice el espejo) y elegirlo en
  «Impresora y resina». Hasta entonces el `.cxdlp` no se guarda.
- ⬜ Envío por WiFi a la Halot-Sky (investigar sin tocarla hasta que el usuario diga).
- ⬜ Alto de capa por tramos: todo el laminador de resina asume alto fijo; antes,
  ver si la LD-002H respeta una z distinta por capa.
- ⚠ Relleno de rejilla del vaciado: en piezas curvas deja bolsillos y pide más
  venteos (Benchy: 19); por eso columnas es el de fábrica.

## 12. Impresoras (`print3d/PLAN_IMPRESORAS.md`)

Regla: ninguna orden a una impresora sin pedido del usuario (leer sí).
- 🧪 Hechas en código, falta probarlas desde la app con cada impresora: K1
  rooteada por Moonraker (el envío ya se probó), K1 de fábrica por el protocolo
  de Creality (P2: solo leída; falta subir/imprimir), Flashforge AD5X (P3/P4),
  controles y cámara (P7), herramientas del MCP con aprobación (P6).
- ⬜ P5: encontrarlas solas en la red (mDNS, Creality, UDP de Flashforge).
- ⬜ P7 resto: configuración de Klipper desde la app, puesta en marcha y
  calibraciones guiadas, cola. CR-10 Max: falta grabar MainsailOS y rehacer su
  `printer.cfg`.
- ⬜ Multicolor de la AD5X (`PLAN_MULTICOLOR.md`, MC0–MC7): nada empezado, con
  preguntas abiertas para el usuario.

## 13. Conector MCP y chat (`apps/PLAN_MCP.md`)

M0, M2–M5, M7 y M8 hechos, más el chat dentro de la app (Claude Code o
proveedores compatibles con OpenAI), laminado, pieles e impresoras.
- ⏸ Pausado por decisión del usuario (2026-10-25): M6 (rig/animación, solo en
  vivo), M9, cancelar a mitad un comando largo y ahorro de tokens (la lista de
  herramientas pesa ~21k tokens y el chat la manda en cada paso).
- 🟨 M1: quedan comandos sin separar de `AppHandle` (se hacen por fase).

## 14. Android y distribución

- 🧪 APK arm64 con CAD (`android.yml`, manual): compila desde el 2026-10-29,
  nunca probado en un teléfono.
- ⬜ `PLAN_ANDROID.md`: A1 clave fija (la pone el usuario, va primero), A2–A7
  proyectos dentro de la app, importar/exportar/compartir, «Abrir con»; A8 táctil
  después de probarlo. Chat por HTTPS en Android (falta iniciar el verificador
  con JNI).
- 🧪 Actualizaciones automáticas firmadas (2026-10-25): el diálogo se probó con
  arnés; falta una actualización real de punta a punta en Windows y en el
  AppImage.
