# TODO Fix — Auditoría 2026-09-26

Resultado de la auditoría del workspace. Estado base: `cargo build` y `cargo check -p pinocchio-app` limpios, 488 tests OK — los bugs de abajo **no** están cubiertos por tests.

Leyenda: 🔴 crítico · 🟠 alto · 🟡 medio · 🔵 bajo/limpieza · ✅ confirmado con probe ejecutable

---

## 🔴 Críticos — el pipeline produce resultados incorrectos

### 1. Autorig mezcla espacios de coordenadas ✅ RESUELTO
- **Dónde:** `libs/pinocchio/core/src/autorig.rs`, `libs/pinocchio/skeleton/src/skeleton.rs`, `libs/pinocchio/embedding/`, `libs/pinocchio/spatial/`
- **Problema original:** malla normalizada centrada en el origen y presets en y∈[0,1]; `bone_positions` nunca se des-normalizaban.
- **Causas adicionales encontradas al arreglarlo:**
  - `DistanceField` sin signo: el test `dist > 0 ⇒ dentro` del embedding siempre era verdadero y los huesos salían de la malla.
  - Esferas mediales: muestreo de 10³ puntos con test de máximo local a ±0.01 sobre un campo de celda más cercana (~0.04) → casi todos los puntos, incluso fuera de la malla, pasaban.
  - `point_triangle_distance` (BVH y DistanceField) con signos mezclados en las regiones de arista → distancias incorrectas.
- **Hecho:**
  - [x] `Normalization` explícita en `autorig`; posiciones de huesos, `bone_rest_transforms`, `embedding` y posiciones de reposo del `Attachment` vuelven a coordenadas originales.
  - [x] `SkeletonFit { Auto, None }` en `PinocchioConfig`: `Auto` encaja la plantilla en la malla con `pinocchio_skeleton::fit_to_bounds`; `None` respeta un esqueleto ya colocado (GUI: esqueletos Custom).
  - [x] `DistanceField::from_mesh_signed` (flood fill + normal del triángulo más cercano, corrige normales invertidas; soporta cáscaras superpuestas).
  - [x] `medial_spheres_from_field`: eje medial extraído de la grilla con signo.
  - [x] `closest_point_on_triangle` (Ericson) + `Bvh::query_closest` + `Bvh::segment_intersects`.
  - [x] El pipeline usa `config.distance_field_resolution` (antes fijo en 32³).
  - [x] GUI: `auto_fit_skeleton` usa `fit_to_bounds` (mismo criterio que autorig).
- **Tests:** `libs/pinocchio/core/tests/autorig_e2e.rs` (humanoide sintético con surface nets, 170 de alto y desplazado): articulaciones dentro de la malla y cerca de las reales, columna ordenada, deformación en reposo = identidad, `SkeletonFit::None`.

### 2. Pesos de skinning ignoran el embedding ✅ RESUELTO
- **Dónde:** `libs/pinocchio/attachment/src/heat_diffusion.rs`, `libs/pinocchio/sparse/src/spd_matrix.rs`
- **Problema original:** `compute_initial_heat` usaba la plantilla; además el heat `1/(1+d²)` casi no discriminaba entre huesos en espacio normalizado.
- **Causa adicional:** `solve_gauss_seidel`/`solve_with_identity` recorrían la matriz entera por cada fila → O(n·nnz) por iteración (horas con 10k vértices). También afectaba a `quadriflow-field`.
- **Hecho:**
  - [x] `autorig` calcula los pesos con el esqueleto **embebido**.
  - [x] Bone heat de Baran & Popović: hueso visible más cercano (ray casting con BVH), `(L + A·H) w = A·H p`, `H = k/(c·d²)`. Partición de la unidad por construcción, invariante a la escala.
  - [x] `SPDMatrix::solve_cg` (gradiente conjugado + Jacobi) y `add_diagonal`; Gauss-Seidel recorre solo la columna i (O(nnz)).
  - [x] API: `HeatDiffusion::compute_weights(&skeleton)` (se eliminó `compute_initial_heat`).
- **Tests:** cadena en cilindro (huesos dominantes por altura), invariancia de escala (cilindro 1 vs 170), CG contra residuo, pesos por región del humanoide (cabeza, muslo, piernas, brazo, antebrazos, mano) > 95 %.

### 3. Mallas grandes: pesos no corresponden a la malla
- **Dónde:** `libs/pinocchio/core/src/autorig.rs:62` (`TODO: implementar transferencia de pesos`)
- **Problema:** con más de 100k caras (50k en `fast`) se decima la malla, y los pesos quedan con el número de vértices de la malla decimada. La GUI los aplica por índice a la original.
- **Fix:**
  - [ ] Transferir pesos a la malla original (vértice más cercano, o baricéntrico sobre el triángulo más cercano de la decimada).
  - [ ] Mientras tanto: devolver error si `attachment.num_vertices() != mesh.num_vertices()`.
