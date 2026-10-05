# CAD paramétrico en Pinocchiors

Traer a Pinocchiors el CAD de `cad-blender` (addon de Blender) sin Blender: núcleo
B-Rep con OpenCASCADE desde el inicio (sin etapa intermedia de mallas) y el flujo
nuevo **escaneo → CAD** que conecta orizon3d con sólidos editables.

Decisiones (2026-10-05):

- **Motor**: OpenCASCADE (OCCT 7.9) desde el día uno. Redondeos, chaflanes, STEP real
  y caras exactas. Se descartó empezar con manifold3d y migrar: las referencias a caras
  cambian de motor a motor y el trabajo se haría dos veces. manifold puede volver
  más adelante solo para booleanas sobre mallas orgánicas (`print3d`), que OCCT hace mal.
- **El árbol guarda recetas, no resultados**: cada operación guarda parámetros y se
  recalcula. El sólido nunca se serializa (solo los STEP importados, como bytes).
- **Referencias por geometría, no por índice**: "esta cara" = punto + normal (+ tipo de
  superficie); "esta arista" = punto medio + dirección. Se resuelven buscando la cara/arista
  más parecida en el sólido recalculado. Evita el problema de nombres topológicos.
- **Unidades**: milímetros. **Ejes**: Z arriba dentro del CAD (como cualquier CAD y como
  la UI de la app); al pasar al visor se convierte a Y arriba (`converter_scene::z_up_to_y_up`).
- **Sin OCCT compila igual**: `cad-occt` detecta OpenCASCADE en `build.rs`; si no está,
  compila un stub (todo devuelve "OpenCASCADE no disponible") y `cad_occt::available()`
  lo dice. Así no hacen falta features en cadena. `CAD_REQUIRE_OCCT=1` lo vuelve error
  (releases), `CAD_OCCT_STUB=1` fuerza el stub (probar).

## Crates

| Crate | Ruta | Qué hace | OCCT |
|---|---|---|---|
| `cad-solver` | `libs/cad/solver` | Solver de restricciones 2D (Newton + LM, dispersas, diagnóstico), ajuste de primitivas (plano/esfera/cilindro, RANSAC), segmentación por normales, simplificación | no |
| `cad-occt` | `libs/cad/occt` | Puente C++ propio a OCCT: `Shape` con RAII, perfiles con líneas/arcos/círculos, operaciones, topología consultable, teselado con id de cara, STEP | sí |
| `cad-model` | `libs/cad/model` | Documento: sketches + árbol de operaciones, recálculo, referencias geométricas, serde | vía cad-occt |

## Fases

- [x] **F0 — Plan** (este archivo).
- [x] **F1 — `cad-solver`**: portar desde `cad-blender/crates/cadblender_solver` sin pyo3 ni
      marca de licencia. Tests del original pasando. `constraint3d` (código muerto allá) no se trae.
- [x] **F2 — `cad-occt`**: puente C++ nuevo (el de cad-blender solo hacía polilíneas y no
      exponía redondeo ni booleanas).
  - build.rs: OCCT del sistema (`OCCT_INCLUDE_DIR`/`OCCT_LIB_DIR`, rutas comunes, pkg-config),
    nombres de librerías de 7.9 (TKDESTEP) y anteriores.
  - Errores: cada llamada devuelve el mensaje de `Standard_Failure` (sin `nullptr` mudos).
  - Perfiles: segmentos, arcos por 3 puntos, círculos, B-splines; varios lazos (agujeros).
  - Primitivas; extruir (vector, simétrico), revolucionar, barrer, loft; booleanas;
    redondeo/chaflán de aristas elegidas; cáscara; transformar, espejar.
  - Topología: caras (tipo de superficie, normal, centro, área, eje/radio de cilindros) y aristas.
  - Teselado: vértices, normales, triángulos con id de cara, polilíneas de aristas (para dibujar
    y para elegir con el mouse).
  - Volumen/área/centro de masa/caja; validez (`BRepCheck`). STEP leer/escribir (bytes y archivo).
