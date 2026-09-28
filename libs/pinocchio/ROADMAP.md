# Pinocchio-RS Roadmap

Auto-rigging de personajes 3D en Rust puro, basado en el paper original de Pinocchio (Baran & Popović, 2007).

---

## Arquitectura

```
┌─────────────────────────────────────────────────────────────┐
│                      pinocchio-core                          │
│                     API: autorig()                           │
└─────────────────────────────────────────────────────────────┘
       │              │              │              │
       ▼              ▼              ▼              ▼
┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐
│   mesh    │  │ skeleton  │  │ embedding │  │attachment │
│ half-edge │  │  presets  │  │  fitting  │  │   heat    │
│   + I/O   │  │  + trait  │  │           │  │ diffusion │
└───────────┘  └───────────┘  └───────────┘  └───────────┘
       │              │              │              │
       ▼              ▼              ▼              ▼
┌───────────┐  ┌───────────┐  ┌───────────┐  ┌───────────┐
│   math    │  │  spatial  │  │   graph   │  │  sparse   │
│  vectors  │  │   octree  │  │  dijkstra │  │  solver   │
│  nalgebra │  │    BVH    │  │ petgraph  │  │   sprs    │
└───────────┘  └───────────┘  └───────────┘  └───────────┘
```

---

## Crates

### pinocchio-math
Álgebra lineal básica sobre `nalgebra`.

| Tipo | Descripción |
|------|-------------|
| `Vector3` | Vector 3D (f64) |
| `Matrix3`, `Matrix4` | Matrices |
| `Quaternion` | Rotaciones |
| `Transform` | TRS transforms |
| `BoundingBox` | AABB |

### pinocchio-sparse
Matrices sparse y solvers.

| Tipo | Descripción |
|------|-------------|
| `SparseMatrix` | Wrapper sobre `sprs::CsMat` |
| `solve_gauss_seidel()` | Solver iterativo para sistemas Laplacianos |
| `solve_with_identity()` | Resuelve `(I + λL)x = b` |

### pinocchio-mesh
Malla half-edge con I/O.

| Feature | Descripción |
|---------|-------------|
| `Mesh` | Estructura half-edge (vértices, aristas, caras) |
| `MeshVertex` | Vértice con posición, normal, aristas |
| `MeshEdge` | Half-edge con twin, next, face |
| `load_obj()` | Carga OBJ con `tobj` |
| `load_glb()` | Carga GLB/glTF con `gltf` |
| `load_mesh()` | Auto-detecta formato |
| `decimate()` | Reducción de malla (edge collapse) |

**Morph Targets (Blend Shapes):**

| Tipo | Descripción |
|------|-------------|
| `MorphDelta` | Delta de posición + normal opcional |
| `MorphTarget` | Nombre + deltas sparse por vértice |
| `MeshWithMorphs` | Malla base + lista de morph targets |
| `apply_morphs(weights)` | Aplica blend shapes con pesos |

**Feature `converter`** (opcional):
```toml
pinocchio-mesh = { version = "0.1", features = ["converter"] }
```
- `load_stl()` — Carga STL via `converter-stl`
- `scene_to_mesh()` — Convierte `converter_scene::Scene` a `Mesh`
- Usa `converter-gltf-io` internamente para GLB

### pinocchio-skeleton
Esqueletos y presets predefinidos.

| Tipo | Descripción |
|------|-------------|
| `Bone` | Hueso: nombre, posición, padre, is_leaf |
| `Skeleton` (trait) | Interfaz para esqueletos |
| `BasicSkeleton` | Implementación genérica |

**Presets incluidos:**

| Preset | Huesos | Descripción |
|--------|--------|-------------|
| `HumanSkeleton` | 20 | Bípedo humanoide (pelvis, spine, brazos, piernas) |
| `QuadSkeleton` | 18 | Cuadrúpedo genérico (perro, gato, lobo) |
| `HorseSkeleton` | 18 | Caballo (cuello/piernas más largas) |
| `CentaurSkeleton` | 27 | Híbrido humano + caballo |
| `BirdSkeleton` | 24 | Ave (alas 4-segmentos, cola, patas) |
| `SpiderSkeleton` | 26 | Arácnido (8 patas, pedipalpos) |
| `SerpentSkeleton` | 16* | Serpiente/dragón (*configurable con `with_segments(n)`) |
| `MechSkeleton` | 28 | Robot/mech genérico |