- **Test:** malla > umbral → `export_weights().0.len() == mesh.num_vertices()`.

### 4. Exportar tras reparar o escalar genera un archivo vacío ✅ RESUELTO
- **Dónde:** `apps/desktop/src/commands.rs` (`mesh_to_scene`, `scale_scene_about`)
- **Hecho:**
  - [x] `mesh_to_scene` crea nodo raíz + `root_nodes`, normales por vértice y conserva `meters_per_unit`/`y_up`.
  - [x] Escalar para imprimir ya no reconstruye la escena: envuelve las raíces en un nodo `T(c)·S(s)·T(-c)` y conserva materiales, UVs y texturas. Si la escena tiene esqueletos, se exporta geometría estática (los joints no cuelgan de `scene.nodes`).
  - [x] Factor realmente aplicado calculado del bbox (`scale_to_fit` solo reduce).
- **Tests:** `apps/desktop` → GLB tras reparar tiene `nodes` y `scenes`; escalado conserva material y centro.
- **Pendiente menor:** tras reparar se pierden materiales/UVs (la topología cambia).

### 5. Transformaciones de nodos ignoradas en todo el proyecto ✅ RESUELTO
- **Hecho:**
  - [x] `converter-scene`: `Scene::world_transforms()`, `mesh_instances()` y `world_primitives()` (normales con matriz normal, winding invertido si det < 0, mallas con skin en espacio de bind, ciclos e índices inválidos ignorados). `compute_bounding_box` ahora es en espacio mundo. Re-export de `glam`.
  - [x] STL y OBJ exportan en espacio mundo con instancias; STL usa la normal de la cara (antes la del primer vértice).
  - [x] `scene_to_mesh` (pinocchio) y la GUI (`get_mesh_data`, stats, bbox) usan el mismo recorrido → los índices de vértice coinciden con los pesos.
- **Tests:** anidado + instancias, espejo, skin, ciclos/índices inválidos, STL y OBJ con nodo transformado, GUI vs pinocchio en el mismo espacio y orden.

---

## 🟠 Altos

### 6. Flag `processing` queda pegado ✅ RESUELTO
- [x] `AppState::try_begin_processing()` devuelve un `ProcessingGuard` que libera el flag en `Drop` (errores, `?` tempranos y panics).
- [x] `run_autorig` y `run_retopology` corren en `tauri::async_runtime::spawn_blocking`; un panic se reporta como error.
- **Test:** `processing_guard_releases_on_drop`.

### 7. Auto-fit y edición de huesos se pierden al transformar ✅ RESUELTO
- [x] Modelo explícito: esqueleto visible = `gizmo(base)`. `original_skeleton` es la base y `skeleton_transform` guarda los parámetros del gizmo.
- [x] `auto_fit_skeleton` fija la base y resetea el gizmo; `move_bone` escribe en la base con la inversa del gizmo, así la edición sobrevive a cambios de escala/rotación.
- **Test:** `gizmo_inverse_roundtrip`.

### 8. Retopología y rig no se pueden exportar
- **Dónde:** `apps/desktop/src/commands.rs:420` (`export_model`), `116-117`
- **Problema:** `quad_mesh` solo se visualiza. `include_skeleton` e `include_weights` no se leen en ninguna parte. Solo existe export JSON de pesos.
- **Fix:**
  - [ ] Opción "usar malla retopologizada" → convertir `QuadMesh` a `Scene` (triangulada para GLB; quads para OBJ).
  - [ ] Escribir skin en la `Scene` (`Skeleton` + `JointIndices`/`JointWeights`) desde `PinocchioOutput` para GLB/USDZ.

### 9. `scale_to_volume` con volumen 0 → NaN/inf ✅ RESUELTO
- [x] `scale_to_volume` y `scale_to_fit` devuelven `Result`: volumen ≤ 0 → `MeshNotClosed`, objetivo/tamaño inválido → `InvalidConfig`; los ejes planos no limitan `scale_to_fit`.
- [x] GUI: valida el factor uniforme (> 0, finito) y muestra el error.

### 10. Índices fuera de rango → panic ✅ RESUELTO
- [x] `Scene::world_primitives` descarta triángulos con índices fuera de rango.
- [x] `Scene::validate_geometry()`: posiciones presentes, un elemento por vértice en cada atributo, índices válidos. `validate()` la incluye.
- [x] Los importadores glTF (archivo y bytes) y OBJ la llaman antes de devolver la escena (cubre converter-core, GUI y WASM).
- [x] `Mesh::try_from_triangles`; `from_triangles` documenta el panic.

### 11. Export OBJ: escritura fuera del directorio con nombres de textura ✅ RESUELTO
- [x] Nombre de archivo = solo el nombre base saneado a `[A-Za-z0-9_-]`; vacío → `{stem}_tex_{i}`.
- [x] Cada textura se escribe una vez; colisiones → `{nombre}_{i}`.
- [x] MTL exporta también `norm`/`map_Bump` (normales), `map_Ke` y `Ke` (emisivo).
- **Test:** rutas absolutas y `../` no escriben fuera del directorio; nombres repetidos no se pisan.
- **Pendiente:** metallic-roughness (glTF lo empaqueta en canales G/B; MTL espera mapas separados).

