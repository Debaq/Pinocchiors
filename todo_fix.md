# TODO Fix — Auditoría 2026-09-26

Resultado de la auditoría del workspace. Estado base: `cargo build` y `cargo check -p pinocchio-app` limpios, 488 tests OK — los bugs de abajo **no** están cubiertos por tests.

Leyenda: 🔴 crítico · 🟠 alto · 🟡 medio · 🔵 bajo/limpieza · ✅ confirmado con probe ejecutable

---

## 🔴 Críticos — el pipeline produce resultados incorrectos

### 1. Autorig mezcla espacios de coordenadas ✅
- **Dónde:** `libs/pinocchio/core/src/autorig.rs:57`, `libs/pinocchio/mesh/src/mesh.rs:154`, `libs/pinocchio/skeleton/src/presets.rs`
- **Problema:** `normalize_bounding_box()` centra la malla en el origen (eje mayor en [-0.5, 0.5]), pero los presets están en y∈[0,1] (pelvis 0.5, cabeza 1.0). Las `bone_positions` resultantes nunca se des-normalizan. En la GUI, los esqueletos Custom (auto-fit/transform) están en coords mundo → tercer espacio distinto.
- **Evidencia:** cilindro de 170 de alto → huesos embebidos en y≈0.5–0.65; `chest`, `neck` y `head` colapsan al mismo punto.
- **Fix:**
  - [ ] Normalizar a [0,1] como el Pinocchio original (min→0, escala por eje mayor) o mover los presets al mismo espacio.
  - [ ] Guardar la transformación de normalización y aplicar la inversa a `bone_positions` y `bone_rest_transforms`.
  - [ ] Si el esqueleto viene en coords mundo (Custom), normalizarlo con la misma transformación que la malla.
- **Test:** cilindro/figura en coords mundo → huesos dentro del bbox de la malla, ordenados en Y (pelvis < chest < neck < head).

### 2. Pesos de skinning ignoran el embedding ✅
- **Dónde:** `libs/pinocchio/core/src/autorig.rs:75`, `libs/pinocchio/attachment/src/heat_diffusion.rs:188`
- **Problema:** `compute_initial_heat(skeleton)` usa las posiciones de la **plantilla**, no los huesos embebidos.
- **Evidencia:** vértice de la cabeza → hueso dominante `hip_r`.
- **Fix:**
  - [ ] Construir un esqueleto con las posiciones de `embedding.bone_positions` y pasarlo a `compute_initial_heat` / `compute_weights`.
  - [ ] Revisar que el heat inicial siga el bone heat de Pinocchio (hueso visible más cercano + resolver `(Δ + H) w = H p`), no solo `1/(1+d²)`.
- **Test:** en la figura de prueba, vértices de la cabeza → `head`/`neck`; vértices del pie → `foot_*`.

### 3. Mallas grandes: pesos no corresponden a la malla
- **Dónde:** `libs/pinocchio/core/src/autorig.rs:62` (`TODO: implementar transferencia de pesos`)
- **Problema:** con más de 100k caras (50k en `fast`) se decima la malla, y los pesos quedan con el número de vértices de la malla decimada. La GUI los aplica por índice a la original.
- **Fix:**
  - [ ] Transferir pesos a la malla original (vértice más cercano, o baricéntrico sobre el triángulo más cercano de la decimada).
  - [ ] Mientras tanto: devolver error si `attachment.num_vertices() != mesh.num_vertices()`.
- **Test:** malla > umbral → `export_weights().0.len() == mesh.num_vertices()`.

### 4. Exportar tras reparar o escalar genera un archivo vacío ✅
- **Dónde:** `apps/desktop/src/commands.rs:1791` (`mesh_to_scene`)
- **Problema:** crea una `Scene` sin `nodes` ni `root_nodes`. El GLB sale sin `nodes`/`scenes` (invisible en visores) y el USDA con `Root {}` vacío. Además se pierden materiales, UVs, normales y skins.
- **Fix:**
  - [ ] Crear un `Node` que referencie el mesh y agregarlo a `root_nodes`.
  - [ ] Mejor: modificar las posiciones de la `Scene` original en lugar de reconstruirla (conservar atributos si la topología no cambió; si cambió, al menos conservar el material).
- **Test:** reparar → `export_glb_bytes` → el JSON tiene `nodes` y `scenes`.