**Carga/guardado de esqueletos:**

| Función | Descripción |
|---------|-------------|
| `load_skeleton_json()` | Carga esqueleto desde JSON string |
| `save_skeleton_json()` | Exporta esqueleto a JSON string |
| `load_skeleton_from_file()` | Carga desde archivo .json |
| `save_skeleton_to_file()` | Guarda a archivo .json |

**Feature `converter`** (opcional):
```toml
pinocchio-skeleton = { version = "0.1", features = ["converter"] }
```
- `from_scene_skeleton()` — Convierte `converter_scene::Skeleton` a `BasicSkeleton`

**Métodos del trait `Skeleton`:**
- `num_bones()`, `get_bone()`, `bones()`
- `root()`, `get_children()`, `get_parent()`
- `is_leaf()`, `get_depth()`
- `scale()`, `translate()`, `scaled()`
- `get_graph_edges()`

### pinocchio-spatial
Estructuras espaciales para consultas geométricas.

| Tipo | Descripción |
|------|-------------|
| `Octree` | Subdivisión espacial adaptativa |
| `BVH` | Bounding Volume Hierarchy para raycast/closest point |
| `DistanceField` | Campo de distancias discreto (grid 3D) |

Optimizaciones:
- Paralelización con `rayon`
- O(log n) para consultas de distancia

### pinocchio-graph
Grafos y algoritmos sobre `petgraph`.

| Función | Descripción |
|---------|-------------|
| `shortest_path()` | Dijkstra para caminos mínimos |
| `build_bone_graph()` | Grafo del esqueleto |

### pinocchio-attachment
Cálculo de pesos de skinning via heat diffusion.

| Función | Descripción |
|---------|-------------|
| `compute_weights()` | Heat diffusion (Baran & Popović) |
| Laplaciano cotangente | Matriz `L` de la malla (paralelizado con rayon) |
| Normalización | Pesos suman 1 por vértice |

**Simetría:**

| Tipo/Función | Descripción |
|--------------|-------------|
| `SymmetryAxis` | Eje de simetría (X, Y, Z) |
| `SymmetryPair` | Par de vértices simétricos |
| `SymmetryMap::detect()` | Detecta simetría en una malla |
| `symmetrize_weights()` | Promedia pesos con huesos simétricos |
| `mirror_weights()` | Copia pesos de un lado al otro |

### pinocchio-embedding
Ajuste del esqueleto dentro de la malla.

| Función | Descripción |
|---------|-------------|
| `fit_skeleton()` | Ajuste automático por extremidades con búsqueda de orientación (ver fase 8) |
| `JointCentering` | Centra articulaciones en la sección del miembro (edición manual) |
| `chain_embed()` | Embedding por cadenas anterior (se conserva, ya no lo usa `autorig`) |
| Superficie medial | Aproximación del eje medial |

**pinocchio-skeleton:** `mirror_pairs`, `symmetry_plane`, `reflect` para editar en espejo.
**pinocchio-attachment:** `attach_detached_parts` hace rígidas las piezas sueltas sin huesos.
**pinocchio-core:** `fit_to_mesh` (ajuste sin pesos, para revisar) y `SkeletonFit::{Auto, None, Exact}`.

### pinocchio-core
API principal.

```rust
use pinocchio_core::{autorig, PinocchioConfig};
use pinocchio_mesh::load_mesh;
use pinocchio_skeleton::HumanSkeleton;

let mesh = load_mesh("character.glb")?;
let skeleton = HumanSkeleton::new();
let result = autorig(&mesh, &skeleton, Some(PinocchioConfig::default()))?;

// result.bone_positions — posiciones finales de huesos
// result.attachment — pesos de skinning
// result.export_weights(4) — exportar con max 4 influences
```

