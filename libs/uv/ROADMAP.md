# UV — la piel de la malla

Crate `uv-core` (Rust puro, sin C++: debe compilar a wasm). Flujo de la app:

```
Importar → Reparar → Retopología (+ traspaso UV) → UV / Piel → Esqueleto → Animar → Exportar
```

- **Retopología:** si el modelo trae UV, se trasladan a la malla nueva
  (fase 0). Es rápido y conserva el atlas original, pero no es exacto donde
  una cara nueva cruza una costura vieja.
- **UV / Piel:** paso propio, haya o no UV. Sin UV, despliega la malla (una
  malla de quads limpia se corta mejor por sus bucles de aristas). Con UV
  trasladadas, permite quedarse con ellas o desplegar de nuevo y hornear la
  textura original sobre el mapa nuevo, que es la forma exacta.

## Fase 0 — Traspaso de UV en la retopología ✅

- `UvSurface`: triángulos con UV, soldados por (grupo, posición, UV). Dos
  triángulos son vecinos solo si la arista tiene las mismas UV a ambos lados,
  así una costura corta la vecindad aunque no separe islas (cilindro).
- `transfer_uvs::<N>(surface, posiciones, caras)`: cada esquina toma la UV del
  triángulo más cercano alcanzable desde el centro de la cara sin cruzar
  costuras; si el más cercano real está al otro lado, extrapola el mapa afín
  del lado del centro. Devuelve UV por esquina, grupo (material) por cara y
  cuántas caras cruzan costuras.
- App: `AppState::quad_uvs`; `quad_mesh_to_scene` agrupa por material y
  duplica vértices en las costuras; los pesos del rig siguen al vértice de
  quads de origen.
- Ejemplo: `cargo run --release -p uv-core --example retopo_uv -- modelo.glb 5000 salida.glb`.
- Gonfoterio (500 k triángulos, 448 islas, 5000 quads): 2.9 s, 39 % de caras
  cruzan costuras. La textura sigue bien; quedan astillas finas donde la UV
  extrapolada cae fuera de la isla. Límite propio del traspaso directo: lo
  corrige la fase 4.

## Fase 1 — Cartas (corte en discos)

- Semillas por aristas vivas y crecimiento de regiones por normal
  (planaridad + compacidad, estilo xatlas / D-charts).
- En mallas de quads: cortar por bucles de aristas; preferir costuras en
  zonas poco visibles (concavidades, oclusión ambiental aproximada).
- Cada carta debe ser un disco: cortar asas (género > 0) con el camino más
  corto entre bordes.

## Fase 2 — Parametrización

- LSCM por carta (2 vértices fijos, mínimos cuadrados dispersos con
  `pinocchio-sparse`), luego refinado ARAP local/global.
- Métrica de estiramiento (L2 de Sander) y detección de triángulos
  invertidos; si una carta supera el umbral, dividirla y repetir.

## Fase 3 — Empaquetado

- Escalar cartas a densidad de texel uniforme; rotar a caja mínima.
- Skyline/maxrects con margen en píxeles según el tamaño de textura destino
  (512–4096), rotaciones de 90°.

## Fase 4 — Horneado

- Rasterizar cada triángulo en el espacio UV nuevo; por texel, punto más
  cercano (o rayo por la normal) sobre la malla original → UV viejas →
  muestrear textura. Canales: base color, ORM, emisiva, normal (reorientada a
  la base tangente nueva).
- Dilatar bordes de islas (margen) para evitar costuras con mipmaps.
- Opcional: normal map de alta (original) a baja (retopología).
- Exportar tangentes (MikkTSpace) cuando el material tiene normal map:
  gltf-validator avisa `MESH_PRIMITIVE_GENERATED_TANGENT_SPACE` en la fase 0.

## Fase 5 — App

- Paso "UV / Piel" en `pipeline.ts` entre retopología y esqueleto.
- Visor: textura de tablero para ver distorsión, vista 2D del atlas,
  resaltar costuras. Opciones: conservar trasladadas / desplegar + hornear,
  tamaño de textura, margen.
- `geometry_changed()` descarta también las UV (ya descarta `quad_uvs`).
- Verificación: exportar GLB → gltf-validator + render con f3d.
