# Plan: restas y uniones con mallas grandes (sin OCCT)

2026-11-08. Probando por el conector MCP, restar un texto de 1 mm al hueso en
«Detalle completo» (STL de 163 000 triángulos) tardó más de 20 minutos, con un
solo núcleo y 4 GB de memoria. Solo convertir ese STL en pieza ya tardaba más
de 2 minutos. No se puede trabajar así.

## Por qué

Cada pieza del diseño es una forma de OCCT. Un STL como pieza es una forma con
una cara por triángulo: 163 000 caras. OCCT está hecho para piezas de pocas
caras exactas (una caja: 6) y en cada booleana revisa y rearma la pieza entera,
aunque lo que se toca sea un rincón. Además convertir, validar y teselar para
la vista también recorren las 163 000 caras.

## La idea

Las piezas que nacen de una malla (escaneo en «Detalle completo», cuerpo del
escaneo por vóxeles) se guardan **como malla**, no como forma OCCT. Las
booleanas donde participa una pieza de malla se hacen sobre triángulos con
**manifold-rust** (Rust puro, port de Manifold —el motor de OpenSCAD y
Blender— con un motor robusto para escaneos con fallas, paralelo con rayon,
cancelación y avance). La herramienta OCCT (el texto, un cilindro) se tesela y
entra a la booleana como malla. Resta un sólido de 2 M de triángulos en ~2,5 s.

Se compila igual para Linux, Windows, macOS y Android: sin C++, sin cmake.

## Fases

- **M1 Medir.** Banco con el hueso: convertir a pieza, validar, teselar,
  restar el texto con OCCT, y lo mismo con manifold-rust. Confirma dónde se va
  el tiempo y cuánto se gana. Meta: el grabado en menos de 10 s.
- **M2 Pieza de malla.** `Part` gana `mesh: Option<Arc<MeshSolid>>` (vértices
  y triángulos, cerrada). Su `shape` OCCT pasa a ser un sustituto liviano
  (la malla simplificada a unos miles de triángulos) solo para lo que necesita
  OCCT: saber qué toca una herramienta, distancias, elegir la pieza.
- **M3 Booleanas por malla.** En `apply` (unir, restar, intersecar), si la
  pieza o la herramienta es de malla: booleana con manifold-rust (motor
  `Auto`: exacto si las dos están limpias, robusto si no). Las piezas que
  salgan sueltas se separan como hoy y sin astillas.
- **M4 Lo que se ve y lo que sale.** La vista, el volumen, la masa, exportar
  STL/3MF, «pasar a modelo» e imprimir usan la malla directo (sin teselar).
  STEP: se arma la forma OCCT entera al exportar, con aviso de que tarda.
- **M5 Operaciones sobre piezas de malla.** Mover, girar, copiar, simetría y
  patrón mueven la malla. Relieve y grabado sobre la piel, con la pieza de
  malla. Lo que necesita caras exactas (redondeo, chaflán, vaciado sobre caras
  de la malla) da un error claro: «no se puede en una pieza de malla».
- **M6 El escaneo como pieza de malla.** `ScanBody` (detalle completo y
  vóxeles) arma la pieza de malla directo: sin convertir a OCCT, instantáneo.
- **M7 Pruebas.** Pruebas de booleanas de malla en el crate, y el hueso de
  punta a punta por el conector: reparar → detalle completo → grabado →
  exportar, con tiempos.

## Avance

- 2026-11-08 M1 (banco `cargo run --release -p cad-scan --example mallas_booleanas
  -- malla.stl [--occt]`; mallas y resultados en `~/.cargo-target/bench/mallas`):
  hueso reparado de 163 658 triángulos, resta de 5 cilindros (2 500 triángulos):
  malla → manifold 0,36 s, **resta 1,52 s**, de vuelta a malla 0,008 s. En la app
  con OCCT el grabado tardó ~29 min.
- Hallazgo: `mesh_shell_direct` (cad_occt.cpp) se rendía si UN triángulo tenía
  normal < tolerancia² (el hueso tiene uno) y caía al cosido de OCCT (minutos).
  Arreglado: usa la normal igual y solo si es nula la de sus vecinos. Falta medir
  cuánto queda la conversión (se esperaba ~1 s cada 20 000 triángulos).
- M2/M3 escritos (sin commitear, compilan): `libs/cad/model/src/mesh_bool.rs`
  (3 pruebas OK), `Part.mesh` (eval.rs; se borra donde OCCT cambia la forma),
  `apply` → `apply_mesh` → `apply_by_mesh` (pieza de malla, o herramienta de
  malla con > `MESH_BOOLEAN_FACES`), `touching` por cajas para piezas de malla,
  ScanBody pasa su malla (salvo parche/pincel). Prueba de punta a punta:
  `libs/cad/model/tests/mesh_parts.rs` (falta correrla).
- `tests/mesh_parts.rs`: 3 pruebas OK (pieza de malla, restar/partir/unir, CAD sigue por OCCT).
- Banco con el arreglo (`~/.cargo-target/bench/mallas/banco2.txt`), hueso 163 k:
  resta manifold 1,6 s; OCCT convertir 10 s (antes minutos), **validar 50 s,
  volumen 18 s, teselar para la vista 14 s**. La app hace las tres en cada cambio.
- M4 escrito: `mesh_bool::mass` (Eberly), `combine_mass`, `components` (pruebas
  contra OCCT OK; `to_shape` deja las caras en el orden de los triángulos, probado).
  apps/desktop/src/cad.rs: `mesh_measures` (masa por triángulos, sin validar
  piezas de malla), `parts_tessellation` + `mesh_tessellation` (la vista),
  `shapes_scene_with` (exportar / pasar a modelo), masas en `parts_at_impl`.
  Falta: `cargo check -p pinocchio-app` limpio, compilar la app y probar por MCP.
- (Plan original de M4, para referencia) para piezas de malla, en apps/desktop/src/cad.rs, volumen /
  masa / validez desde los triángulos (no OCCT) y la vista (`encode_view_mesh`)
  con los triángulos de la pieza (si las caras de `from_mesh` siguen el orden de
  los triángulos, `triangle_face` = índice). Después acelerar `mesh_shell_direct`
  (std::map → hash, aristas sin BRepBuilderAPI_MakeEdge). Luego probar por MCP.

## Fuera de este plan

- Booleanas entre dos piezas CAD normales: siguen en OCCT (son exactas y
  rápidas con pocas caras).
- Redondeos y chaflanes sobre mallas.