**`PinocchioConfig`:**
- `fast()` — Resolución baja, rápido
- `default()` — Balance calidad/velocidad
- `high_quality()` — Máxima precisión
- `.with_diffusion_weight(f64)` — Peso del heat diffusion
- `.with_max_influences(usize)` — Max huesos por vértice

---

## Estado Actual

| Crate | Estado | Tests |
|-------|--------|-------|
| `pinocchio-math` | ✅ Completo | 11 |
| `pinocchio-sparse` | ✅ Completo | 7 |
| `pinocchio-mesh` | ✅ Completo | 18 (con converter y morph targets) |
| `pinocchio-skeleton` | ✅ Completo | 19 (con converter) |
| `pinocchio-spatial` | ✅ Completo | 12 |
| `pinocchio-graph` | ✅ Completo | 6 |
| `pinocchio-attachment` | ✅ Completo | 15 (con simetría) |
| `pinocchio-embedding` | ✅ Completo | 12 |
| `pinocchio-core` | ✅ Completo | 6 |

**Total: 170+ tests, 260+ en workspace completo**

### Optimizaciones Implementadas

- [x] BVH para consultas de distancia O(log n)
- [x] Paralelización con `rayon` (campo de distancias, Laplaciano cotangente)
- [x] Decimación automática para mallas >100K triángulos
- [x] Solver iterativo Gauss-Seidel para sistemas Laplacianos
- [x] Integración con `converter-scene` via feature flag
- [x] Heat diffusion paralelo por hueso

---

## Fases Pendientes

### Fase 1: Validación

- [ ] Comparar output con Pinocchio C++ original
- [ ] Suite de mallas de prueba estándar
- [ ] Métricas de calidad de skinning
- [ ] Benchmarks con `criterion`

### Fase 2: Más Presets de Esqueleto ✅

- [x] `BirdSkeleton` — Aves (24 huesos: alas 4-segmentos, cola, patas)
- [x] `SpiderSkeleton` — 26 huesos (8 patas simplificadas, pedipalpos)
- [x] `SerpentSkeleton` — Serpiente/dragón (configurable, default 16 huesos)
- [x] `MechSkeleton` — Robot/mech genérico (28 huesos)
- [x] Carga de esqueletos desde JSON (`load_skeleton_json`, `save_skeleton_json`)
- [x] Carga de esqueletos desde glTF (`from_scene_skeleton` con feature `converter`)

### Fase 3: Mejoras de Algoritmos ✅

- [x] Paralelizar heat diffusion por hueso
- [x] Paralelizar construcción de Laplaciano cotangente (`build_laplacian` con rayon)
- [x] Soporte para morph targets (`MorphDelta`, `MorphTarget`, `MeshWithMorphs`)
- [x] Constraints de simetría (`SymmetryMap`, `symmetrize_weights`, `mirror_weights`)

### Fase 4: Exportación

- [ ] Exportar a GLB con skin (via converter)
- [ ] Exportar a FBX
- [ ] Formato binario propio para cache

### Fase 5: WASM y Web

- [ ] Configurar `wasm-bindgen`
- [ ] API simplificada para web
- [ ] Ejemplo con Three.js
- [ ] Worker threads para no bloquear UI

### Fase 6: GPU

- [ ] `wgpu` para campo de distancias
- [ ] Compute shaders para heat diffusion
- [ ] LOD automático

### Fase 8: Ajuste de esqueleto y pesos ✅ (2026-09-28)

El embedding por cadenas solo escalaba la plantilla a la caja de la malla y
repartía las cadenas por cercanía a la plantilla: en el gonfoterio dejaba
pecho, cuello y cabeza en un punto y cruzaba las patas.

**Ajuste automático (`fit.rs`):**
- Extremidades: Dijkstra desde la celda más profunda con costo `longitud / d²`;
  se extrae la rama que más sobresale (distancia a su unión menos el radio
  medial máximo en el camino) y se cubre el tubo alrededor de su camino (así la
  celda vecina de la misma pata no es otra extremidad).
