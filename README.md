# Pinocchiors

Suite de procesamiento 3D en **Rust**: auto-rigging, pose y animación, retopología a quads, remallado, mapas UV con horneado de texturas, reparación de mallas, preparación para impresión 3D, CAD paramétrico (con OpenCASCADE), render por trazado de rayos, captura con escáneres y conversión entre formatos (glTF/GLB, OBJ, STL, PLY, 3MF, USDA/USDZ).

Incluye una app de escritorio (Tauri + SolidJS + Three.js), una CLI de conversión y bindings WASM.

```
Import → Reparación → Retopología → UV / Piel → Rigging → Pesos → Animación → Impresión 3D → Export
```

El CAD paramétrico, el escaneo y el render son módulos aparte dentro de la misma app (ver [Características](#características)).

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
- [Pendientes y planes](#pendientes-y-planes)
- [Referencias](#referencias)
- [Licencia](#licencia)

---

## Características

### Pinocchio: auto-rigging
Implementación en Rust del algoritmo de [Pinocchio](https://people.csail.mit.edu/ibaran/papers/2007-SIGGRAPH-Pinocchio.pdf) (Baran & Popović, SIGGRAPH 2007).

- Ajusta automáticamente un esqueleto dentro de una malla: detecta las extremidades en el eje medial (patas, cabeza, cola…), busca la orientación de la plantilla (Y o Z arriba, girada) y asigna cada extremo a una extremidad; las articulaciones siguen el eje medial con las proporciones de la plantilla.
- Edición manual: articulaciones en espejo (`_l`/`_r`, `.L`/`.R`, `Left`/`Right`), centrado en la sección del miembro, y un esqueleto editado se usa tal cual (`SkeletonFit::Exact`).
- Calcula los pesos de skinning con **bone heat** (Baran & Popović) y difusión con el Laplaciano cotangente. Cada vértice va con el hueso que pasa por el centro de su tubo (no el más cercano), así en un tronco gordo la panza queda con la columna y no con las patas. Las piezas sueltas sin huesos (colmillos, ojos) se mueven rígidas con la parte del cuerpo donde se apoyan.
- Pincel de pesos en la app (sumar, restar, suavizar, espejo) y pose de prueba para revisarlos.
- Trabaja en las coordenadas originales del modelo, suelda costuras UV y transfiere los pesos si la malla se decima.
- Esqueletos predefinidos: `HumanSkeleton`, `QuadSkeleton`, `HorseSkeleton`, `CentaurSkeleton`, `BirdSkeleton`, `SpiderSkeleton`, `SerpentSkeleton`, `MechSkeleton`.
- Esqueletos por forma de cuerpo + apéndices (`BodyPlan`): bípedo, digitígrado, cuadrúpedo, radial, pez, artrópodo o cadena, con cuello, cola, trompa, orejas, alas, tentáculos, aletas, pinzas o antenas de N segmentos, y variantes listas (elefante, jirafa, dragón, pulpo, calamar, pez, delfín, cangrejo, insecto, escorpión, T-rex, ave, serpiente).
- Decimación automática en mallas grandes, normalización y presets `fast()` / `high_quality()`.
- Pose y animación en la app: rig con IK y límites, restricciones, línea de tiempo con keys, curvas y NLA, retargeting y BVH, captura de movimiento con MediaPipe (cámara), resortes y ragdoll, skinning por GPU y exportación animada.

### QuadriFlow: retopología
Port de [QuadriFlow](https://github.com/hjwdzh/QuadriFlow) (SGP 2018).

- Campo de orientación 4-RoSy y campo de posición (estilo Instant Meshes).
- Jerarquía multiescala, flujo de costo mínimo y parametrización entera.
- Extracción de una malla de quads con preservación opcional de aristas vivas, modo adaptativo y eliminación de flips.
- Paralelizado con `rayon`.
- Estado: desarrollo **pausado** desde 2026-09-27 (ver [`PENDIENTES.md`](PENDIENTES.md)).
- En la app, la sección **Remallar** reúne además simplificar, suavizar, remallado isótropo, por vóxeles y a quads.

### UV / Piel (`uv-core`)
La "piel" de la malla: coordenadas UV, materiales y texturas.

- Traspaso de UV del modelo original a la malla retopologizada, sin saltar entre islas en las costuras. Con "Seguir las costuras de UV" (activada por defecto si el modelo tiene UV) los quads siguen las costuras del atlas y cada uno cae dentro de una sola isla.
- Desplegado propio: islas que crecen a la vez (desviación de normal acotada, nunca cruzan aristas vivas) y siempre son discos, LSCM + ARAP, empaquetado con densidad de texel uniforme.
- Horneado de las texturas originales sobre el mapa nuevo (color, metal/rugosidad, oclusión, emisión) y de la normal: la malla liviana conserva el relieve de la original como normal map, con tangentes glTF exportadas.
- Editor de texturas dividido 2D/3D con capas (importa y exporta PSD y XCF, vía `converter-layers`) y «Editar en GIMP».

### Reparación de mallas (`pinocchio-repair`)
- Diagnóstico: bordes abiertos, vértices duplicados, caras degeneradas, aristas non-manifold y autointersecciones (con BVH y el test de Möller).
- Reparación: fusión de duplicados, eliminación de caras degeneradas, normales consistentes y orientadas hacia afuera, y relleno de agujeros (*ear clipping* o Liepa).

### Impresión 3D (`pinocchio-print3d`)
- Volumen, área, centro de masa y detección de malla cerrada.
- Escalado uniforme o no uniforme, escalado por tamaño o volumen objetivo, y centrado.
- Corte por planos con cierre de las caras cortadas y subdivisión en piezas etiquetadas.
- Uniones entre piezas: dowel, dovetail (también deslizante), puzzle, terraza, pirámide y cierre de cuarto de vuelta, con tolerancias FDM predefinidas.
- Cortes óptimos (pocas piezas, por los cuellos) y a mano con planos en el visor; orientación que cabe en la impresora, soporte estimado y marcas de pared fina.
- Vaciado con cáscara por vóxeles, drenaje y venteo; distribución de piezas en placas (nesting) y preparar todo para imprimir en un paso.
- Impresoras y materiales propios (holgura y contracción por eje), modelos de calibración y pieles imprimibles (mapas de altura por zonas con UV).

### CAD paramétrico (`libs/cad`)
Modelador paramétrico con historial de operaciones sobre **OpenCASCADE** (`cad-occt`, puente C++; sin OCCT se compila un stub y el resto del workspace sigue funcionando).

- Sketch con restricciones y cotas (`cad-solver`), operaciones (extrusión, revolución, barrido, recubrimiento, redondeos, vaciado, agujeros, roscas, patrones), chapa metálica con desarrollo y dobladillo, y STEP de entrada/salida (`cad-model`).
- Ensambles con relaciones (fija, bisagra, deslizante, cilíndrica, plana, rótula, paralela, pasador en ranura), vista explosionada y GLB animado de una unión.
- Engranes (rectos, helicoidales, cónicos, sin fin, planetarios, poleas y piñones de cadena) con cálculo de resistencia, y planos 2D a PDF.
- Escaneo → CAD (`cad-scan`): ajustar primitivas, curvas, moldes y superficies libres sobre un escaneo. Ver `libs/cad/ROADMAP.md`.

### Render (`geppettoray`)
Motor propio de trazado de rayos en Rust: BSDF glTF, luces y HDRI (seis entornos incluidos, ver `apps/desktop/assets/hdri/`), vidrio, cámara real, denoise espacial, animación y suelo con sombra. Plan en `libs/PLAN_RENDER.md`.

### Escáner (`orizon3d-core`)
Captura con escáneres Revopoint (V4L2) y plato giratorio con webcam y láser, nubes de puntos con edición, registro de tomas y mallado. Ver `libs/orizon3d/ROADMAP.md`.

### Converter: formatos 3D
Conversor inspirado en `usd_from_gltf` de Google, sin FFI ni dependencias C/C++.

| Formato | Importar | Exportar | Notas |
|---------|:--------:|:--------:|-------|
| glTF / GLB | ✅ | ✅ | PBR, texturas, esqueletos, animaciones, `KHR_materials_unlit`, `KHR_texture_transform`; compresión Draco (lee y escribe) y reducción de triángulos |
| OBJ + MTL | ✅ | ✅ | Materiales y texturas |
| STL | ✅ | ✅ | Lectura ASCII/binario, escritura binaria |
| PLY | ✅ | ✅ | Lectura ASCII/binario (ambos endian), color por vértice, UV por vértice o por esquina y textura de MeshLab; escritura binaria |
| 3MF | — | ✅ | Impresión 3D (`converter-3mf`) |
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
│   ├── cad/                # CAD paramétrico
│   │   ├── solver/         # Solver de restricciones del sketch
│   │   ├── occt/           # Puente C++ a OpenCASCADE (stub sin OCCT)
│   │   ├── model/          # Documento, operaciones, ensambles, chapa, engranes
│   │   └── scan/           # Escaneo → CAD
│   ├── geppettoray/        # Motor de render por trazado de rayos
│   ├── orizon3d/           # Escáneres 3D (Revopoint, plato giratorio)
│   └── converter/          # Conversión de formatos
│       ├── scene/          # Formato intermedio (Scene)
│       ├── gltf-io/        # glTF/GLB (+ Draco)
│       ├── obj/            # OBJ + MTL
│       ├── stl/            # STL
│       ├── ply/            # PLY
│       ├── threemf/        # 3MF
│       ├── usda/           # USDA + USDZ + UsdSkel
│       ├── layers/         # Capas de texturas PSD / XCF
│       ├── core/           # Fachada: convert(), import(), export()
│       └── wasm/           # Bindings wasm-bindgen
└── apps/
    ├── desktop/            # Backend Tauri 2 (Rust); example `cad_http` (puente HTTP para e2e del CAD)
    ├── web/                # Frontend SolidJS + Three.js + Tailwind (e2e en e2e/)
    └── converter/          # CLI de conversión (clap)
```

Además: `scripts/` (compilar OpenCASCADE estático, iconos, release) y `pinocchio.sh` (dev, build y ejecución).

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

`apps/desktop` también usa `cad-*` (CAD y escaneo→CAD), `geppettoray` (render) y `orizon3d-core` (escáneres).

---

## Requisitos

- **Rust** ≥ 1.88 (edition 2024)
- Para la app de escritorio:
  - **Node.js** ≥ 18 y npm
  - [Dependencias del sistema para Tauri 2](https://v2.tauri.app/start/prerequisites/) (en Linux: `webkit2gtk-4.1`, `libappindicator`, etc.)
- Para WASM: [`wasm-pack`](https://rustwasm.github.io/wasm-pack/)
- Para el CAD con geometría real: OpenCASCADE 7.9 (`scripts/build-occt.sh` lo compila estático; `OCCT_INCLUDE_DIR`/`OCCT_LIB_DIR`, `OCCT_STATIC=1`, `CAD_REQUIRE_OCCT=1`). Sin OCCT todo compila con un stub y el CAD responde «OpenCASCADE no disponible».

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

`./pinocchio.sh` envuelve el flujo de la app: `dev`, `dev:web`, `build`, `build:debug`, `build:web`, `build:all`, `run`, `install`, `info`, `clean` (sin argumentos abre un menú).

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

La app expone unos 220 comandos (`apps/desktop/src/`, registrados en `lib.rs`). Estos son los grupos principales con ejemplos; la lista completa está en el código.

| Grupo | Comandos |
|-------|----------|
| Import/Export | `get_supported_formats`, `import_model`, `export_model`, `get_mesh_data` |
| Proyecto y objetos | `new_project`, `open_project`, `save_project`, `objects_info`, `export_objects`, `take_snapshot` |
| Esqueletos | `list_skeleton_presets`, `select_skeleton`, `get_body_plan`, `select_body_plan`, `get_skeleton_data`, `transform_skeleton`, `move_bone` (con espejo), `auto_fit_skeleton`, `center_bones` |
| Auto-rig | `run_autorig`, `get_weights_data`, `set_vertex_weights` (pincel), `get_weight_mirror` |
| Retopología | `run_retopology`, `get_quad_mesh_data` |
| UV / Piel | `get_uv_info`, `run_uv_unwrap`, `restore_transferred_uvs`, `get_checker_texture`, `get_skin_materials`, `get_skin_texture`, `unwrap_original_mesh`, `undo_unwrap_original` |
| Reparación | `analyze_mesh`, `repair_mesh`, `undo_repair`, `get_repair_diagnostics` |
| Remallar | `remesh_preview`, `remesh_info`, `remesh_voxel_grid`, `remesh_apply`, `remesh_discard` |
| Texturas y materiales | `import_texture`, `export_texture`, `edit_texture_externally`, `get_scene_materials`, `set_scene_material`, `assign_scene_material` |
| Impresión 3D | `analyze_print3d`, `scale_mesh_for_print`, `subdivide_mesh`, `export_print3d_piece`, `check_print_readiness`, `suggest_print_orientation`, `add_print_joints`, `estimate_print_material`, `export_print_plates`, `prepare_for_printing`, `apply_print_skin` |
| CAD | `cad_new`, `cad_get_document`, `cad_set_document`, `cad_evaluate`, `cad_solve_sketch`, `cad_mesh`, `cad_assembly_mesh`, `cad_gear_design`, `cad_drawing`, `cad_export`, `cad_import_step`, `cad_export_assembly_animation` |
| Escaneo → CAD | `cad_scan_use`, `cad_scan_detect`, `cad_scan_slice`, `cad_scan_mold`, `cad_scan_skin`, `cad_scan_propose`, `cad_scan_apply` |
| Escáneres | `scanner_connect`, `scanner_preview`, `scanner_scan`, `scan_cloud_add`, `scan_cloud_fuse`, `scan_cloud_create_model` |
| Render | `ray_render_start`, `ray_render_animation_start`, `ray_render_status`, `ray_render_image`, `ray_render_cancel` |

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
| `--draco` / `--draco-level <0-10>` | Comprimir la geometría con Draco (glTF/GLB); el nivel implica `--draco` |
| `--simplify <0-1>` | Fracción de triángulos a conservar (glTF/GLB) |
| `--simplify-error <f>` | Error máximo de la reducción, relativo al tamaño del modelo (por defecto `0.01`; requiere `--simplify`) |
| `--format <fmt>` | Formato de salida: `glb`, `gltf`, `usda`, `usdz`, `stl`, `obj`, `ply`, `3mf` (obligatorio sin archivo de salida) |
| `--batch` | Convierte varios archivos de entrada (requiere `--format`); código de salida 1 si alguno falla |
| `--out-dir <dir>` | Directorio de salida del modo batch (requiere `--batch`) |

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

Pruebas del frontend (`apps/web`): `npm run e2e:unit` (`node --test`) y `npm run e2e:cad` (arnés CDP contra `cad_http`, con `vite`). El único workflow es `.github/workflows/release.yml` (tests + binarios con CAD) y solo se lanza a mano o con un tag `pre.*`.

Los tests de la app de escritorio necesitan las dependencias de sistema de Tauri:

```bash
cargo test -p pinocchio-app
```

---

## Pendientes y planes

Lo que falta, verificado contra el código, está en [`PENDIENTES.md`](PENDIENTES.md). Planes vivos: `libs/cad/ROADMAP.md` (y sus `PLAN_*.md`), `libs/pinocchio/print3d/PLAN_PRINT3D.md`, `libs/PLAN_RENDER.md` y `libs/orizon3d/ROADMAP.md`.

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
