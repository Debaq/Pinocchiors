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

### 19. El embedding sigue a la plantilla, no a las proporciones de la malla
- **Dónde:** `libs/pinocchio/embedding/src/embedding.rs` (`discrete_embed`, `refine_embedding`)
- **Problema:** cada articulación toma la esfera medial más cercana a su posición en la plantilla ajustada y el refinamiento la empuja de vuelta hacia esa posición. En el humanoide de prueba, codo y muñeca quedan ~0.05 (en altura 1) más cerca del torso que en el modelo.
- **Fix:**
  - [ ] Embedding discreto real de Pinocchio (asignación sobre el grafo de esferas con penalizaciones de longitud/dirección), o ajuste por cadenas: extremidad = punto medial más lejano en la dirección del miembro y articulaciones repartidas según las longitudes de la plantilla.
  - [ ] Refinamiento que centre en el eje medial en vez de volver a la plantilla (`refine_embedding_global` existe pero no se usa).
- **Test:** endurecer tolerancias de `autorig_humanoid_joints_inside_and_in_place` (codo/muñeca < 0.03).

### 20. `DistanceField::sample` devuelve la celda más cercana
- **Dónde:** `libs/pinocchio/spatial/src/distance_field.rs` (`sample`)
- [ ] El comentario dice "trilineal" pero no interpola. Implementar interpolación trilineal (mejora `gradient()` y el test de interior).

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
- [ ] Roundtrip repair → export GLB con nodos (#4).
- [ ] Converter con transformaciones de nodos → STL/OBJ (#5).
- [ ] Fuzz/proptest de importadores con índices inválidos (#10).
- [ ] Des-ignorar los doctests de `pinocchio-core`, `quadriflow-core`, `pinocchio-repair` y `pinocchio-print3d`.
- [ ] 23 crates sin tests unitarios (wasm, CLI, desktop, varios de quadriflow).

---

## Orden sugerido

1. ~~#1 + #2 (+ test E2E)~~ ✅ — siguiente paso natural: #19 (precisión del embedding).
2. #4 + #5 — export correcto.
3. #6 + #7 + #12 — estado de la GUI.
4. #9 + #10 + #11 — robustez y seguridad.
5. #3, #8, #14 — funcionalidades incompletas.
6. Medios y limpieza.