- Orientación: 8 giros de la plantilla (Y o Z arriba), con sesgo a favor de la
  estándar (Y arriba, mirando a +Z) para no invertir izquierda/derecha en
  cuerpos casi simétricos.
- Asignación húngara de extremos de la plantilla (hojas, y la raíz de una
  cadena) a extremidades. Costo: distancia; altura solo para los extremos que
  tocan el suelo (patas contra colmillos); dirección desde el centro;
  prominencia; cabeza gruesa / cola fina (por nombre, grosor medido en el
  tercio de la rama junto a la punta). Sin asignar cuesta 0,6.
- Colocación: bifurcaciones en el promedio de los puntos donde sus
  extremidades entran al tronco (radio ≥ 75 % del tronco), sin la rama
  dominante (la columna desde la pelvis). Hojas en la celda más avanzada en la
  dirección del hueso (dedos del pie, no el talón). Cadenas intermedias con las
  proporciones de la plantilla; el primer tramo se ancla a la proyección de la
  plantilla.
- Resolución mínima 96 celdas (colas finas); el ajuste usa la malla completa
  aunque los pesos se calculen en la decimada.
- Tests con personajes sintéticos (cápsulas + marching tetrahedra, en
  `embedding/tests/characters`): humano, elefante con trompa y cola, girado 90°
  y con Z arriba. Todas las articulaciones a menos de 6 % del tamaño.
- Gonfoterio (500 k triángulos): orientación, 4 patas, cabeza y cola correctas;
  colmillos, trompa y orejas quedan como extremidades sin hueso.

**Pesos:** piezas sueltas sin huesos adentro (colmillos, ojos) toman los pesos
del punto del cuerpo más cercano y se mueven rígidas (antes el colmillo seguía
a la pata delantera).

**Edición manual (app):** "Ajustar automáticamente" muestra el esqueleto para
revisar; mover articulaciones con espejo (`hand_l` ↔ `hand_r`, `paw_fl` ↔
`paw_fr`, `.L/.R`, `Left/Right`); centrar la seleccionada o todas en la
sección del miembro; un esqueleto ajustado o editado se usa tal cual
(`SkeletonFit::Exact`) al calcular los pesos. "Probar la pose": con Rotar se
gira una articulación y la malla se dobla con los pesos (skinning lineal en el
visor).

**Banco:** `cargo run --release -p pinocchio-core --example rig_view -- modelo.glb quad salida [--bend hueso grados] [--extremities]`
deja `salida_rest.glb` (malla coloreada por pesos + huesos) y
`salida_pose.glb`; imprime qué hueso domina cuántos vértices.

**Pendiente:**
- [ ] Raíz de extremidades finas en cuerpos gordos: la base de la cola queda
      dentro de la grupa y la cola domina parte del lomo. Probé anclar el primer
      tramo donde el radio salta (1,6×) y rompía piernas humanas (los cambios de
      radio entre pie y canilla también saltan).
- [ ] Pesos en cuerpos gruesos: el calor por "hueso visible más cercano" deja
      la columna sin vértices en la superficie del tronco (los dominan patas y
      cuello). Evaluar difusión con peso por distancia al eje medial o
      voxelización (Dionne & de Lasa, "geodesic voxel binding").
- [ ] Pintar pesos a mano (pincel sumar/restar/suavizar por hueso, espejo).
- [ ] Pose de prueba encadenada (varias articulaciones a la vez).
- [ ] Apéndices de la fase 7 para las extremidades sin hueso (trompa, orejas).

### Fase 7: Esqueletos por forma de cuerpo + variantes (acordado 2026-09-27)

Los presets actuales cubren un animal por forma y les faltan apéndices: con
`QuadSkeleton` un elefante queda con la trompa rígida (solo tiene cuello,
cabeza, cola de 2 huesos y 4 patas). En vez de un preset cerrado por especie,
construir el esqueleto como **forma base + apéndices** con cantidad de
segmentos configurable (como `SerpentSkeleton::with_segments`), y ofrecer
variantes con nombre ya armadas para la UI.

