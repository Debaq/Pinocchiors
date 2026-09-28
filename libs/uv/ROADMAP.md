# UV — la piel de la malla

Crate `uv-core` (Rust puro, sin C++: debe compilar a wasm). Flujo de la app:

```
Importar → Reparar → Retopología (+ traspaso UV) → UV / Piel → Esqueleto → Animar → Exportar
```

- **Retopología:** si el modelo trae UV, se trasladan a la malla nueva
  (fase 0). Es rápido y conserva el atlas y los materiales originales, pero
  donde una cara nueva cruza una costura vieja la textura se estira.
- **UV / Piel:** paso propio, haya o no UV. Despliega la malla de quads en
  islas nuevas cortadas por sus aristas y hornea encima las texturas del
  original. Cada texel lee de un solo punto del original: sin astillas ni
  costuras. Se puede volver a las UV trasladadas.

## Módulos

| Módulo | Qué hace |
|---|---|
| `surface` | `UvSurface`: malla de referencia con UV y normales. Vecindad que se corta en las costuras, BVH, baricéntricas, UV y marco tangente en un punto. |
| `transfer` | `transfer_uvs`: UV por esquina para otra malla; cada esquina busca desde el centro de su cara sin cruzar costuras. |
| `geometry` | `PolyMesh`: normales de Newell, áreas, vecindad por aristas, abanicos. |
| `charts` | Segmentación en islas (crecimiento simultáneo + relajación, ver abajo); garantiza discos. |
| `param` | LSCM + ARAP por isla; métricas de inversión y estiramiento L2. |
| `pack` | Escala a densidad de texel uniforme, caja mínima, skyline con giro de 90°. |
| `unwrap` | Orquesta: islas → parametrización (en paralelo) → partir las malas → empaquetar. |
| `tangent` | `corner_frames`: normales y tangentes por esquina, las mismas al hornear y al exportar. |
| `bake` | `bake`: rasteriza el mapa nuevo, busca el punto del original por texel y evalúa canales; dilata los bordes. |
| `skin` | Nivel escena: `scene_surface`, `transferred_skin`, `unwrapped_skin` (hornea color, metal/rugosidad, oclusión, emisión y normal), `skin_scene` (malla + piel → `Scene`), `checker_texture`. |

## Fase 0 — Traspaso de UV en la retopología ✅

- Dos triángulos son vecinos solo si la arista tiene las mismas UV a ambos
  lados: una costura corta la vecindad aunque no separe islas (cilindro).
- Si el punto más cercano real queda al otro lado de una costura, se
  extrapola el mapa afín del lado del centro de la cara.
- Gonfoterio (500 k triángulos, 448 islas, 5000 quads): 2,9 s, 35 % de caras
  cruzan costuras; astillas finas en esas caras. Las corrige la fase 4.
- Solo cuenta como cruce una esquina que pasa la costura por más de un décimo
  de su cara (antes contaba cualquier vértice apenas corrido de ella).
- Con `RemeshConfig::preserve_seams` los quads siguen las costuras de las
  islas gruesas (ver `libs/quadriflow/ROADMAP.md`): Suzanne 5,8 % → 1,7 %,
  gonfoterio 35 % → 22,6 %.

## Fase 1 — Islas ✅

- Crecimiento simultáneo desde semillas con cola de prioridad global (estilo
  D-Charts / xatlas). Costo = `1 − n·n̄` + 0,1 · redondez (fracción del
  perímetro no compartido). Límites duros: desviación ≤ `max_angle` (55°) y
  nunca cruzar aristas vivas (diedro > `sharp_angle`, 70°).
- Caras que nadie acepta siembran islas nuevas (la más grande primero).
- Relajación (4 rondas): semilla = cara más interior (BFS desde el borde) y se
  vuelve a crecer.
- Discos: χ = 1, un lazo de borde, conexa. Si no, se parte en dos con
  Dijkstra por centroides desde dos caras lejanas.

## Fase 2 — Parametrización ✅

- LSCM (Cauchy-Riemann por triángulo, peso = área, 2 vértices fijos a su
  distancia 3D) con CG precondicionado de `pinocchio-sparse`.
- ARAP local/global (10 iteraciones, cotangentes acotadas a [1e-3, 1e3]); se
  descarta si agrega inversiones.
- Isla con triángulos invertidos o estiramiento L2 > 1,25: se parte y se
  repite (hasta 10 rondas).

## Fase 3 — Empaquetado ✅

- Cada isla a su área 3D (densidad de texel uniforme), girada a la caja de
  menor área (aristas de la cápsula convexa).
- Skyline de abajo a la izquierda, cajas de mayor a menor, probando 90°;
  varios anchos de franja y se queda el cuadrado más chico. Margen en px del
  tamaño de textura destino (se itera porque depende del lado final).
- Gonfoterio: 74 islas, 44 % del atlas cubierto.

## Fase 4 — Horneado ✅

- Por texel: triángulo destino (rasterizado), punto 3D, punto más cercano
  del original → grupo, UV originales y marco tangente de ambos lados.
- Color base y emisión se combinan con su factor en espacio lineal y se
  vuelven a sRGB. Metal/rugosidad y oclusión con sus factores.
- Normal: normal de sombreado del original (más su normal map) al espacio
  tangente nuevo; se hornea siempre, así la malla liviana conserva el relieve
  de la original. Convención glTF: T hacia +u, B = w (N × T) hacia −v; la
  exportación escribe esas mismas tangentes (`TANGENT`).
- Dilatación de `2 × padding` texels y el resto del fondo con el color medio.
- Gonfoterio: sin astillas, arrugas de la trompa conservadas, gltf-validator
  0 errores / 0 avisos.

## Fase 5 — App ✅

- Paso "UV / Piel" (`apps/web/src/components/steps/UvStep.tsx`): métricas del
  mapa, tamaño de textura, margen, curvatura por isla, desplegar y hornear,
  volver a las UV trasladadas, vista con textura / tablero / sin textura y
  atlas 2D.
- Backend: `AppState::quad_skin`; comandos `get_uv_info`, `run_uv_unwrap`,
  `restore_transferred_uvs`, `get_uv_texture`, `get_uv_layout`;
  `get_quad_mesh_data` duplica vértices en las costuras e incluye UV.
- La exportación con retopología usa la piel (materiales, texturas,
  tangentes) y los pesos del rig siguen al vértice de quads de origen.

## Pendiente

- Empaquetado por rasterizado (como xatlas) para pasar de ~45 % a ~70 % de
  uso del atlas.
- Horneado con rayos por la normal además del punto más cercano: en zonas
  finas o muy juntas (colmillo contra pata) el más cercano puede ser la otra
  pieza; se ve una rayita clara en el gonfoterio.
- Normal map de alta a baja sin UV de origen: `UvSurface` exige UV; un STL
  denso podría aportar su relieve igual.
- Desplegar también la malla original (sin retopología).
- Costuras preferidas en zonas poco visibles (oclusión aproximada) y
  alineadas a las costuras viejas.

## Verificación

```text
cargo test -p uv-core
cargo run --release -p uv-core --example retopo_uv -- modelo.glb 5000 salida.glb [--unwrap] [--checker] [--size 2048]
f3d salida.glb --output render.png
```

Validar el GLB con gltf-validator (npm, `validateBytes`).
