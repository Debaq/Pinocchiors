# Pinocchiors

Suite de procesamiento 3D en **Rust puro**: auto-rigging, retopología a quads, reparación de mallas, preparación para impresión 3D y conversión entre formatos (glTF/GLB, OBJ, STL, USDA/USDZ).

Incluye una app de escritorio (Tauri + SolidJS + Three.js), una CLI de conversión y bindings WASM.

```
Import → Reparación → Retopología → Rigging → Pesos → Impresión 3D → Export
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

- Ajusta automáticamente un esqueleto dentro de una malla (esferas mediales, grafo, optimización discreta y refinamiento continuo).
- Calcula los pesos de skinning mediante **difusión de calor** (*bone heat*).
- Esqueletos predefinidos: `HumanSkeleton`, `QuadSkeleton`, `HorseSkeleton`, `CentaurSkeleton`, `BirdSkeleton`, `SpiderSkeleton`, `SerpentSkeleton`, `MechSkeleton`.
- Decimación automática en mallas grandes, normalización y presets `fast()` / `high_quality()`.

### QuadriFlow: retopología
Port de [QuadriFlow](https://github.com/hjwdzh/QuadriFlow) (SGP 2018).

- Campo de orientación 4-RoSy y campo de posición (estilo Instant Meshes).
- Jerarquía multiescala, flujo de costo mínimo y parametrización entera.
- Extracción de una malla de quads con preservación opcional de aristas vivas, modo adaptativo y eliminación de flips.
- Paralelizado con `rayon`.

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
│   │   ├── field/          # Campos de orientación/posición
│   │   ├── hierarchy/      # Jerarquía multiescala
│   │   ├── optimizer/      # Optimización de campos
│   │   ├── flow/           # Min-cost flow
│   │   ├── parametrizer/   # Parametrización entera
│   │   ├── extractor/      # Extracción de la malla quad
│   │   └── core/           # API pública: remesh()
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
| Esqueletos | `list_skeleton_presets`, `select_skeleton`, `get_skeleton_data`, `transform_skeleton`, `move_bone`, `auto_fit_skeleton` |
| Auto-rig | `run_autorig`, `get_weights_data` |
| Retopología | `run_retopology`, `get_quad_mesh_data` |
| Reparación | `analyze_mesh`, `repair_mesh`, `undo_repair`, `get_repair_diagnostics` |
| Impresión 3D | `analyze_print3d`, `scale_mesh_for_print`, `subdivide_mesh`, `export_print3d_piece` |

---

## CLI de conversión

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

# Modo batch: la salida toma el nombre de la entrada con la nueva extensión
converter personaje.glb --batch --format usda
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
| `--batch` / `--format <fmt>` | Modo batch con formato de salida: `glb`, `usda`, `usdz`, `stl`, `obj` |

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
let quads = remesh_with_callback(&mesh, &config, |stage, msg| {
    println!("[{}] {msg}", stage.name());
})?;

println!("{} vértices, {} quads", quads.vertices.len(), quads.faces.len());
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

| Módulo | Tests aprox. |
|--------|-------------:|
| pinocchio (núcleo) | 170+ |
| pinocchio-repair | 61 |
| pinocchio-print3d | 39 |
| converter-usda | 36 |
| converter-gltf-io | 25 |
| converter-core | 14 |
| converter-obj | 6 |

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

Doble licencia **MIT** o **Apache-2.0**, a elección del usuario.