- [x] **F3 — `cad-model`**: documento con sketches en planos (XY/XZ/YZ, cara, plano libre),
      entidades + restricciones resueltas con `cad-solver`, detección de regiones cerradas,
      operaciones (sketch, extruir ciego/simétrico/pasante/hasta cara, revolucionar, redondeo,
      chaflán, cáscara, booleana, patrón lineal/circular, espejo, primitiva, STEP importado),
      recálculo con error por operación, retroceso (rollback), suprimir, serde.
- [ ] **F4 — Escaneo → CAD**: de una malla (`pinocchio_mesh::Mesh` o nube de orizon3d)
      segmentar + ajustar → primitivas detectadas (plano, cilindro, esfera) con su error;
      convertirlas en operaciones (plano de trabajo desde plano detectado, agujero/tetón desde
      cilindro, corte de la malla por plano → sketch). Banco con modelos de `~/Descargas`.
- [ ] **F5 — App de escritorio**: feature `cad` en `apps/desktop`, comandos Tauri (documento,
      agregar/editar/borrar operación, recalcular, teselado para el visor, exportar STEP/STL/3MF,
      convertir el sólido en el modelo de la app), guardado en `.pinocchio`.
- [ ] **F6 — Espacio de trabajo "Diseñar"** en la web: árbol de operaciones, panel de parámetros,
      primitivas, sketch básico (línea, rectángulo, círculo, arco) sobre plano o cara, extruir,
      elegir caras/aristas en el visor, redondeo. Textos sin mencionar Blender.
- [ ] **F7 — Distribución**: workflow manual nuevo (no tocar `release.yml`) que compila OCCT
      estático solo con los toolkits necesarios y lo guarda en caché; Linux, Windows, macOS.
      Medir cuánto engorda el instalador.
- [ ] **F8 — Sketch completo**: portar herramientas de cad-blender (recortar/extender, offset,
      polígono, ranura, auto-restricciones, gestor de restricciones, cotas en el visor).
- [ ] **F9 — Más adelante**: planos 2D (proyección + cotas), ensambles, chapa.

## Compilar

Arch: `pacman -S opencascade` (7.9.3). Otras: definir `OCCT_INCLUDE_DIR` y `OCCT_LIB_DIR`.

```
cargo test -p cad-solver
cargo test -p cad-occt
cargo test -p cad-model
```

## Bitácora

- **2026-10-05 F1** (c19087b): solver portado; 101 tests. Arreglado `solve_drag` cuando el
  destino choca con las restricciones (devolvía un compromiso que no cumplía ninguna).
- **2026-10-05 F2** (7047c85, ab9e38a): puente nuevo, 15 tests con volúmenes exactos.
  Hallazgos: (1) `ShapeFix_Face` no da vuelta agujeros mal orientados → se decide por área;
  (2) redondeos imposibles "funcionan" y dejan sólidos inválidos → redondeo, chaflán,
  cáscara, desmolde y booleanas validan con `BRepCheck_Analyzer`; (3) los traductores STEP
  imprimen estadísticas por stdout → se quitan los printers del messenger global.
- **2026-10-05 F3**: `cad-model` con 12 tests de flujo completo (placa paramétrica que cambia
  de ancho y su redondeo sigue la arista, bolsillo en cara pasante, tetón hasta cara,
  revolución con eje del sketch, patrón circular de agujeros, simetría, vaciado, corte,
  errores aislados por operación, retroceso, suprimir, serde, importar STEP).
  - Regiones: caras del grafo plano (media arista "anterior en orden antihorario"), lazos
    sueltos para círculos, contención por el lazo más chico; profundidad par = perfil.
  - Radio de círculos es dato en el solver (no variable): las cotas de radio se aplican
    antes de resolver. Tangencia línea-arco con extremo común = perpendicular al radio (exacta).
  - Referencias: cara = punto + normal (coseno ≥ 0,9), arista = punto + dirección; error si
    la mejor coincidencia está a más de media diagonal del cuerpo.
  - Límite conocido: dos aristas paralelas equidistantes del punto guardado (p. ej. tras
    duplicar una altura) pueden empatar. Arreglo de fondo: nombres topológicos por historia
    (`BRepAlgoAPI_*::Generated/Modified`), pendiente.