### 5. Transformaciones de nodos ignoradas en todo el proyecto
- **Dónde:** `libs/pinocchio/mesh/src/adapter.rs:18`, `apps/desktop/src/commands.rs:316` (`get_mesh_data`), `libs/converter/stl/src/export.rs:31`, `libs/converter/obj/src/export.rs:55`
- **Problema:** todos iteran `scene.meshes` directamente. GLB con escala 0.01, rotación Z-up o varios nodos → geometría incorrecta en el visor, autorig y STL/OBJ. Las instancias se pierden.
- **Fix:**
  - [ ] Agregar `Scene::world_transforms() -> Vec<Mat4>` (recorriendo desde `root_nodes`) y `Scene::flattened_meshes()` en `converter-scene`.
  - [ ] Usarlos en `scene_to_mesh`, `get_mesh_data` y en los exportadores STL/OBJ (transformar también las normales).
- **Test:** escena con un nodo escalado ×2 y trasladado → STL exportado con bbox esperado.

---

## 🟠 Altos

### 6. Flag `processing` queda pegado
- **Dónde:** `apps/desktop/src/commands.rs:882-894`, `1051-1059`
- **Problema:** hace `swap(true)` y después retorna con `?` si falta la malla o el esqueleto → desde ahí todo autorig o retopología responde "Ya hay un proceso en curso" hasta reiniciar la app. Un panic dentro tiene el mismo efecto.
- **Fix:**
  - [ ] Guard RAII (`struct ProcessingGuard<'a>(&'a AtomicBool)` con `Drop` que haga `store(false)`).
  - [ ] Ejecutar el trabajo pesado en `tauri::async_runtime::spawn_blocking`.

### 7. Auto-fit y edición de huesos se pierden al transformar
- **Dónde:** `apps/desktop/src/commands.rs:634` (`transform_skeleton`), `725` (`move_bone`), `782` (`auto_fit_skeleton`)
- **Problema:** `transform_skeleton` siempre parte de `original_skeleton`, que auto-fit y `move_bone` no actualizan. Ejemplo: auto-fit → cambiar escala → el esqueleto vuelve al preset.
- **Fix:**
  - [ ] `auto_fit_skeleton` y `move_bone` deben actualizar `original_skeleton` (nueva base), o
  - [ ] Separar "base editada" y "transformación de gizmo" en el estado.

### 8. Retopología y rig no se pueden exportar
- **Dónde:** `apps/desktop/src/commands.rs:420` (`export_model`), `116-117`
- **Problema:** `quad_mesh` solo se visualiza. `include_skeleton` e `include_weights` no se leen en ninguna parte. Solo existe export JSON de pesos.
- **Fix:**
  - [ ] Opción "usar malla retopologizada" → convertir `QuadMesh` a `Scene` (triangulada para GLB; quads para OBJ).
  - [ ] Escribir skin en la `Scene` (`Skeleton` + `JointIndices`/`JointWeights`) desde `PinocchioOutput` para GLB/USDZ.

### 9. `scale_to_volume` con volumen 0 → NaN/inf
- **Dónde:** `libs/pinocchio/print3d/src/transform.rs:80`
- **Fix:**
  - [ ] Validar `current_volume > EPS && target_volume > 0`; devolver `Result`.
  - [ ] En `scale_to_fit` (`:53`) proteger contra `dims[i] == 0`.

### 10. Índices fuera de rango → panic
- **Dónde:** `libs/pinocchio/mesh/src/mesh.rs:29` (`from_triangles`), `libs/converter/scene/src/scene.rs` (`validate`)
- **Problema:** no se validan los índices; `Scene::validate()` no se llama en producción y tampoco revisa índices de vértices. Un GLB malformado tumba la app.
- **Fix:**
  - [ ] `validate()` debe revisar `index < positions.len()` y el largo de los atributos por primitiva.
  - [ ] Llamar `validate()` al final de cada importador (o en `converter_core::import`).
  - [ ] `from_triangles` → `try_from_triangles` que devuelva `Result`.

### 11. Export OBJ: escritura fuera del directorio con nombres de textura
- **Dónde:** `libs/converter/obj/src/export.rs:196-206`
- **Problema:** el nombre viene del archivo importado; un nombre absoluto (`/home/x/algo`) escribe `/home/x/algo.png`. Texturas con el mismo nombre se sobrescriben. Solo se exporta `map_Kd`.
- **Fix:**
  - [ ] Sanear: solo `file_stem` de `Path::new(name).file_name()`, filtrar a `[A-Za-z0-9_-]`, y si queda vacío usar `{stem}_tex_{i}`.
  - [ ] Nombres únicos por índice de textura.
  - [ ] Exportar también `map_Bump`/`norm`, `map_Pr`/`map_Pm` (PBR MTL) cuando existan.

---

## 🟡 Medios

### 12. `import_model` no limpia el estado anterior
- **Dónde:** `apps/desktop/src/commands.rs:300-310`
- [ ] Resetear `quad_mesh`, `diagnostics`, `mesh_before_repair`, `scene_before_repair`, `print3d_pieces`, `mesh_before_print_scale`.
- [ ] Agregar un método `AppState::reset_derived()`.

