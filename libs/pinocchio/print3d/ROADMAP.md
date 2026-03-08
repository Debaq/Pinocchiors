# pinocchio-print3d - Roadmap

Librería para preparación de modelos 3D para impresión. Permite redimensionar, medir, calcular volúmenes, cortar/subdividir piezas grandes, numerarlas y opcionalmente generar insertos para ensamblaje.

## Fases de Implementación

---

## Fase 1: Análisis y Medición

**Objetivo**: Funciones para analizar propiedades geométricas de mallas.

### Tareas

- [ ] `compute_volume(mesh) -> f64`
  - Algoritmo: Divergence theorem (tetraedralización con origen)
  - Fórmula: `V = Σ (v0 · (v1 × v2)) / 6` para cada triángulo
  - Requiere malla cerrada (watertight)

- [ ] `compute_surface_area(mesh) -> f64`
  - Suma de áreas de todos los triángulos
  - `A = Σ ||(v1-v0) × (v2-v0)|| / 2`

- [ ] `compute_center_of_mass(mesh) -> Vector3<f64>`
  - Método volumétrico: `CoM = Σ(vol_tetra × centroide_tetra) / vol_total`
  - Centroide de tetraedro: `(v0 + v1 + v2 + origen) / 4`

- [ ] `compute_bounding_box(mesh) -> BoundingBox`
  - Min/max de todos los vértices
  - Retorna dimensiones X, Y, Z

- [ ] `analyze(mesh) -> MeshAnalysis`
  - Estructura con todas las propiedades calculadas
  - Incluye: volumen, área, CoM, bounding box, es_cerrada

### Tests

- [ ] Test volumen cubo unitario = 1.0
- [ ] Test volumen esfera (aproximado)
- [ ] Test área cubo = 6.0
- [ ] Test CoM cubo centrado = (0.5, 0.5, 0.5)
- [ ] Test bounding box correcto

---

## Fase 2: Transformaciones

**Objetivo**: Escalar y orientar modelos para impresión.

### Tareas

- [ ] `scale(mesh, factor: f64)`
  - Escala uniforme desde el centro
  - Modifica vértices in-place

- [ ] `scale_non_uniform(mesh, factors: Vector3<f64>)`
  - Escala diferente por eje

- [ ] `scale_to_fit(mesh, max_size: Vector3<f64>) -> f64`
  - Escala para caber en volumen dado
  - Retorna factor aplicado

- [ ] `scale_to_volume(mesh, target_volume: f64) -> f64`
  - Escala para alcanzar volumen objetivo
  - Útil para estimar material

- [ ] `translate_to_origin(mesh)`
  - Mueve bounding box min a (0,0,0)

- [ ] `center_on_origin(mesh)`
  - Centra el modelo en el origen

- [ ] `orient_flat_on_bed(mesh)`
  - Orienta cara más grande hacia Z=0
  - Heurística: buscar triángulo/grupo más plano

### Tests

- [ ] Test escala x2 duplica dimensiones
- [ ] Test scale_to_fit respeta límites
- [ ] Test translate_to_origin posiciona correctamente

---

## Fase 3: Corte y Subdivisión

**Objetivo**: Dividir modelos grandes en piezas imprimibles.

### Dependencia

- Integrar `csgrs` crate para operaciones CSG

### Tareas

- [ ] `Plane` struct
  - `origin: Vector3<f64>`
  - `normal: Vector3<f64>`
  - Métodos: `from_points()`, `distance_to_point()`

- [ ] `slice_by_plane(mesh, plane) -> (Mesh, Mesh)`
  - Divide malla en dos partes
  - Genera caras de cierre en el corte
  - Usa BSP tree o integra `csgrs::cut()`

- [ ] `slice_by_planes(mesh, planes) -> Vec<Mesh>`
  - Múltiples cortes secuenciales

- [ ] `SubdivideConfig`
  - `max_dimension: f64` - tamaño máximo por pieza
  - `build_volume: Vector3<f64>` - volumen de impresora
  - `overlap: f64` - solapamiento para joints (0 por defecto)
  - `strategy: SubdivideStrategy` - Grid, Optimal, Manual

- [ ] `subdivide(mesh, config) -> Vec<LabeledPiece>`
  - Divide automáticamente en piezas
  - Calcula planos de corte según estrategia
  - Numera piezas automáticamente

- [ ] `SubdivideStrategy::Grid`
  - Cortes regulares en X, Y, Z

- [ ] `SubdivideStrategy::Optimal`
  - Minimiza número de cortes
  - Respeta volumen de impresora

### Tests

- [ ] Test corte cubo por mitad = 2 piezas iguales
- [ ] Test corte genera caras cerradas
- [ ] Test subdivide respeta max_dimension
- [ ] Test numeración correcta

---

## Fase 4: Etiquetado de Piezas

**Objetivo**: Sistema de numeración y tracking de piezas.

### Tareas

- [ ] `LabeledPiece` struct
  ```rust
  pub struct LabeledPiece {
      pub mesh: Mesh,
      pub id: usize,
      pub label: String,         // "1A", "1B", "2A"...
      pub original_position: Vector3<f64>,
      pub neighbors: Vec<usize>, // IDs de piezas adyacentes
      pub cut_faces: Vec<usize>, // índices de caras de corte
  }
  ```

- [ ] `LabelingScheme`
  - `Numeric` - 1, 2, 3...
  - `Alphanumeric` - 1A, 1B, 2A...
  - `Coordinate` - X0Y0Z0, X1Y0Z0...

- [ ] `generate_labels(pieces, scheme) -> Vec<LabeledPiece>`
  - Asigna labels según esquema

- [ ] `find_neighbors(pieces) -> ()`
  - Detecta qué piezas son adyacentes
  - Usa proximidad de caras de corte