**Formas base** (existentes y nuevas):

| Forma | Estado | Cubre |
|---|---|---|
| Bípedo | existe (`Human`) | humanoides, primates |
| Bípedo digitígrado con cola | nueva | dinosaurios, aves no voladoras, canguro |
| Cuadrúpedo | existe (`Quad`, `Horse`) | perro, felino, caballo, elefante |
| Radial | nueva | pulpo, calamar, medusa, estrella de mar: cabeza/manto + N cadenas largas desde el centro |
| Eje horizontal con aletas | nueva | pez, tiburón, delfín: columna + aletas dorsal, pectorales, pélvicas, caudal |
| Artrópodo | existe (`Spider`) | ampliar a N patas: insectos (6), arañas (8), cangrejo/escorpión (+ pinzas) |
| Cadena | existe (`Serpent`) | serpiente, gusano, anguila |
| Árbol libre | nueva | plantas, cuerdas, props: cadenas ramificadas sin anatomía |
| Mecánico | existe (`Mech`) | robots |

**Apéndices** enganchables a cualquier forma, con segmentos configurables:
trompa (cadena desde la cabeza), cola larga, alas (cadena tipo `Bird`),
aletas, tentáculos, orejas móviles, mandíbula, cuello largo, pinzas, cuernos
fijos.

**Variantes con nombre** (forma + apéndices):

| Variante | Receta |
|---|---|
| Elefante | cuadrúpedo + trompa (8-12 segmentos) + orejas + colmillos fijos |
| Jirafa | cuadrúpedo + cuello largo (5-7 segmentos) |
| Dragón | cuadrúpedo + alas + cola larga + cuello largo |
| Pulpo | radial con 8 tentáculos |
| Calamar | radial con 8 brazos + 2 tentáculos largos + aletas del manto |
| Pez | eje horizontal con aletas |
| Delfín / ballena | eje horizontal, aleta caudal horizontal |
| Cangrejo | artrópodo 8 patas + 2 pinzas |
| Insecto | artrópodo 6 patas + alas + antenas |
| T-rex | bípedo digitígrado + cola larga + brazos cortos |

**Pendientes:**
- [ ] Modelo de datos: forma base + lista de apéndices (anclaje, segmentos, simetría) → `BasicSkeleton`
- [ ] Formas nuevas: radial, eje horizontal con aletas, bípedo digitígrado, árbol libre
- [ ] Apéndices: trompa, tentáculo, aleta, ala, cola, cuello, oreja, pinza
- [ ] Variantes con nombre en `list_skeleton_presets` y en la UI
- [ ] Auto-ajuste de cadenas (trompa, tentáculos, colas) siguiendo el eje medial de la malla: más fácil que las extremidades articuladas
- [ ] Pesos: las cadenas largas y delgadas necesitan más segmentos para no quebrarse al doblar; validar el heat diffusion en tentáculos

---

## Dependencias

| Crate | Versión | Uso |
|-------|---------|-----|
| `nalgebra` | 0.33 | Álgebra lineal |
| `sprs` | 0.11 | Matrices sparse |
| `petgraph` | 0.7 | Grafos |
| `tobj` | 4.0 | OBJ I/O |
| `gltf` | 1.4 | GLB/glTF I/O |
| `rayon` | 1.10 | Paralelismo |

### Opcionales (feature `converter`)

| Crate | Uso |
|-------|-----|
| `converter-scene` | Representación intermedia |
| `converter-gltf-io` | Import GLB mejorado |
| `converter-stl` | Import/export STL |

---

## Compatibilidad

- **Rust Edition:** 2024
- **MSRV:** 1.85
- **Targets:** `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `wasm32-unknown-unknown`

---

## Referencias

- Baran, I., & Popović, J. (2007). *Automatic rigging and animation of 3D characters*. ACM SIGGRAPH.
- [Pinocchio original (C++)](https://github.com/elrond79/Pinocchio)
