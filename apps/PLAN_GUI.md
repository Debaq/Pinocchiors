# Plan de Modernización GUI Pinocchiors

## Estado Actual

### Librerías (Backend Rust)
| Librería | Estado | Tests |
|----------|--------|-------|
| pinocchio (9 módulos) | 100% | 170+ |
| converter (5 módulos) | ~90% | 47 |
| quadriflow (7 módulos) | 100% | completo |

### GUI Actual (Tauri)
- **8 comandos Tauri** básicos (load_mesh, autorig, export)
- **Formatos importación:** OBJ, GLB solamente
- **Formatos exportación:** JSON solamente
- **Viewer Three.js:** malla, wireframe, esqueleto, pesos (heatmap)
- **No integra:** converter completo, quadriflow, animaciones

---

## Objetivo: GUI Profesional para Pipeline 3D Completo

### Flujo de trabajo deseado:
```
Import → Retopología → Rigging → Poses → Animación → Dimensionado/Corte → Export
```

---

## Fase 1: Integración de Formatos (Prioridad Alta)

### 1.1 Import Multi-formato
**Comandos Tauri nuevos:**
```rust
// Usar converter-scene como formato pivote
import_file(path: String) -> Result<SceneInfo>  // Auto-detect: GLB/OBJ/STL
import_glb(path) -> Scene
import_obj(path) -> Scene
import_stl(path) -> Scene
```

**Dependencias:**
- `converter-gltf-io` (import GLB)
- `converter-obj` (import OBJ)
- `converter-stl` (import STL)

**UI:**
- Dropdown en diálogo open con todos los formatos
- Preview de información antes de importar

### 1.2 Export Multi-formato
**Comandos Tauri nuevos:**
```rust
export_glb(scene, path) -> Result<()>
export_obj(scene, path) -> Result<()>
export_stl(scene, path) -> Result<()>
export_usda(scene, options, path) -> Result<UsdaOutput>
export_usdz(scene, options, path) -> Result<()>  // Fase 5 converter pendiente
```

**UI:**
- Panel de exportación con opciones por formato
- Preview de archivos generados
- Opciones USDA: escala, up-axis, split texturas ORM

---

## Fase 2: Retopología con QuadriFlow

### 2.1 Backend
**Comando Tauri:**
```rust
run_retopology(config: RetopologyConfig, on_progress: Channel<Progress>) -> Result<QuadMeshData>

struct RetopologyConfig {
    target_quads: usize,        // Número objetivo de quads
    preserve_sharp: bool,        // Preservar bordes agudos
    sharp_angle: f32,            // Ángulo de detección (grados)
    adaptive: bool,              // Resolución adaptiva por curvatura
}
```

**Dependencias:**
- `quadriflow-core::remesh(mesh, config)`

### 2.2 UI
- **Panel Retopología:**
  - Slider: número de quads objetivo
  - Checkbox: preservar bordes agudos
  - Slider: ángulo de bordes (15-90°)
  - Toggle: resolución adaptiva
  - Botón: "Ejecutar Retopología"
  - Progress bar con etapas (field, flow, extract)

- **Visualización:**
  - Toggle: ver malla original vs retopologizada
  - Overlay: comparación lado a lado
  - Stats: número de quads, singularidades, calidad

---

## Fase 3: Sistema de Esqueletos Mejorado

### 3.1 Nuevos Presets
Agregar los 4 esqueletos nuevos de pinocchio-skeleton:
- BirdSkeleton (24 huesos)
- SpiderSkeleton (26 huesos)
- SerpentSkeleton (16 huesos, configurable)
- MechSkeleton (28 huesos)

### 3.2 Editor de Esqueleto
**Comandos Tauri:**
```rust
create_custom_skeleton(bones: Vec<BoneInput>) -> SkeletonData
modify_bone(skeleton_id, bone_idx, changes: BoneChanges)
save_skeleton_preset(skeleton: SkeletonData, name: String)
load_skeleton_json(path: String) -> SkeletonData
```

**UI:**
- **Modo edición de huesos:**
  - Arrastrar para reposicionar
  - Click derecho: añadir hijo, eliminar
  - Panel propiedades: nombre, posición, restricciones

- **Biblioteca de esqueletos:**
  - Presets integrados (8 tipos)
  - Esqueletos personalizados guardados
  - Import/export JSON

---

## Fase 4: Sistema de Poses y Timeline

### 4.1 Base de Poses
**Estructuras:**
```rust
struct Pose {
    name: String,
    bone_transforms: HashMap<String, Transform>,  // nombre_hueso -> transform
    tags: Vec<String>,                             // "idle", "walk", "action"
}

struct PoseLibrary {
    poses: Vec<Pose>,
    categories: Vec<String>,
}
```