---

## 🟡 Medios

### 12. `import_model` no limpia el estado anterior ✅ RESUELTO
- [x] `AppState::reset_derived()` (resultado, quad mesh, diagnósticos, backups, piezas) llamado al importar.
- [x] Frontend: al importar también limpia la vista de la malla de quads.
- **Test:** `reset_derived_clears_previous_model_state`.

### 13. UVs desalineadas en `get_mesh_data` ✅ RESUELTO (con #5)
- **Dónde:** `apps/desktop/src/commands.rs:369-382`
- [x] Si alguna primitiva tiene UVs, rellenar con `[0,0]` las que no tienen.
- [x] Generar normales reales en lugar de las dummy `(0,1,0)`.

### 14. Mallas GLB sin soldar (costuras de UV)
- **Dónde:** `libs/pinocchio/mesh/src/adapter.rs`
- [ ] Soldar por posición (con tolerancia) para autorig y reparación, guardando el mapeo `original → soldado` para devolver los pesos por vértice original.

### 15. `partial_cmp().unwrap()` hace panic con NaN ✅ RESUELTO
- [x] `repair/normals.rs`, `parametrizer/integer.rs` y `usda/skeleton.rs` usan `total_cmp`.

### 16. `subdivide` (print3d) ✅ RESUELTO (salvo unidades)
- [x] Error `InvalidConfig` si el margen no deja volumen útil (antes devolvía una pieza sin avisar).
- [x] `calculate_z_planes` protege contra altura ≤ 0.
- [x] Se cuentan los planos necesarios **antes** de generarlos; más de `MAX_CUT_PLANES` (1000) → error que sugiere revisar unidades.
- [ ] Considerar `scene.meters_per_unit` (requiere definir las unidades de STL y del volumen en la GUI).

### 17. Progreso del autorig falso y bloqueo del runtime ✅ RESUELTO
- [x] `pinocchio_core::autorig_with_progress` con `AutorigStage` (preparing, embedding, weights, done) emitidas al comenzar cada etapa real.
- [x] `spawn_blocking` (ver #6).

### 18. Strip/fan inválido → índices `None` ✅ RESUELTO
- [x] Si la conversión queda vacía se omite la primitiva.

### 19. El embedding sigue a la plantilla, no a las proporciones de la malla ✅ RESUELTO
- **Dónde:** `libs/pinocchio/embedding/src/chain.rs` (nuevo)
- **Hecho:**
  - [x] Embedding por cadenas: Dijkstra sobre la grilla interior con costo `longitud/d²` (sigue el eje medial); extremo de cada extremidad = punto medial más lejano de su región; articulaciones repartidas según las proporciones de la plantilla.
  - [x] `quality_score` invariante a la escala (proporciones de huesos).
  - [x] `max_medial_spheres` y `refine_iterations` quedan sin efecto (documentado).
- **Resultado (humanoide de prueba, altura 1):** codo, muñeca y mano a < 0.01 de las reales (antes ~0.05–0.08). Se adapta a pose T con la plantilla en pose A.
- **Tests:** tolerancias endurecidas a 0.03 en brazos; regiones anatómicas originales; `autorig_adapts_to_t_pose`.
- **Pendiente menor:** en las piernas la cadera queda ~0.05 baja porque la plantilla tiene el hueso pelvis→cadera más largo que el modelo.

### 20. `DistanceField::sample` devuelve la celda más cercana ✅ RESUELTO
- [x] Interpolación trilineal entre centros de celda (sin NaN con celdas infinitas). **Test:** reproduce un campo lineal exacto.

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

- [x] E2E autorig: figura en coords mundo → posiciones de huesos dentro de la malla y huesos dominantes correctos (cubre #1, #2).
- [ ] Autorig con malla > umbral de decimación (#3).
- [x] Roundtrip repair → export GLB con nodos (#4).
- [x] Converter con transformaciones de nodos → STL/OBJ (#5).
- [ ] Fuzz/proptest de importadores con índices inválidos (#10).
- [ ] Des-ignorar los doctests de `pinocchio-core`, `quadriflow-core`, `pinocchio-repair` y `pinocchio-print3d`.
- [ ] 23 crates sin tests unitarios (wasm, CLI, desktop, varios de quadriflow).

---

## Orden sugerido

1. ~~#1 + #2 (+ test E2E)~~ ✅
2. ~~#4 + #5 + #19~~ ✅
3. ~~#6 + #7 + #12 + #17~~ ✅
4. ~~#9 + #10 + #11 + #15 + #16 + #18 + #20~~ ✅
4. #9 + #10 + #11 — robustez y seguridad.
5. #3, #8, #14 — funcionalidades incompletas.
6. Medios y limpieza.