- [ ] `export_assembly_info(pieces, path)`
  - Exporta JSON con info de ensamblaje
  - Lista de piezas, neighbors, posiciones

### Tests

- [ ] Test labels únicos
- [ ] Test neighbors correctos
- [ ] Test export/import JSON

---

## Fase 5: Sistema de Joints (Manual/Opcional)

**Objetivo**: Generar geometría de insertos para ensamblaje.

### Tipos de Joint

1. **Dowel (Clavija)**
   - Cilindro simple
   - Config: diámetro, profundidad, tolerancia

2. **Dovetail (Cola de milano)**
   - Trapecio con ángulo
   - Config: ancho, altura, ángulo, tolerancia

3. **Puzzle**
   - Forma de pieza de puzzle
   - Config: tamaño, profundidad

4. **Terrace (Escalonado)**
   - Escalones para alineación
   - Config: niveles, altura_escalón

### Tareas

- [ ] `JointConfig` trait
  - Métodos comunes para todos los joints

- [ ] `DowelConfig`
  ```rust
  pub struct DowelConfig {
      pub diameter: f64,      // mm
      pub depth: f64,         // mm
      pub tolerance: f64,     // -0.4mm típico para FDM
      pub count: usize,       // cantidad por cara
  }
  ```

- [ ] `DovetailConfig`
  ```rust
  pub struct DovetailConfig {
      pub width: f64,
      pub height: f64,
      pub depth: f64,
      pub angle: f64,         // grados, típico 7-15°
      pub tolerance: f64,
  }
  ```

- [ ] `generate_dowel_geometry(config) -> Mesh`
  - Genera cilindro para inserto
  - Versión macho y hembra

- [ ] `generate_dovetail_geometry(config) -> Mesh`
  - Genera trapecio 3D

- [ ] `add_joints_to_pieces(piece_a, piece_b, joint_config)`
  - Operación booleana: resta huecos, añade insertos
  - Detecta cara de contacto automáticamente

- [ ] `JointPlacement`
  - Manual: usuario especifica posición
  - Auto: distribuye joints uniformemente en cara

### Tests

- [ ] Test geometría dowel correcta
- [ ] Test tolerancias aplicadas
- [ ] Test boolean operations funcionan

---

## Fase 6: Integración y CLI

**Objetivo**: Fachada unificada y herramienta de línea de comandos.

### Tareas

- [ ] `PrintPrepConfig`
  ```rust
  pub struct PrintPrepConfig {
      pub build_volume: Vector3<f64>,
      pub target_scale: Option<f64>,
      pub subdivide: bool,
      pub add_joints: bool,
      pub joint_config: Option<JointConfig>,
      pub labeling: LabelingScheme,
  }
  ```

- [ ] `prepare_for_printing(mesh, config) -> PrintResult`
  - Pipeline completo
  - Retorna piezas listas + metadatos

- [ ] `PrintResult`
  ```rust
  pub struct PrintResult {
      pub pieces: Vec<LabeledPiece>,
      pub total_volume: f64,
      pub estimated_material: f64, // gramos (asumiendo densidad)
      pub assembly_info: AssemblyInfo,
  }
  ```

- [ ] Comando CLI `apps/print3d`
  - `print3d analyze model.stl`
  - `print3d scale model.stl --factor 2.0`
  - `print3d subdivide model.stl --max-size 200`
  - `print3d prepare model.stl --build-volume 220x220x250`

### Tests

- [ ] Test pipeline completo
- [ ] Test CLI básico

---

## Dependencias

```toml
[dependencies]
pinocchio-mesh = { workspace = true }
pinocchio-math = { workspace = true }
pinocchio-spatial = { workspace = true }
nalgebra = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }

# CSG operations (plane cutting, boolean ops)
# csgrs = "0.8"  # Evaluar integración
```

---

## Notas Técnicas

### Tolerancias FDM Recomendadas
- Desktop FDM: ±0.5mm mínimo
- Industrial: ±0.2mm
- Joints (dowels/dovetails): **-0.4mm** (macho más pequeño que hembra)

### Algoritmos Clave
- **Volumen**: Divergence theorem con tetraedros
- **Corte**: BSP tree o plane-triangle intersection
- **Boolean ops**: CSG con `csgrs` o implementación propia

### Referencias
- LuBan (MIT): https://github.com/nicklcz/LuBan
- TSlicer paper: Optimal slicing for Z-monotone meshes
- csgrs: https://crates.io/crates/csgrs

---

## Progreso

| Fase | Estado | Completado |
|------|--------|------------|
| 1. Análisis | ✅ Completo | 100% |
| 2. Transformaciones | ✅ Completo | 100% |
| 3. Corte/Subdivisión | ✅ Completo | 100% |
| 4. Etiquetado | ✅ Completo | 100% |
| 5. Joints | ⚠️ Geometría lista | 80% |
| 6. Integración | Pendiente | 0% |

**Tests totales: 39**

### Fase 5 - Detalles

Implementado:
- ✅ `generate_cylinder_geometry()` - Cilindros para dowels
- ✅ `generate_dovetail_geometry()` - Trapecio 3D para cola de milano
- ✅ `generate_pyramid_geometry()` - Pirámide truncada/punta para alineación
- ✅ `generate_dowel()` / `generate_pyramid()` - Wrappers con config
- ✅ `calculate_joint_positions()` - Distribución uniforme (1, 2, 3, 4, n joints)
- ✅ Configs: DowelConfig, DovetailConfig, PuzzleConfig, TerraceConfig, PyramidConfig

Pendiente:
- ⏳ `apply_joints_between_pieces()` - Requiere operaciones booleanas CSG
- ⏳ Integración con `csgrs` crate para boolean operations