**Comandos Tauri:**
```rust
create_pose(skeleton_id, name: String) -> PoseId
save_pose_library(path: String)
load_pose_library(path: String) -> PoseLibrary
apply_pose(skeleton_id, pose_id)
blend_poses(pose_a, pose_b, factor: f32) -> Pose
```

### 4.2 Timeline
**Estructuras:**
```rust
struct Timeline {
    duration_frames: u32,
    fps: f32,
    keyframes: Vec<Keyframe>,
    curves: HashMap<String, AnimationCurve>,  // nombre_hueso.propiedad -> curva
}

struct Keyframe {
    frame: u32,
    pose: Pose,
    interpolation: Interpolation,  // Linear, Bezier, Step
}
```

**UI - Timeline Panel:**
```
┌─────────────────────────────────────────────────────────────┐
│ ◀ ▶ ⏸ │ 0:00.00 │ ━━━━━━━━●━━━━━━━━━━━━━━━━ │ 30 FPS │ 5s │
├─────────────────────────────────────────────────────────────┤
│ ▼ Root          │ ◆──────────────◆─────────────────◆──── │
│   ▼ Spine       │ ────────◆──────────────◆──────────── │
│     ▼ Chest     │ ◆────────────────────◆─────────◆─── │
│       ▼ Head    │ ──────◆────────────────────◆────── │
│       ▼ L_Arm   │ ◆─────────◆──────────────◆──────── │
│       ▼ R_Arm   │ ◆─────────◆──────────────◆──────── │
│   ▼ L_Leg       │ ◆─────────────────◆──────────────── │
│   ▼ R_Leg       │ ◆─────────────────◆──────────────── │
└─────────────────────────────────────────────────────────────┘
```

**Funcionalidades:**
- Scrub timeline (arrastrar para preview)
- Insertar keyframe en frame actual
- Copiar/pegar poses entre keyframes
- Curvas de interpolación editables
- Export a animación (converter-scene Animation)

---

## Fase 5: Dimensionado y Corte para Impresión 3D

### 5.1 Sistema de Dimensionado
**Comandos Tauri:**
```rust
get_real_dimensions(mesh_id) -> Dimensions3D
scale_to_size(mesh_id, target: Dimensions3D, axis: Axis) -> Result<()>
set_unit(mesh_id, unit: Unit)  // mm, cm, m, inch

struct Dimensions3D {
    width: f64,   // X
    height: f64,  // Y
    depth: f64,   // Z
    unit: Unit,
}
```

**UI - Panel Dimensiones:**
```
┌─────────────────────────────┐
│ Dimensiones Reales          │
├─────────────────────────────┤
│ Ancho (X):  [150.0] mm  🔒  │
│ Alto (Y):   [280.0] mm      │
│ Fondo (Z):  [120.0] mm  🔒  │
├─────────────────────────────┤
│ Unidades: [mm ▼]            │
│ Escala: 1.0                 │
│ [Aplicar Dimensiones]       │
└─────────────────────────────┘
```
- 🔒 = mantener proporción
- Input directo de dimensiones objetivo
- Conversión entre unidades

### 5.2 Sistema de Corte
**Estructuras:**
```rust
struct CutPlane {
    position: Vec3,
    normal: Vec3,
    connector_type: ConnectorType,
}

enum ConnectorType {
    None,
    Pins { diameter: f64, depth: f64, count: u32 },
    Dovetail { angle: f64, depth: f64 },
    Puzzle { tolerance: f64 },
    Magnet { diameter: f64, depth: f64 },
}

struct CutResult {
    pieces: Vec<MeshPiece>,
    connectors: Vec<ConnectorGeometry>,
}
```

**Comandos Tauri:**
```rust
add_cut_plane(position, normal) -> CutPlaneId
set_connector_type(plane_id, connector: ConnectorType)
preview_cut(planes: Vec<CutPlaneId>) -> CutPreview
execute_cut(planes: Vec<CutPlaneId>) -> CutResult
export_pieces(pieces: Vec<MeshPiece>, format: String, path: String)
```

**UI - Panel de Corte:**
```
┌─────────────────────────────────────┐
│ Planos de Corte                     │
├─────────────────────────────────────┤
│ [+ Añadir Plano]  [Modo: XY ▼]      │
├─────────────────────────────────────┤
│ ▼ Plano 1 (Y = 140mm)               │
│   Conector: [Pins ▼]                │
│   Diámetro: [4.0] mm                │
│   Profundidad: [8.0] mm             │
│   Cantidad: [4]                     │
│                                     │
│ ▼ Plano 2 (Y = 280mm)               │
│   Conector: [Puzzle ▼]              │
│   Tolerancia: [0.2] mm              │
├─────────────────────────────────────┤
│ Piezas resultantes: 3               │
│ Pieza más grande: 140 x 120 x 150mm │
│                                     │
│ [Preview Corte]  [Ejecutar Corte]   │
│ [Exportar Piezas STL]               │
└─────────────────────────────────────┘
```

