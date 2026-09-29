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
- Alisado final de bordes (`ChartOptions::smooth_seams`): una cara de borde
  pasa a la carta vecina si acorta las costuras y la vecina la acepta
  (ángulo, aristas vivas). Costuras ~7 % más cortas, cobertura del atlas
  69,5 → 71,3 %, sin costo de tiempo apreciable.
- Probado y descartado: esconder costuras por oclusión ambiental (48 rayos
  por cara). Como costo extra del crecimiento casi no cambia el largo visible
  (9,34 → 9,22) y agrega islas; como peso en el alisado mejora ~1 % sobre el
  alisado por largo solo, y cuesta ~0,4 s.
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
- Por rasterizado, como xatlas: cada isla se dibuja en una grilla de 512
  celdas por lado (rasterizado conservador; un quad aporta sus dos
  triangulaciones para cubrirlo aunque no sea convexo) y se engorda el margen.
  De mayor a menor área, cada isla va al primer hueco libre de abajo a la
  izquierda probando los cuatro giros de 90° (gana el borde superior más
  bajo); solo si no cabe se agranda el cuadrado. Las islas chicas llenan los
  huecos de las grandes.
- Filas del atlas con menos celdas libres que la fila de la isla se saltan, y
  las filas candidatas se prueban en paralelo.
- La escala se ajusta (hasta 12 intentos) para que el lado final no pase de
  512 celdas: así el margen en celdas equivale al menos a `padding` texels.
- Cobertura media (5 modelos × 2000/8000 quads): skyline 50 % → 69,5 %;
  gonfoterio 44 % → 65 %. Tarda 0,1–0,8 s (antes < 0,1 s). Con 1024 celdas
  sube 1–2 puntos más pero tarda el triple. Ordenar por lado mayor en vez de
  área empeora (66,8 %).
- Banco: `cargo run --release -p uv-core --example unwrap_bench -- modelo.glb 2000 8000`.

## Fase 4 — Horneado ✅

- Por texel: triángulo destino (rasterizado), punto 3D y proyección al
  original → grupo, UV originales y marco tangente de ambos lados.
- Proyección como un horneado con jaula (`UvSurface::project`): rayo por la
  normal interpolada de la malla nueva hacia ambos lados, alcance = lado medio
  de la cara destino; gana el corte más cercano de un triángulo que mire hacia
  el mismo lado (los que miran al revés se atraviesan). Sin corte, el punto
  más cercano. El más cercano solo caía en otra pieza que pasa cerca (pecho
  entre las patas, base de los colmillos del gonfoterio) y dejaba cuñas de
  textura ajena. Cuesta ~0,3 s más en el gonfoterio.
- Color base y emisión se combinan con su factor en espacio lineal y se
  vuelven a sRGB. Metal/rugosidad y oclusión con sus factores.
- Normal: normal de sombreado del original (más su normal map) al espacio
  tangente nuevo; se hornea siempre, así la malla liviana conserva el relieve
  de la original. Convención glTF: T hacia +u, B = w (N × T) hacia −v; la
  exportación escribe esas mismas tangentes (`TANGENT`).
- Sin UV de origen (escaneo en STL) se hornea igual desde la geometría
  (`material_surface`, UV en cero): el relieve de la malla densa como normal
  map, los factores por material y los colores de vértice (`COLOR_0`,
  `UvPart::colors`, multiplicados al color base en espacio lineal). Cabeza
  escaneada de 126 k triángulos a 4000 quads: la oreja recupera sus pliegues.
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

- `pinocchio-spatial`: un rayo con una componente −0 descarta cajas del BVH
  que tocan su origen (0 · −inf = NaN cae del lado equivocado). `project` lo
  esquiva pasando −0 a +0. Arreglarlo en el BVH cambia los pesos de
  `heat_diffusion` en ~1e-5 (su test de invariancia de escala tiene
  tolerancia 1e-5): coordinarlo con quien trabaje en el rig.
- Desplegar también la malla original (sin retopología).
- Importar PLY (hoy solo se exporta): es el formato típico de escaneos con
  color por vértice.
- Costuras alineadas a las costuras viejas del original. (Esconderlas por
  oclusión no rindió, ver fase 1.)

## Verificación

```text
cargo test -p uv-core
cargo run --release -p uv-core --example retopo_uv -- modelo.glb 5000 salida.glb [--unwrap] [--checker] [--size 2048]
f3d salida.glb --output render.png
```

Validar el GLB con gltf-validator (npm, `validateBytes`).
