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

### 3. Mallas grandes: pesos no corresponden a la malla ✅ RESUELTO
- [x] Con decimación, los pesos se transfieren a cada vértice original: punto más cercano de la malla decimada (BVH) + interpolación baricéntrica.
- [x] El `Attachment` usa siempre las posiciones y el número de vértices de la malla original.
- **Test:** `autorig_decimated_mesh_transfers_weights_to_original` (decimación forzada, regiones > 90 %).

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

### 8. Retopología y rig no se pueden exportar ✅ RESUELTO
- [x] Opción "Usar malla retopologizada": `QuadMesh` → `Scene` (triangulada) para cualquier formato.
- [x] Opción "Incluir esqueleto y pesos": escena con skin (`Skeleton` + `JointIndices`/`JointWeights`, hasta 4 influencias). Un joint por hueso en la cabeza de su segmento (como Blender); conserva materiales, normales y UVs.
- [x] Rig + retopología: los pesos se transfieren a la malla de quads con `pinocchio_core::transfer_weights` (ahora pública).
- [x] Reparar, deshacer reparación o escalar para imprimir invalidan el rig (antes se podían exportar pesos de otra malla); además se verifica que el número de pesos coincida con los vértices.
- [x] Frontend: dos casillas en el paso de exportación.
- **Tests:** GLB con skin ida y vuelta (joints, pesos normalizados), USDA con UsdSkel, malla de quads triangulada.
- **Pendiente:** OBJ/STL no tienen rig (esperado); el JSON de pesos sigue siendo solo de la malla original.

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

### 14. Mallas GLB sin soldar (costuras de UV) ✅ RESUELTO
- [x] `Mesh::welded(tolerance)` → malla soldada + mapeo original → soldado.
- [x] `autorig` suelda antes de decimar/embeber/calcular pesos y devuelve los pesos por vértice original: los duplicados de una costura reciben exactamente los mismos pesos.
- **Test:** `autorig_unwelded_mesh_gets_identical_weights_on_seams` (humanoide con todos los triángulos separados).

### 15. `partial_cmp().unwrap()` hace panic con NaN ✅ RESUELTO
- [x] `repair/normals.rs`, `parametrizer/integer.rs` y `usda/skeleton.rs` usan `total_cmp`.

### 16. `subdivide` (print3d) ✅ RESUELTO (salvo unidades)
- [x] Error `InvalidConfig` si el margen no deja volumen útil (antes devolvía una pieza sin avisar).
- [x] `calculate_z_planes` protege contra altura ≤ 0.
- [x] Se cuentan los planos necesarios **antes** de generarlos; más de `MAX_CUT_PLANES` (1000) → error que sugiere revisar unidades.
- [x] Unidades: STL importado en mm (`meters_per_unit = 0.001`), STL exportado siempre en mm, GLB exportado en metros (glTF). El panel de impresión convierte mm ↔ unidades de la escena (análisis, escalado, volumen de impresión, margen, piezas).

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
- [x] El primer joint de cada cadena se ancla a la proyección de su plantilla sobre el camino (mejora leve en brazos).
- **Limitación conocida:** en el humanoide sintético la cadera queda en y≈0.42 (el muslo nace dentro del torso) y la rodilla ~0.03 baja: el reparto por proporciones de la plantilla no conoce la anatomía del modelo.

### 20. `DistanceField::sample` devuelve la celda más cercana ✅ RESUELTO
- [x] Interpolación trilineal entre centros de celda (sin NaN con celdas infinitas). **Test:** reproduce un campo lineal exacto.

---

## 🔵 Bajos / limpieza

- [x] `quadriflow/optimizer/src/pool.rs`: el `transmute` a `'static` se reemplazó por `with_f64_buffer(|buf| …)` / `with_usize_buffer`.
- [x] CLI `apps/converter`:
  - [x] salida opcional con `--format` (y error claro si sobrescribiría la entrada);
  - [x] `--batch` acepta varios archivos, `--out-dir`, resumen y código de salida 1 si alguno falla;
  - [x] `--format` también sin `--batch` (validado contra la extensión de la salida). Tests del plan de conversión.
- [x] Comando `undo_print_scale` + botón "Deshacer escala" (restaura malla y escena).
- [x] `libs/converter/usdz/` (carpeta vacía) borrada.
- [x] `apps/web`: se quitó `pnpm-lock.yaml`; queda `package-lock.json` (lo usan `tauri.conf.json` y CI).
- [x] `Cargo.toml`: `repository` → `https://github.com/Debaq/Pinocchiors`.
- [x] `LICENSE-MIT` y `LICENSE-APACHE` (texto oficial).
- [x] Clippy: errores `approx_constant` del test de `pt_graph` corregidos.
- [ ] Clippy: ~100 lints de estilo. `cargo clippy --fix` propone APIs de Rust ≥ 1.88 (`as_chunks`, let chains) y el workspace declara 1.85: decidir si subir `rust-version`.
- [x] Frontend: `npx tsc --noEmit` pasa.
- [x] CI: el job de tests instala las dependencias de Tauri y corre también `pinocchio-app`.

---

## Tests que faltan

- [x] E2E autorig: figura en coords mundo → posiciones de huesos dentro de la malla y huesos dominantes correctos (cubre #1, #2).
- [x] Autorig con malla > umbral de decimación (#3).
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
5. ~~#3 + #14~~ ✅
6. ~~#8~~ ✅
4. #9 + #10 + #11 — robustez y seguridad.
5. #3, #8, #14 — funcionalidades incompletas.
6. Medios y limpieza.
