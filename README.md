# Pinocchiors

Suite de procesamiento 3D en **Rust puro**: auto-rigging, retopología a quads, mapas UV con horneado de texturas, reparación de mallas, preparación para impresión 3D y conversión entre formatos (glTF/GLB, OBJ, STL, PLY, USDA/USDZ).

Incluye una app de escritorio (Tauri + SolidJS + Three.js), una CLI de conversión y bindings WASM.

```
Import → Reparación → Retopología → UV / Piel → Rigging → Pesos → Impresión 3D → Export
```

---

## Contenido

- [Características](#características)
- [Estructura del repositorio](#estructura-del-repositorio)
- [Requisitos](#requisitos)
- [Compilación](#compilación)
- [App de escritorio](#app-de-escritorio)
- [CLI de conversión](#cli-de-conversión)
- [Uso como librería](#uso-como-librería)
- [WASM](#wasm)
- [Tests](#tests)
- [Roadmaps](#roadmaps)
- [Referencias](#referencias)
- [Licencia](#licencia)

---

## Características

### Pinocchio: auto-rigging
Implementación en Rust del algoritmo de [Pinocchio](https://people.csail.mit.edu/ibaran/papers/2007-SIGGRAPH-Pinocchio.pdf) (Baran & Popović, SIGGRAPH 2007).

- Ajusta automáticamente un esqueleto dentro de una malla: detecta las extremidades en el eje medial (patas, cabeza, cola…), busca la orientación de la plantilla (Y o Z arriba, girada) y asigna cada extremo a una extremidad; las articulaciones siguen el eje medial con las proporciones de la plantilla. Detalle en [`libs/pinocchio/ROADMAP.md`](libs/pinocchio/ROADMAP.md) (fase 8).
- Edición manual: articulaciones en espejo (`_l`/`_r`, `.L`/`.R`, `Left`/`Right`), centrado en la sección del miembro, y un esqueleto editado se usa tal cual (`SkeletonFit::Exact`).
- Calcula los pesos de skinning con **bone heat** (Baran & Popović) y difusión con el Laplaciano cotangente. Cada vértice va con el hueso que pasa por el centro de su tubo (no el más cercano), así en un tronco gordo la panza queda con la columna y no con las patas. Las piezas sueltas sin huesos (colmillos, ojos) se mueven rígidas con la parte del cuerpo donde se apoyan.
- Pincel de pesos en la app (sumar, restar, suavizar, espejo) y pose de prueba para revisarlos.
- Trabaja en las coordenadas originales del modelo, suelda costuras UV y transfiere los pesos si la malla se decima.
- Esqueletos predefinidos: `HumanSkeleton`, `QuadSkeleton`, `HorseSkeleton`, `CentaurSkeleton`, `BirdSkeleton`, `SpiderSkeleton`, `SerpentSkeleton`, `MechSkeleton`.
- Esqueletos por forma de cuerpo + apéndices (`BodyPlan`): bípedo, digitígrado, cuadrúpedo, radial, pez, artrópodo o cadena, con cuello, cola, trompa, orejas, alas, tentáculos, aletas, pinzas o antenas de N segmentos, y variantes listas (elefante, jirafa, dragón, pulpo, calamar, pez, delfín, cangrejo, insecto, escorpión, T-rex, ave, serpiente).
- Decimación automática en mallas grandes, normalización y presets `fast()` / `high_quality()`.

### QuadriFlow: retopología
Port de [QuadriFlow](https://github.com/hjwdzh/QuadriFlow) (SGP 2018).

- Campo de orientación 4-RoSy y campo de posición (estilo Instant Meshes).
- Jerarquía multiescala, flujo de costo mínimo y parametrización entera.
- Extracción de una malla de quads con preservación opcional de aristas vivas, modo adaptativo y eliminación de flips.
- Paralelizado con `rayon`.

### UV / Piel (`uv-core`)
La "piel" de la malla: coordenadas UV, materiales y texturas.

- Traspaso de UV del modelo original a la malla retopologizada, sin saltar entre islas en las costuras. Con "Seguir las costuras de UV" (activada por defecto si el modelo tiene UV) los quads siguen las costuras del atlas y cada uno cae dentro de una sola isla.
- Desplegado propio: islas que crecen a la vez (desviación de normal acotada, nunca cruzan aristas vivas) y siempre son discos, LSCM + ARAP, empaquetado con densidad de texel uniforme.
- Horneado de las texturas originales sobre el mapa nuevo (color, metal/rugosidad, oclusión, emisión) y de la normal: la malla liviana conserva el relieve de la original como normal map, con tangentes glTF exportadas.
- Plan y detalles en [`libs/uv/ROADMAP.md`](libs/uv/ROADMAP.md).

### Reparación de mallas (`pinocchio-repair`)
- Diagnóstico: bordes abiertos, vértices duplicados, caras degeneradas, aristas non-manifold y autointersecciones (con BVH y el test de Möller).
- Reparación: fusión de duplicados, eliminación de caras degeneradas, normales consistentes y orientadas hacia afuera, y relleno de agujeros (*ear clipping* o Liepa).

### Impresión 3D (`pinocchio-print3d`)
- Volumen, área, centro de masa y detección de malla cerrada.
- Escalado uniforme o no uniforme, escalado por tamaño o volumen objetivo, y centrado.
- Corte por planos con cierre de las caras cortadas y subdivisión en piezas etiquetadas.
- Uniones entre piezas: dowel, dovetail, puzzle, terraza y pirámide, con tolerancias FDM predefinidas.

### Converter: formatos 3D
Conversor inspirado en `usd_from_gltf` de Google, sin FFI ni dependencias C/C++.

| Formato | Importar | Exportar | Notas |
|---------|:--------:|:--------:|-------|
| glTF / GLB | ✅ | ✅ | PBR, texturas, esqueletos, animaciones, `KHR_materials_unlit`, `KHR_texture_transform` |
| OBJ + MTL | ✅ | ✅ | Materiales y texturas |
| STL | ✅ | ✅ | Lectura ASCII/binario, escritura binaria |
| PLY | ✅ | ✅ | Lectura ASCII/binario (ambos endian), color por vértice, UV por vértice o por esquina y textura de MeshLab; escritura binaria |
| USDA | — | ✅ | UsdSkel (esqueleto, SkelAnimation, binding) |
| USDZ | — | ✅ | ZIP sin compresión alineado a 64 bytes, compatible con AR Quick Look |

Todos los formatos pasan por una representación intermedia común (`converter-scene`).

---

## Estructura del repositorio

```
Pinocchiors/
├── libs/
│   ├── pinocchio/          # Auto-rigging + reparación + impresión 3D
│   │   ├── math/           # Vectores, matrices, transforms (nalgebra)
│   │   ├── sparse/         # Matrices dispersas y solvers (sprs)
│   │   ├── mesh/           # Malla half-edge + carga OBJ/GLB/STL
│   │   ├── skeleton/       # Trait Skeleton + presets
│   │   ├── spatial/        # Octree, BVH, campos de distancia
│   │   ├── graph/          # Grafos y caminos mínimos (petgraph)
│   │   ├── embedding/      # Ajuste del esqueleto dentro de la malla
│   │   ├── attachment/     # Pesos de skinning (difusión de calor)
│   │   ├── repair/         # Diagnóstico y reparación de mallas
│   │   ├── print3d/        # Preparación para impresión 3D
│   │   └── core/           # API pública: autorig()
│   ├── quadriflow/         # Retopología a quads
│   │   └── core/           # API pública: remesh()
│   ├── uv/                 # Mapas UV
│   │   └── core/           # Traspaso, desplegado, empaquetado y horneado
│   └── converter/          # Conversión de formatos
│       ├── scene/          # Formato intermedio (Scene)
│       ├── gltf-io/        # glTF/GLB
│       ├── obj/            # OBJ + MTL
│       ├── stl/            # STL
│       ├── usda/           # USDA + USDZ + UsdSkel
│       ├── core/           # Fachada: convert(), import(), export()
│       └── wasm/           # Bindings wasm-bindgen
└── apps/
    ├── desktop/            # Backend Tauri 2 (Rust)
    ├── web/                # Frontend SolidJS + Three.js + Tailwind
    └── converter/          # CLI de conversión (clap)
```

### Dependencias entre módulos

```
                    ┌────────────────┐
                    │  apps/desktop  │
                    └───────┬────────┘
        ┌───────────────┬───┴──────────┬──────────────────┐
        ▼               ▼              ▼                  ▼
 pinocchio-core   quadriflow-core  pinocchio-repair  converter-*
        │               │          pinocchio-print3d      │
        │               │          uv-core ───────────────┤
        └───────┬───────┘                                 │
                ▼                                         │
          pinocchio-mesh ◀── feature "converter" ─────────┘
```

---

## Requisitos

- **Rust** ≥ 1.85 (edition 2024)
- Para la app de escritorio:
  - **Node.js** ≥ 18 y npm
  - [Dependencias del sistema para Tauri 2](https://v2.tauri.app/start/prerequisites/) (en Linux: `webkit2gtk-4.1`, `libappindicator`, etc.)
- Para WASM: [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)

---

## Compilación

```bash
git clone https://github.com/Debaq/Pinocchiors.git
cd Pinocchiors

# Compilar todo el workspace
cargo build --release

# Solo las librerías (sin la app de escritorio)
cargo build --release -p pinocchio-core -p quadriflow-core -p converter-core
```

El perfil `release` usa `lto = true` y `codegen-units = 1`.

---

## App de escritorio

Interfaz gráfica completa para el pipeline 3D: visor Three.js, outliner de escena, edición del esqueleto, undo/redo y atajos de teclado.

Al importar, el paso **Estructura** muestra lo que trae el archivo: nodos, mallas y primitivas (atributos, material), materiales con miniaturas de sus texturas, texturas, esqueletos y animaciones. El visor muestra el modelo con sus materiales PBR y texturas originales (se pueden apagar en Visualización) y tiene luces ajustables: dirección, altura, intensidad y color de la principal, relleno, ambiente, reflejos de entorno y luz desde la cámara; mantener **L** y arrastrar gira la luz principal alrededor del modelo.

El visor usa los gestos de Blender:

| Gesto | Acción |
|-------|--------|
| Botón central / Alt + izquierdo | Orbitar |
| Shift + central / Shift + Alt + izquierdo | Desplazar |
| Rueda / Ctrl + central / Ctrl + Alt + izquierdo | Zoom |
| Clic izquierdo | Seleccionar articulación (en vacío, deselecciona) |
| G | Mover la articulación con el mouse (X/Y/Z limita al eje; clic o Enter confirma, clic derecho o Esc cancela; se deshace con Ctrl+Z) |
| R | Rotar la articulación como pose de prueba (necesita pesos) |
| B | Pincel de pesos: Ctrl invierte, Shift suaviza, F radio, Shift+F intensidad |
| 1 / 3 / 7 (Ctrl: opuesta) | Vista frontal / derecha / superior |
| . / Inicio | Centrar en la selección / ver todo |
| L + arrastrar | Girar la luz principal |

Cada etapa trabaja con la salida de la anterior. Reparar reemplaza la malla y conserva la piel: UV, materiales y texturas se trasladan desde el original (los parches de agujeros toman la textura del entorno). Los vértices partidos por costuras de UV, normales o materiales no cuentan como duplicados en el diagnóstico; después de la retopología, UV / Piel, esqueleto, pesos, pincel y pose de prueba usan la malla de quads (la que se exporta), salvo que en Retopología se desmarque "Usar esta malla en las etapas siguientes". Impresión 3D sigue sobre la malla original reparada, donde importa la geometría fiel y no la topología. Si se exporta una malla distinta de la del rig, los pesos se trasladan.

```bash
cd apps/web
npm install

# Modo desarrollo (levanta Vite en :5173 y la ventana Tauri)
npx tauri dev --config ../desktop/tauri.conf.json

# Build de producción
npx tauri build --config ../desktop/tauri.conf.json
```

### Comandos Tauri disponibles

| Grupo | Comandos |
|-------|----------|
| Import/Export | `get_supported_formats`, `import_model`, `export_model`, `get_mesh_data` |
| Esqueletos | `list_skeleton_presets`, `select_skeleton`, `get_body_plan`, `select_body_plan`, `get_skeleton_data`, `transform_skeleton`, `move_bone` (con espejo), `auto_fit_skeleton`, `center_bones` |
| Auto-rig | `run_autorig`, `get_weights_data`, `set_vertex_weights` (pincel), `get_weight_mirror` |
| Retopología | `run_retopology`, `get_quad_mesh_data` |
| UV / Piel | `get_uv_info`, `run_uv_unwrap`, `restore_transferred_uvs`, `get_uv_texture`, `get_uv_layout` |
| Reparación | `analyze_mesh`, `repair_mesh`, `undo_repair`, `get_repair_diagnostics` |
| Impresión 3D | `analyze_print3d`, `scale_mesh_for_print`, `subdivide_mesh`, `export_print3d_piece` |

---

## CLI de conversión

Programa de terminal (`converter`) que convierte modelos entre formatos sin abrir la app. Usa las mismas librerías de conversión que la app de escritorio, pero no hace rigging, retopología ni orientación. Sirve para:

- **Convertir muchos archivos de una vez** con `--batch`, en vez de abrirlos uno por uno en la app.
- **Automatizar**: scripts, un servidor o una carpeta que se convierte sola (código de salida 1 si algo falla).
- **Preparar modelos para AR** en iPhone/iPad (GLB → USDZ con `--arkit`).
- **Probar el conversor** sin interfaz mientras se desarrolla.

Cada release trae el binario listo para Linux, Windows y macOS (Intel y ARM): `converter-<versión>-<plataforma>.tar.gz` o `.zip`. Para compilarlo:

```bash
cargo install --path apps/converter
# o bien
cargo run --release -p converter-cli -- <entrada> <salida> [opciones]
```

### Ejemplos

```bash
# GLB → USDZ compatible con AR Quick Look
converter modelo.glb modelo.usdz --arkit

# OBJ → GLB optimizado con texturas de hasta 1024 px
converter modelo.obj modelo.glb --optimize-geometry --max-texture-size 1024 --texture-quality 85

# STL en metros → OBJ en centímetros
converter pieza.stl pieza.obj --scale 100

# Sin archivo de salida: mismo nombre con la extensión de --format
converter personaje.glb --format usdz

# Modo batch: varios archivos, salida junto a cada entrada o en --out-dir
converter --batch --format stl modelos/*.glb --out-dir stl/
```

### Opciones

| Opción | Descripción |
|--------|-------------|
| `--scale <f>` | Factor de escala (p. ej. `100` para pasar de metros a centímetros) |
| `--max-texture-size <px>` | Tamaño máximo de las texturas |
| `--no-animations` | No exportar animaciones |
| `--arkit` | Compatibilidad con AR Quick Look (Apple) |
| `--split-orm` | Separar la textura ORM en canales individuales |
| `--fps <f>` | FPS para las animaciones USD (por defecto `24`) |
| `--texture-quality <1-100>` | Calidad JPEG de las texturas (solo GLB) |
| `--optimize-geometry` | Deduplicar vértices y eliminar degenerados (solo GLB) |
| `--generate-normals` | Generar normales si faltan (solo GLB) |
| `--flatten-transforms` | Hornear los transforms en la geometría (solo GLB, sin esqueletos) |
| `--strip-unused` | Eliminar materiales y texturas no usados (solo GLB) |
| `--format <fmt>` | Formato de salida: `glb`, `usda`, `usdz`, `stl`, `obj` (obligatorio sin archivo de salida) |
| `--batch` | Convierte varios archivos de entrada (requiere `--format`); código de salida 1 si alguno falla |
| `--out-dir <dir>` | Directorio de salida del modo batch |

---

## Uso como librería

Añade los crates como dependencias por ruta o git:

```toml
[dependencies]
pinocchio-core   = { git = "https://github.com/Debaq/Pinocchiors" }
pinocchio-mesh   = { git = "https://github.com/Debaq/Pinocchiors", features = ["converter"] }
quadriflow-core  = { git = "https://github.com/Debaq/Pinocchiors" }
converter-core   = { git = "https://github.com/Debaq/Pinocchiors" }
```

### Auto-rigging

```rust
use pinocchio_core::{autorig, PinocchioConfig};
use pinocchio_core::mesh::load_obj;
use pinocchio_core::skeleton::HumanSkeleton;

let mesh = load_obj("personaje.obj")?;
let skeleton = HumanSkeleton::new();

let result = autorig(&mesh, &skeleton, Some(PinocchioConfig::high_quality()))?;

// Posiciones de los huesos ajustadas a la malla
for (i, pos) in result.bone_positions.iter().enumerate() {
    println!("hueso {i}: {pos:?}");
}

// Pesos de skinning (máximo 4 influencias por vértice)
let (bone_indices, weights) = result.export_weights(4);
```

`PinocchioConfig` admite un builder: `with_diffusion_weight`, `with_max_spheres`, `with_refine_iterations`, `with_max_influences`, `with_resolution`, `with_auto_decimate`, `without_normalization`.

### Retopología

```rust
use quadriflow_core::{remesh_with_callback, RemeshConfig};

let config = RemeshConfig::quality(5_000); // o RemeshConfig::fast(n)
// preserve_seams: true → los quads siguen las costuras de UV (para trasladar la piel)
let quads = remesh_with_callback(&mesh, &config, |stage, msg| {
    println!("[{}] {msg}", stage.name());
})?;

println!("{} vértices, {} quads", quads.vertices.len(), quads.faces.len());
```

### UV / Piel

```rust
use uv_core::{scene_surface, skin_scene, transferred_skin, unwrapped_skin, BakeOptions};

// positions: [[f64; 3]], faces: [[usize; 4]] de la malla retopologizada
let surface = scene_surface(&scene); // UV y normales del original
let skin = match &surface {
    Some(s) => transferred_skin(&scene, s, &positions, &faces), // rápido, conserva el atlas
    None => unwrapped_skin(&scene, None, &positions, &faces, &BakeOptions::default()),
};
// O desplegar de nuevo y hornear (sin costuras, con normal map del original):
let skin = unwrapped_skin(&scene, surface.as_ref(), &positions, &faces, &BakeOptions::default());
let (textured_scene, _) = skin_scene(&positions, &faces, Some(&skin), &scene);
```

### Reparación

```rust
use pinocchio_repair::{analyze, repair_all, AnalysisConfig, RepairConfig};

let diag = analyze(&mesh, &AnalysisConfig::default());
let summary = repair_all(&mut mesh, &RepairConfig::default())?;
```

### Conversión de formatos

```rust
use converter_core::{convert, import, export, ConvertOptions};

// Conversión directa (el formato se detecta por la extensión)
convert("modelo.glb", "modelo.usdz", &ConvertOptions::default())?;

// O paso a paso a través de Scene
let scene = import("modelo.obj")?;
export(&scene, "modelo.glb", &ConvertOptions::default())?;
```

> Para ver la API completa, consulta la documentación de cada crate con `cargo doc --open`.

---

## WASM

`converter-wasm` expone el conversor al navegador:

```bash
wasm-pack build libs/converter/wasm --target web --release
```

```js
import init, { convert, supported_formats, import_to_json } from "./pkg/converter_wasm.js";

await init();
const glb = new Uint8Array(await file.arrayBuffer());
const usdz = convert(glb, "glb", "usdz", JSON.stringify({ arkit_compatible: true }));
```

| Función | Descripción |
|---------|-------------|
| `convert(bytes, in_fmt, out_fmt, options_json?)` | Convierte bytes de un formato a otro |
| `supported_formats()` | Devuelve en JSON los formatos soportados |
| `import_to_json(bytes, fmt)` | Importa un archivo y devuelve la escena en JSON |

---

## Tests

```bash
# Todo el workspace
cargo test --workspace

# Un crate concreto
cargo test -p pinocchio-repair
cargo test -p converter-gltf-io
```

Incluye tests end-to-end del autorig sobre humanoides sintéticos (`libs/pinocchio/core/tests/autorig_e2e.rs`): posiciones de articulaciones, pesos por región del cuerpo, pose T, malla decimada y malla sin soldar.

Los tests de la app de escritorio necesitan las dependencias de sistema de Tauri:

```bash
cargo test -p pinocchio-app
```

---

## Roadmaps

- [`libs/pinocchio/ROADMAP.md`](libs/pinocchio/ROADMAP.md): auto-rigging
- [`libs/pinocchio/print3d/ROADMAP.md`](libs/pinocchio/print3d/ROADMAP.md): impresión 3D
- [`libs/quadriflow/ROADMAP.md`](libs/quadriflow/ROADMAP.md): retopología
- [`libs/converter/ROADMAP.md`](libs/converter/ROADMAP.md): conversor
- [`apps/PLAN_GUI.md`](apps/PLAN_GUI.md): plan de la GUI

### Pendiente
- Uniones entre piezas en `print3d` (`apply_joints_between_pieces`), que requieren CSG.
- Importación de STL desde bytes en WASM.
- Poses y animación en la GUI.

---

## Referencias

- I. Baran, J. Popović. *Automatic Rigging and Animation of 3D Characters*. SIGGRAPH 2007.
- J. Huang, Y. Zhou, M. Nießner, J. Shewchuk, L. Guibas. *QuadriFlow: A Scalable and Robust Method for Quadrangulation*. SGP 2018.
- W. Jakob, M. Tarini, D. Panozzo, O. Sorkine-Hornung. *Instant Field-Aligned Meshes*. SIGGRAPH Asia 2015.
- P. Liepa. *Filling Holes in Meshes*. SGP 2003.
- T. Möller. *A Fast Triangle-Triangle Intersection Test*. JGT 1997.
- Google, [`usd_from_gltf`](https://github.com/google/usd_from_gltf).

---

## Licencia

Doble licencia **MIT** ([LICENSE-MIT](LICENSE-MIT)) o **Apache-2.0** ([LICENSE-APACHE](LICENSE-APACHE)), a elección del usuario.