**Visualización 3D:**
- Planos de corte semitransparentes
- Preview de conectores en 3D
- Separación animada de piezas
- Color por pieza para identificación

---

## Fase 6: Mejoras de UI/UX General

### 6.1 Layout Profesional
```
┌───────────────────────────────────────────────────────────────────┐
│ Menu: Archivo │ Editar │ Vista │ Malla │ Esqueleto │ Herramientas │
├───────────┬───────────────────────────────────────┬───────────────┤
│ Outliner  │                                       │ Properties    │
│ ─────────│         Viewport 3D                   │ ─────────────│
│ ▼ Scene   │                                       │ [Tab: Mesh]   │
│   ▼ Mesh  │                                       │ [Tab: Skel]   │
│   ▼ Skel  │                                       │ [Tab: Retopo] │
│   ▼ Anim  │                                       │ [Tab: Anim]   │
│           │                                       │ [Tab: Cut]    │
├───────────┴───────────────────────────────────────┴───────────────┤
│                         Timeline                                  │
│ ◀ ▶ ⏸ │ Frame 0 │ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
└───────────────────────────────────────────────────────────────────┘
```

### 6.2 Nuevas Características UI
- **Undo/Redo** completo con historial
- **Múltiples viewports** (4 vistas: perspectiva, frontal, lateral, superior)
- **Gizmos 3D** para transformaciones (mover, rotar, escalar)
- **Snapping** a grid, vértices, bordes
- **Atajos de teclado** configurables
- **Temas** claro/oscuro
- **Drag & drop** para importar archivos

### 6.3 Persistencia
- Guardar/cargar proyectos completos (.pinocchio)
- Autosave cada N minutos
- Historial de archivos recientes

---

## Dependencias Nuevas

### Rust (Cargo.toml desktop)
```toml
[dependencies]
# Existentes
pinocchio-core = { path = "../../libs/pinocchio/crates/pinocchio-core" }
pinocchio-mesh = { path = "../../libs/pinocchio/crates/pinocchio-mesh", features = ["converter"] }
pinocchio-skeleton = { path = "../../libs/pinocchio/crates/pinocchio-skeleton", features = ["converter"] }

# Nuevos
quadriflow-core = { path = "../../libs/quadriflow/crates/quadriflow-core" }
converter-scene = { path = "../../libs/converter/crates/converter-scene" }
converter-gltf-io = { path = "../../libs/converter/crates/converter-gltf-io" }
converter-stl = { path = "../../libs/converter/crates/converter-stl" }
converter-obj = { path = "../../libs/converter/crates/converter-obj" }
converter-usda = { path = "../../libs/converter/crates/converter-usda" }
```

### TypeScript (package.json web)
```json
{
  "dependencies": {
    "three": "^0.170",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "gsap": "^3.12"  // Para animaciones timeline
  }
}
```

---

## Cronograma Estimado

| Fase | Descripción | Complejidad | Duración |
|------|-------------|-------------|----------|
| 1 | Import/Export multi-formato | Media | 1-2 semanas |
| 2 | Retopología QuadriFlow | Media | 1 semana |
| 3 | Sistema esqueletos mejorado | Baja | 3-5 días |
| 4 | Poses y Timeline | Alta | 2-3 semanas |
| 5 | Dimensionado y Corte | Alta | 2-3 semanas |
| 6 | Mejoras UI/UX | Media | 2 semanas |

**Total estimado:** 8-12 semanas

---

## Prioridad Recomendada

1. **Fase 1** - Import/Export (fundamento para todo)
2. **Fase 2** - Retopología (valor inmediato)
3. **Fase 5** - Dimensionado/Corte (diferenciador para impresión 3D)
4. **Fase 3** - Esqueletos mejorados (extensión natural)
5. **Fase 4** - Timeline (más complejo, puede hacerse incremental)
6. **Fase 6** - UI/UX (mejoras continuas)

---

## Notas Técnicas

### Arquitectura Recomendada
- **Scene como formato pivote:** Todas las operaciones trabajan sobre `converter-scene::Scene`
- **Estado en AppState:** Mantener Scene actual, historial de undo, configuración
- **Comandos async:** Operaciones pesadas (retopología, corte) con progress channel
- **Serialización:** Scene puede serializarse a JSON para guardar proyectos

### Consideraciones de Rendimiento
- Decimación automática para preview (>100K triángulos)
- Web workers para operaciones de UI pesadas
- Lazy loading de texturas
- Level of Detail (LOD) en viewport

### Testing
- Tests unitarios para cada comando Tauri nuevo
- Tests de integración converter → GUI
- Tests E2E con Playwright/Tauri test

