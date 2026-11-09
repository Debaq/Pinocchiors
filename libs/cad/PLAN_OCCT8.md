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
- 2026-11-08 (rama `occt8`): O1, O2 y O3 hechos en el PC. Detalle abajo.
- 2026-11-08: O5 hecho (superficie por grilla, resortes, planos y piezas en paralelo). Falta O4.

## Resultados (2026-11-08)

### O1 — compilar

- `scripts/build-occt.sh ~/.cargo-target/occt-8.0.1 8.0.1` compila **sin
  cambios** en el script (la 8 reordenó el árbol, pero los módulos y
  `BUILD_ADDITIONAL_TOOLKITS` siguen valiendo; CMake agrega `TKHelix`,
  `TKXMesh`, `TKExpress` solo). Prefijo: 263 MB.
- `build.rs` ya elegía OCCT por `OCCT_INCLUDE_DIR`/`OCCT_LIB_DIR`/`OCCT_STATIC`:
  nada que tocar.

### O2 — puente y pruebas

- `cad_occt.cpp`: un solo error. `Standard_Failure` ya no es
  `Standard_Transient` (sin `DynamicType()`): se usa `ExceptionType()` y
  `what()` con `#if OCC_VERSION_HEX >= 0x080000`, así compila con 7.9 y 8.
  El resto son avisos de obsoletos (`Standard_True/False`, typedefs `TColgp_*`,
  `TopTools_*`); siguen andando, se pueden limpiar después.
- `cargo test -p cad-occt -p cad-model -p cad-scan`: **511 pasan, 0 fallan, 4
  ignoradas**, igual que con 7.9.3.
- `cargo check -p pinocchio-app --tests` limpio (el único aviso,
  `evaluate_committed` sin usar, ya estaba).
- App real (debug, OCCT 8 estática): abre bien (no negra). Por el conector en
  vivo se armó una pieza (cajas, 18 agujeros, eje, chaflán, perno M10 con
  rosca, vista, plano PDF, STEP, STL) en la ventana: mismos volúmenes, caras,
  aristas y tamaños de archivo al byte que la pre-release 33001fe (7.9.3).

### O3 — 7.9.3 contra 8.0.1

Banco `libs/cad/occt/examples/bench_occt.rs` (`cargo run --release -p cad-occt
--example bench_occt [filtro]`), las dos versiones estáticas compiladas con el
mismo `build-occt.sh`, máquina libre:

- **Geometría idéntica** en los 24 casos (volúmenes, caras, aristas, válido),
  salvo 0,003 mm³ en un loft de 72 890 mm³.
- **Más rápida o igual en todo**: booleanas con muchos agujeros ×1,8–1,9,
  booleanas curvas ×2,1, roscas ×1,9, STEP grande ida y vuelta ×2,1, redondeos
  de muchas aristas ×1,5, loft ×1,4, HLR de placa ×1,3, malla de vista ×1,1–1,2,
  4 tareas en 4 hilos ×1,17. Masa (`BRepGProp`) igual (×0,97).
- **Estabilidad**: el vaciado de una caja redondeada que no se puede hacer
  **tumba el proceso con la 7.9.3 en 8 de 20 intentos** (violación de segmento:
  en la app se cierra todo); con la 8.0.1, 0 de 20 (da el error limpio).
  Redondeos y chaflanes: 0 caídas en las dos.

Lo nuestro contra lo nuevo de la 8 (`libs/cad/occt/bench/t_occt8.cpp`,
`OCCT_PREFIX=… bench/build8.sh t_occt8 [superficie|helice|hlr|malla]`):

- **Superficie libre (E10)**: piel de prueba de 60×40 mm. Relleno de hoy
  (`cad_fill`, borde + 100 puntos): desvío máx 1,34 mm (rms 0,38). **Gordon**
  con red 3×3: máx 0,10 mm (rms 0,035), 0,16 s; red 5×5: máx 0,003 mm, 1,4 s.
  Ojo: cada curva de la red con pocos puntos (4 tramos por celda); con 20 la
  superficie se infla a 397×397 polos y todo se vuelve lento.
- **Hélice**: la nuestra ya es exacta (error 5·10⁻⁶ mm, 0,4 ms). `TKHelix` es
  más exacta (5·10⁻⁸) pero más lenta (0,65 ms); sirve para lo que no tenemos:
  **resortes cónicos y de paso variable** (probado: anda).
