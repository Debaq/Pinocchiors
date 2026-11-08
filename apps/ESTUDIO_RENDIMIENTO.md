# Estudio: qué hace lento el frontend y qué conviene pasar a Rust

2026-11-08. Revisión de código (sin perfilar) de apps/web/src y del puente con
apps/desktop/src. Pedido del usuario: «el lado frontend es mucho más lento».

## Conclusión

El frontend **ya delega casi todo lo pesado a Rust**: mallas, UV, remallado,
LOD, rayos, laminado, nubes de escaneo. No hay trabajo de píxeles en JS ni
booleanas en TS. La lentitud viene sobre todo de dos lugares:

1. **El puente con el diseño CAD**: cada cambio dispara 3–5 llamadas y cada
   una vuelve a copiar y serializar el documento entero (y en el escaneo, a
   rearmar referencias) aunque nada haya cambiado.
2. **Unos pocos bucles en JS sobre todos los vértices** en el visor
   (Viewer3D.ts): pincel de pesos, posar huesos, primer uso del pincel de
   suavizado/máscara, selección en nubes grandes.

Si la lentitud es general (no en una herramienta), hay que medir con el
perfilador del navegador: App.tsx (~10 k líneas) y DesignStep.tsx (~11 k)
pueden tener efectos reactivos de más; eso no se revisó.

## Prioridades (ganancia / esfuerzo)

### A. Puente CAD (Rust + un poco de TS) — lo primero

1. **Evaluar sin serializar** (alto, fácil). `Document::geometry_hash`
   (libs/cad/model/src/document.rs:271) clona el documento y lo pasa a JSON en
   cada llamada, aun con la caché acertando; `evaluate()` (apps/desktop/src/
   cad.rs:303) arma un `CadResult` completo que 27 comandos descartan
   (`face_ref`, `edge_ref`, `feature_faces`, `mesh`, medir, planos…). Hacer:
   contador de versión del documento como clave, y `ensure_evaluated` aparte de
   `build_result`.
2. **Una sola llamada por cambio** (alto, fácil). cad.ts:4509-4516 hace
   `set_document` → `cad_mesh` → `cad_tool_mesh` en serie. Fusionar en una
   respuesta binaria, o al menos en paralelo, y guardar la malla de la vista
   por versión (hoy se re-tesela con OCCT en cada `cad_mesh`).
3. **Caras/aristas en lote** (alto si se eligen muchas, fácil). Un invoke por
   cara/arista elegida (designActions.ts, CadView.tsx:2593 hace N+1): agregar
   `cad_face_refs`/`cad_edge_refs` con una lista.
4. **Pasar el mouse por el árbol** (medio, fácil). CadView.tsx:892 llama al
   backend en cada fila sin pausa: mandar la operación de cada cara junto con
   la malla y resolver el resaltado en el front.
5. **Ensambles** (alto en ensambles, medio). Dos comandos repiten el mismo
   cálculo y el deslizador de explosión re-tesela todo en cada movimiento: la
   explosión es solo mover piezas, se puede hacer en el visor.
6. **Planos 2D** (medio-alto, medio). `cad_drawing` recalcula y manda en JSON
   todas las vistas aunque cambie una: caché por vista y binario.

### B. Bucles en el visor (Viewer3D.ts)

7. **Vecinos para pincel de suavizado/máscara** (alto en primer uso, fácil):
   `neighborsOf` (~2013) usa un `Set` por vértice y claves de texto: segundos y
   mucha memoria con 500 k vértices. Pasar a Rust (comando binario) o a
   arreglos tipados.
8. **Influencia estimada de huesos** (medio, fácil): `estimateInfluence`
   (boneInfluence.ts:60) es vértices × huesos; a Rust con rayon.
9. **Pincel de pesos** (medio, medio): cada pincelada recorre todos los
   vértices (~1790); usar el BVH que ya está (three-mesh-bvh) o una rejilla.
   No necesita Rust.
10. **Posar un hueso** (alto al posar mallas grandes, medio): `updatePose`
    (~2937) deforma toda la malla en CPU en cada movimiento; la solución es
    skinning en GPU (ya existe para animar), no Rust.
11. **Nubes de millones de puntos** (medio-alto, medio): selección y recoloreo
    completos en cada pincelada (~4339-4600) y la selección viaja como JSON
    (scan_cloud.rs `indices: Vec<u32>`): recoloreo incremental y binario.

### C. Datos que viajan como JSON grande

12. **Zona de la piel** (medio, fácil): print_skin.rs `ZoneInfo` devuelve todas
    las caras (~1 MB con 163 k) en cada trazo: devolver resumen + máscara binaria.
13. **Texturas** (medio, fácil): se vuelven a bajar todas en cada carga y en
    cada guardado de GIMP (structure.rs:456): pedir solo las que cambiaron.
14. **Bytes como array JSON** (bajo-medio, fácil): `cad_write_bytes`
    (DrawingView.tsx:701) manda un PNG como lista de números (4× más grande);
    copiar el patrón de `write_render_image`.
15. Menores: `drawingShade.shadeView` (spread de arrays grandes), `recordableArgs`
    (App.tsx:538 clona argumentos con JSON), `nearestNormal` y snap a vértice
    lineales (usar BVH), cajas en placement.ts (ya hay placement.rs).

## Lo que está bien (no tocar)

Mallas grandes en binario (`Response`), decodificación sin copias
(buffers.ts, lod.ts), bucle de render a demanda con calidad adaptativa,
arrastre de sketches por deltas, cola de envíos que descarta intermedios,
progreso por canales pequeños.

## Orden sugerido

A1 → A2 → A3 → A4 (el diseño CAD se siente más ágil en todo) → B7, B8 → C12,
C13 → A5, A6 → B9, B10, B11 según qué use el usuario → el resto si hay quejas.
