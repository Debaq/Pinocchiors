# Plan: frontend más ágil (rendimiento)

2026-11-08. Sale de apps/ESTUDIO_RENDIMIENTO.md (léelo para el detalle y los
archivos). Empieza por el diseño CAD, que es lo que más trabajo de más hace en
cada cambio. Regla: en el PC se compilan y prueban solo los motores (crates
`libs/`); lo de la app (`apps/desktop`, `apps/web`) lo compila GitHub con la
release.

## Fases

- **R1 Huella sin copiar** (libs/cad/model). `Document::geometry_hash` clonaba
  el documento entero (con sus versiones guardadas, que son documentos
  completos) y lo pasaba a un texto JSON en cada llamada. Ahora se escribe
  directo al hash desde una vista prestada, sin copia ni texto.
- **R2 Evaluar sin rearmar el resultado** (apps/desktop/src/cad.rs). Número de
  versión del documento como clave rápida; `ensure_evaluated` separado de armar
  el `CadResult` (27 comandos lo armaban y lo botaban); la malla de la vista
  guardada por versión (hoy se re-tesela en cada `cad_mesh`).
- **R3 Una llamada por cambio** (cad.ts + cad.rs). `set_document` → `cad_mesh`
  → `cad_tool_mesh` en serie: juntar en una respuesta, o al menos en paralelo.
- **R4 Caras y aristas en lote**. `cad_face_refs` / `cad_edge_refs` /
  `cad_face_edge_refs` con listas; designActions.ts y CadView.tsx los usan.
- **R5 Pasar el mouse por el árbol sin backend**. La operación de cada cara
  viaja con la malla; el resaltado se resuelve en el front.
- **R6 Vecinos e influencia en Rust**. `neighborsOf` (Viewer3D.ts) e
  `estimateInfluence` (boneInfluence.ts) como comandos binarios con rayon.
- **R7 Datos binarios**. Zona de la piel (máscara binaria + resumen), texturas
  solo las que cambiaron, `cad_write_bytes` como cuerpo crudo.
- **R8 Ensambles y planos**. Explosión en el visor; caché por vista en planos.
- **R9 Visor**. Pincel de pesos con BVH; posar con skinning en GPU; nubes
  grandes con recoloreo incremental.

## Avance

- 2026-11-08: plan escrito.
- R1 escrito: `geometry_hash` con vista prestada + escritura directa al hash
  (libs/cad/model/src/document.rs); prueba `geometry_hash_follows_geometry_only`
  en tests/cache.rs (corriendo en el PC; resultado en
  ~/.cargo-target/bench/mallas/tests_r1.txt).
- R2 escrito (app, sin compilar en el PC: lo compila la release):
  `refresh_cache` devuelve la caché tomada; `ensure_evaluated` /
  `ensure_committed` reemplazan 27 `evaluate(..)?;` que botaban el resultado;
  `VIEW_MESH` guarda la malla de la vista por (huella, escala).
- R1 PROBADO en el PC: cache.rs 3 OK (incluye la nueva) + tabs.rs 18 OK.
- R3 escrito (cad.ts `sendNow`): malla y herramienta en paralelo
  (`Promise.all`), no en serie. La fusión en una sola respuesta queda para
  después si hace falta.
- R4 escrito: comandos `cad_face_refs` / `cad_edge_refs` (cad.rs, registrados
  en lib.rs); en cad.ts `faceRef` / `edgeRef` pasan por `batchedRefs`, que
  junta los pedidos del mismo instante en una llamada (los 35 lugares que los
  usan no cambian). `cad_face_ref` / `cad_edge_ref` de a uno siguen (los usan
  e2e y el conector MCP).
- R2–R4 sin compilar en el PC (app): los revisa la release.
- R5 escrito (versión simple): `featureFaces` en cad.ts recuerda la respuesta
  por (versión del diseño, operación); pasar el mouse otra vez por la misma
  fila no llama al backend. Mandar la operación de cada cara con la malla
  (sin backend del todo) queda para después.
- Siguiente: R6 (vecinos e influencia de huesos en Rust).