- **Plano 2D**: el HLR exacto de hoy (75 ms, 4 vistas) es **más rápido** que
  `HLRBRep_PolyAlgo` por malla (129 ms). Se queda el nuestro.
- **Malla a sólido**: `BRepBuilderAPI_FastSewing` **no sirve** para
  triángulos (solo une parches de bordes naturales). Se queda `cad_from_mesh`.

Visor (OCCT 7.9.3, DRAWEXE): la placa de 625 agujeros dibuja en 0,5 ms de CPU
por cuadro; la diferencia con el nuestro no es velocidad sino funciones. Lo que
OCCT tiene y el nuestro no: varios planos de corte con tapa y rayado, cámara
ortográfica, selección exacta sobre el B-Rep, grilla infinita, teselado según
el zoom, instancias en ensambles. Lo nuestro que OCCT no tiene: croquis con
restricciones, render de fotos (GTAO, WebGPU), visor de mallas con rig y
animación, escaneo y resina.

### O5 — lo nuevo, aprovechado (2026-11-08)

1. **Superficie libre del escaneo por grilla** (no Gordon). Gordon con curvas
   por las filas y columnas de una grilla de la piel salía inflado (280×280
   polos de grado 9 con el parámetro de cuerda; 87×87 con uniforme) y lento:
   en un casquete de esfera, 4 s y 0,2 mm de desvío, contra 1 ms y 0,0004 mm de
   una B-spline por la grilla (`GeomAPI_PointsToBSplineSurface`). Gordon es para
   redes de curvas, no para grillas de puntos. Quedó:
   - `cad_grid_face` / `Shape::grid_face`: B-spline por la grilla (que pase por
     los puntos o a menos de `tol`), recortada por el borde.
   - `FreeZone::skin_grid`: grilla sobre el plano medio de la zona; cada punto,
     donde la perpendicular toca la zona; los de afuera, sobre el plano
     tangente del triángulo más cercano (promediar vecinos aplanaba el borde).
   - `fit_surface` prueba grillas de 13, 21 y 31 y se queda con lo que menos
     se aparta del escaneo (grilla o relleno). En una piel suave de 60×40:
     grilla 0,008 mm lejos del borde; el mejor relleno tenía un pico de 91 mm.
     En zonas muy empinadas en el borde (casquete hasta 75°) gana el relleno.
   - Anda igual con 7.9 (no usa nada de la 8). `ScanSurface.grid` opcional.
2. **Resortes con TKHelix**: `cad_make_helix_parts` / `Shape::helix_parts`
   (tramos con su paso y vueltas, diámetro lineal entre tramos). La hélice del
   diseño suma `pitch2` (paso variable) y `end_turns` + `end_pitch` (extremos
   cerrados de resorte de compresión); la cónica pasa a TKHelix. Con 7.9 (sin
   TKHelix), spline por puntos. UI (Diseñar → Hélice) y guía del conector IA.
   `build.rs` enlaza TKHelix si está.
3. **Planos 2D en paralelo** (`drawing_impl`): con OCCT 8 cada vista en su
   hilo con su copia (`Shape::deep_copy`: teselar escribe en la forma). Mismas
   líneas que de a una. 4 vistas: pieza 44 → 36 ms, placa 10×10 471 → 301 ms
   (la isométrica manda).
4. **Piezas en paralelo**: masa y caja de cada pieza (`parts_mass`, copias
   livianas: solo lee) y malla de cada pieza para el visor
   (`parts_tessellation`, copias profundas). Solo con OCCT 8.
5. Arreglo de pruebas: `scan_mold_*` y `extrude_up_to_the_scan_*` usaban la
   misma huella (7) en la caché de sólidos del escaneo: al correr juntas, una
   usaba el escaneo de la otra holgura (fallaba de a ratos).

Sin hacer: redondeos independientes en paralelo y STEP en paralelo (rinde
poco frente a lo que cuesta; ver PLAN_ROBUSTEZ para redondeos).

### Qué sigue

- O4 (release con la 8): `OCCT_VERSION: "8.0.1"` en `release.yml` y
  `android.yml`. Falta probar la compilación de la 8 en Windows (MSVC) y
  Android en CI. Es lo último (pedido del usuario).
