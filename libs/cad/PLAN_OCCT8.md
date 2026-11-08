# Plan: OpenCASCADE 8.0.1

2026-11-08. Hoy la app usa OCCT 7.9.3 (Arch la trae en el PC; la release la
compila estática con `scripts/build-occt.sh`, `OCCT_VERSION: "7.9.3"` en
`.github/workflows/release.yml` y `android.yml`). La 8.0.1 salió el
2026-07-30 (8.0.0 el 2026-05-07, 8.0.0p1 el 2026-06-17). El usuario leyó las
notas y quiere aprovecharla. Notas: `gh api
repos/Open-Cascade-SAS/OCCT/releases/tags/V8_0_0 --jq .body` (también
`V8_0_0_p1`, `V8.0.1`); guía de migración:
<https://dev.opencascade.org/doc/overview/html/occt__upgrade.html>.

Regla: se compila y se prueba en el PC (el usuario lo pidió el 2026-11-08);
la release recién al final, con todo verde acá.

## Qué ganamos (cruzado con lo que usa la app)

1. **Estabilidad del CAD.** Arreglos de cuelgues y caídas en redondeos y
   chaflanes (`ChFi3d_Builder`, `BRepFilletAPI_MakeFillet::Add` que se
   colgaba, chaflanes que caían), fusiones que se colgaban (lofts, el «fuse
   simplificado»), secciones booleanas con B-splines periódicas que se
   colgaban, vaciado. Hoy un cuelgue de OCCT deja el diseño pegado.
2. **Paralelismo real.** El manejo de errores de OCCT es `thread_local`, sin
   candado global; los redondeos independientes ya no comparten estado entre
   hilos (8.0.1); STEP admite un lector o escritor por hilo; `Standard_Strtod`
   es seguro entre hilos (p1). Nuestro `Shape` ya es `Send`
   (`libs/cad/occt/src/lib.rs:381`).
3. **Más rápido sin tocar nada nuestro.** Intersección de superficies con BVH
   (`IntPatch_PolyhedronBVH`, consultas O(log n) en vez de comparar
   triángulos), `BOPAlgo_PaveFiller` con menos copias y memoria, propiedades
   de masa (`BRepGProp`) más rápidas (la app las pide seguido: centro, caja,
   medir), validar caras con muchos agujeros (8.0.1: patrones, placas
   perforadas), sanado de formas más rápido (STEP).
4. **Superficies de Gordon** (`GeomFill_Gordon`, rehecho en p1 y mejorado en
   8.0.1): superficie desde una red de curvas. Para la superficie libre del
   escaneo (E10 de PLAN_ESCANEO, hoy por relleno) con curvas tomadas de la piel.
5. **Hélices** (`TKHelix`): roscas y resortes con mejor geometría
   (`cad_make_helix`, `cad_make_thread*` en `cad_occt.cpp`).
6. **Mallado y STEP.** Costuras de caras periódicas mejor malladas
   (`BRepMesh_BaseMeshAlgo`), sin desbordes de pila; STEP ~20 % más chico,
   escritura más robusta.

## Qué cuesta

- Puente C++ (`libs/cad/occt/cpp/cad_occt.cpp`, ~3 900 líneas, 123
  includes). Ya atrapa `Standard_Failure` y `std::exception` (en la 8
  `Standard_Failure` hereda de `std::exception`: compatible) y lanza con
  `throw Standard_Failure(...)` (sigue valiendo). Revisar: `Raise()`/`Throw()`
  quitados, firmas de HLR con punteros tipados (`HLRBRep_*`), APIs que
  devuelven handles, `.gxx` de clasificadores (usar `BRepClass`), funciones
  matemáticas globales deprecadas (`Sqrt`, `Abs`… → `std::`), C++17 mínimo.
- `scripts/build-occt.sh`: la 8 reordenó el árbol (`src/Module/Toolkit/...`)
  y CMake; revisar módulos y `BUILD_ADDITIONAL_TOOLKITS`
  (`TKDESTEP;TKFeat;TKOffset;TKFillet;TKBool;TKBO;TKMesh;TKShHealing;TKPrim;TKTopAlgo`,
  más `TKHelix` si se usa). VTK ya viene apagado.
- Release: la caché de OCCT se rehace (~40 min por plataforma la primera vez:
  Linux, Windows con MSVC, Android).

## Fases

### O1 — OCCT 8.0.1 en el PC

- Compilar 8.0.1 estática con `scripts/build-occt.sh` en
  `~/.cargo-target/occt-8.0.1` (sin tocar la 7.9.3 del sistema).
- Que `libs/cad/occt/build.rs` pueda elegirla por variable de entorno
  (ver cómo encuentra hoy OCCT; la release ya usa una ruta propia).

### O2 — El puente compila y las pruebas pasan

- Arreglar `cad_occt.cpp` contra la 8 hasta que compile sin avisos nuevos.
- `cargo test -p cad-occt`, `-p cad-model`, `-p cad-scan` (y bancos de
  `libs/cad/occt/bench`) con la 8: los mismos resultados que con la 7.9.3.
  Anotar cualquier diferencia de geometría (volúmenes, caras, aristas).
- `cargo check -p pinocchio-app --tests` y abrir la app real una vez (ver
  apps/URGENTE_RELEASE.md: la app real tiene fallas que las pruebas no ven).

### O3 — Medir 7.9.3 contra 8.0.1

- Mismas piezas, mismos pasos: booleanas (caja con muchos agujeros, piezas
  curvas), redondeos y chaflanes, vaciado, planos 2D (HLR de 4 vistas),
  mallado de la vista, STEP ida y vuelta, el hueso frontal con relieve.
  Tiempos y si termina o se cuelga. Resultado en este plan.

### O4 — Release con la 8

- `OCCT_VERSION: "8.0.1"` en `release.yml` y `android.yml`; `build-occt.sh`
  al día; release Linux primero, después Windows y Android.

### O5 — Aprovechar lo nuevo

1. Planos 2D: cada vista en paralelo (HLR por vista con rayon en
   `drawing_impl`, apps/desktop/src/cad.rs).
2. Por pieza en paralelo: malla de la vista, masa y caja de cada pieza del
   diseño.
3. Redondeos independientes en paralelo cuando el árbol lo permita.
4. STEP: importar y exportar varias piezas en paralelo.
5. Superficie libre del escaneo con `GeomFill_Gordon` desde una red de curvas
   de la piel (comparar con el relleno actual de E10).
6. Roscas y resortes con `TKHelix`.

## Avance

- 2026-11-08: plan escrito (notas de 8.0.0, 8.0.0p1 y 8.0.1 leídas y cruzadas
  con la app).