### 13. UVs desalineadas en `get_mesh_data`
- **Dónde:** `apps/desktop/src/commands.rs:369-382`
- [ ] Si alguna primitiva tiene UVs, rellenar con `[0,0]` las que no tienen.
- [ ] Generar normales reales en lugar de las dummy `(0,1,0)` (`:362`).

### 14. Mallas GLB sin soldar (costuras de UV)
- **Dónde:** `libs/pinocchio/mesh/src/adapter.rs`
- [ ] Soldar por posición (con tolerancia) para autorig y reparación, guardando el mapeo `original → soldado` para devolver los pesos por vértice original.

### 15. `partial_cmp().unwrap()` hace panic con NaN
- [ ] `libs/pinocchio/repair/src/repair/normals.rs:163`
- [ ] `libs/quadriflow/parametrizer/src/integer.rs:140`
- [ ] `libs/converter/usda/src/skeleton.rs:506`
- **Fix:** usar `f64::total_cmp`.

### 16. `subdivide` (print3d)
- **Dónde:** `libs/pinocchio/print3d/src/slicer.rs:623`, `742`, `785`
- [ ] Error explícito si `margin * 2 >= build_volume[i]` (hoy devuelve una sola pieza sin avisar).
- [ ] `calculate_z_planes` sin guarda `max_height <= 0` → `inf as usize` → loop enorme / OOM.
- [ ] Límite máximo de planos (p. ej. 1000) → error.
- [ ] Considerar `scene.meters_per_unit` (modelo en metros vs volumen de impresión en mm).

### 17. Progreso del autorig falso y bloqueo del runtime
- **Dónde:** `apps/desktop/src/commands.rs:920-942`
- [ ] Callback de progreso real en `autorig` (como `remesh_with_callback`).
- [ ] `spawn_blocking` (ver #6).

### 18. Strip/fan inválido → índices `None`
- **Dónde:** `libs/converter/gltf-io/src/import.rs:445-460`
- [ ] Si la conversión queda vacía, saltar la primitiva en vez de dejar `indices: None` (hoy se interpreta como lista de triángulos implícita).

---

## 🔵 Bajos / limpieza

- [ ] `libs/quadriflow/optimizer/src/pool.rs:127,134` — `transmute` a `'static` sobre un `thread_local`, unsound. No se usa: borrar `get_f64_buffer`/`get_usize_buffer` o cambiarlos por una API con closure (`with_f64_buffer(|buf| …)`).
- [ ] CLI `apps/converter/src/main.rs`:
  - [ ] la ayuda dice que `output` es opcional, pero el modo normal falla sin él → derivarlo de `--format`;
  - [ ] `--batch` no procesa varios archivos → aceptar varios inputs/glob;
  - [ ] `--format` se ignora fuera de `--batch`.
- [ ] `mesh_before_print_scale` se guarda pero no existe comando `undo_print_scale`.
- [ ] `libs/converter/usdz/` es una carpeta vacía fuera del workspace → borrar.
- [ ] `apps/web` tiene `package-lock.json` y `pnpm-lock.yaml` → elegir uno.
- [ ] `Cargo.toml`: `repository` apunta a `pinocchio-rs/pinocchio` → `Debaq/Pinocchiors`.
- [ ] Agregar `LICENSE-MIT` y `LICENSE-APACHE`.
- [ ] Clippy: 3 errores `approx_constant` (`libs/pinocchio/graph/src/pt_graph.rs:157-160`, `3.14` en un test) y unos 100 lints de estilo (`cargo clippy --fix`).
- [ ] Frontend sin verificar: `npm install && npx tsc --noEmit` en `apps/web`.

---

## Tests que faltan

- [ ] E2E autorig: figura en coords mundo → posiciones de huesos dentro de la malla y huesos dominantes correctos (cubre #1, #2).
- [ ] Autorig con malla > umbral de decimación (#3).
- [ ] Roundtrip repair → export GLB con nodos (#4).
- [ ] Converter con transformaciones de nodos → STL/OBJ (#5).
- [ ] Fuzz/proptest de importadores con índices inválidos (#10).
- [ ] Des-ignorar los doctests de `pinocchio-core`, `quadriflow-core`, `pinocchio-repair` y `pinocchio-print3d`.
- [ ] 23 crates sin tests unitarios (wasm, CLI, desktop, varios de quadriflow).

---

## Orden sugerido

1. #1 + #2 (+ test E2E) — el autorig es el núcleo del proyecto.
2. #4 + #5 — export correcto.
3. #6 + #7 + #12 — estado de la GUI.
4. #9 + #10 + #11 — robustez y seguridad.
5. #3, #8, #14 — funcionalidades incompletas.
6. Medios y limpieza.
