# Optimizaciones

Registro de la tanda de optimización de octubre de 2026: qué se midió, qué se cambió, con qué
resultados y qué falta. Todo está en la rama `fix/render-normales-e2e-y-manual`.

Principio de trabajo: **medir antes de tocar**, y cuando un cambio promete ser exacto, comprobar que
la salida es idéntica (checksum de la malla o de las UV, tests contra la versión original).

## Resumen

| Área | Antes | Ahora |
|---|---|---|
| Pesos de piel (autorig), 20 huesos, 82k tris, 1 hilo / 16 hilos | 84,7 s / 17,7 s | 0,72 s / 0,38 s |
| Pesos de piel, 120 huesos, 82k tris | >90 s / >90 s | 1,45 s / 0,59 s |
| Pesos de piel, 120 huesos, 330k tris, 16 hilos | horas | 2,8 s |
| Desplegado de UV, 43k tris, 1 hilo / 16 hilos | 10,6 s / 2,4 s | 2,4 s / 0,9 s |
| Horneado por rayos 2048², 270k tris, 1 hilo / 16 hilos | 16,6 s / 2,1 s | 6,6 s / 1,1 s |
| `ray_hit` (82k tris, 1 hilo) | 220 ms | 97 ms |
| `segment_intersects` (82k tris, 1 hilo) | 521 ms | 185 ms |
| Construcción del BVH (82k tris, 1 hilo / 16 hilos) | 189 ms (sin hilos) | 101 ms / 50 ms |
| Raycast de picking, 500k tris | 53 ms | 0,12 ms |
| Línea de tiempo, 120 huesos × 300 keys: montar | 12,2 s | 0,25 s |
| Línea de tiempo: poner una key | ~11,2 s | ~0,15 s |
| Historial guardado, 50 ediciones de un clip pesado | 341 MB y 5 s | 8 MB y 0,2 s |
| Historial con 200 ediciones | fallaba (`Invalid string length`) | 22 MB y 0,3 s |
| Vóxeles de `quadriflow` | referencia | ~7–12 % (y resultado ahora determinista) |

Los tiempos son de mallas sintéticas (esferas con relieve) y de bancos, no de modelos reales, con
LTO apagado. La máquina es compartida y los números con 16 hilos fluctúan ±50 % según la carga.

## Lo hecho

### Interfaz y animación (`apps/web`)

- **Línea de tiempo** (`7999e52`): la fila resumen llamaba a `keysAtFrames` por cada cuadro (cuadrático);
  ahora `keysByFrame` agrupa en una pasada y `trackKeyInfo` precalcula cuadros e interpolación por
  pista. Solo se dibujan las keys de las filas visibles en el scroll.
- **`violations` y `trajectory`** (`083ecf7`): se calculan por tandas de 8 ms (`createChunked`,
  `lib/chunked.ts`) en vez de bloquear el hilo en cada edición. El commit dejó intercambiadas las
  líneas `empty:` de ambas; se corrigió en `4476806`.
- **Bucle de render permanente** (`4476806`): `renderGlow` era un método del prototipo que el
  constructor del visor envuelve para pedir un cuadro; `animate` lo llamaba cada cuadro y el visor
  redibujaba a 60 fps aunque estuviera quieto (desde `41a40b2`). Ahora es una propiedad flecha. En
  reposo la app queda en 0 FPS y ~0,3 % de CPU (medido con el mamut cargado, en Preparar y en Animar).
- **Calidad adaptativa** (`4476806`): baja el pixel ratio a 0,7 mientras hay movimiento marcado
  (cámara, gizmo, giro modal, `setPose`) y lo restaura a los 250 ms. Ajuste "Calidad adaptativa al mover".
- **LOD al animar** (`4476806`): con más de 100 000 caras y rig, el backend simplifica a ~50 000
  (`quadriflow_core::remesh::simplify`, los pesos se heredan exactos) y el visor lo usa solo al
  reproducir. Exportes, guardado, render y capturas usan la malla completa. Ajuste "LOD al animar".
- **Historial** (`dabe723`): cada edición guardaba el clip entero antes y después, y al serializar
  se perdía la compartición de listas de keys. `packHistory` guarda cada lista distinta una vez. Los
  proyectos viejos cargan igual; uno nuevo no deshace bien en una versión anterior de la app.
- **Worker de animación** (`65dde32`): `bakeClip` y `observedLimits` corren en un Web Worker con
  datos planos, un trabajo a la vez, y respaldo síncrono si no hay `Worker`, falla al cargar o no
  avisa que arrancó en 8 s. Probado en Chromium y WebKitGTK 2.52 (sobre el servidor de desarrollo).
- **Picking con BVH** (`24980bf`): `three-mesh-bvh` en las mallas de reposo desde 20 000 triángulos
  (construcción diferida en el primer picking, ~350 ms; refit ~35 ms). La malla posada no se toca.

### Núcleo en Rust

- **Pesos de piel** (`046a40b`): un CG por hueso pasa a una sola factorización Cholesky dispersa
  (`faer`, `SpdFactor` en `pinocchio-sparse`) y dos sustituciones por hueso. Coincide con el CG hasta
  2,5·10⁻¹¹; si la factorización falla cae a CG. La factorización va en secuencial: la paralela de
  `faer` es más lenta en mallas (0,94 s secuencial contra 3,4 s con 16 hilos a 164k vértices).
- **BVH paralelo** (`046a40b`): `Bvh::build` con cajas precalculadas, subárboles en paralelo y
  ordenación estable. El árbol es idéntico al anterior (test con empates). Beneficia al embedding,
  al ajuste de esqueleto y al campo de distancias.
- **Ensamblado** (`046a40b`): `SPDMatrix::from_entries` / `from_triplets` ordenan en paralelo y de
  forma estable (resultado determinista con cualquier número de hilos); `add_diagonal` mezcla sin reordenar.
- **UV** (`3fb0d83`): LSCM y ARAP resuelven con Cholesky (ARAP factoriza una vez para todas las
  iteraciones; respaldo a CG si el residuo supera 1e-6). Las UV difieren del CG en <6·10⁻⁸.
  Empaquetado con **resultado idéntico bit a bit**: búsqueda del primer hueco con ventana-OR sobre la
  fila más llena de la carta (191M → 3,4M llamadas a `fits`), dilatación por palabras de 64 bits
  (28×) y rasterizado repartido por carta y por giro.
- **Rayos** (`2f48fbc`): BVH aplanado en preorden, recorrido con pila, hijo más cercano primero y
  desempate por el orden de hojas del recorrido original (mismo triángulo incluso en empates).
  Tests contra el recorrido recursivo original con 12 000 rayos y 4 000 segmentos.
- **Vóxeles y suavizado** (`4cf46a5`): el suavizado llenaba las listas de vecinos recorriendo un
  `HashMap` con orden aleatorio, y el resultado variaba en los últimos bits entre ejecuciones (con
  vóxeles + isótropo salían dos mallas distintas). Ahora las aristas se ordenan y el resultado es el
  mismo con cualquier número de hilos. Además: pila fija en `winding`, inundación sin divisiones por
  vecino y un hasher tipo Fx en los `HashMap` grandes.

## Cómo medir

Bancos en Rust (tests `#[ignore = "perf"]`, imprimen `PERF crate=… bench=… threads=… ms=…`):

```
CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
  cargo test --release -p <crate> <filtro> -- --ignored --nocapture --test-threads=1
```

- Comparar con `RAYON_NUM_THREADS=1` y sin la variable.
- `PERF_SCALE=small` para humo; `PERF_SCALE=large` en `pinocchio-attachment` (~330k triángulos).
- `quadriflow-core` necesita `--features simplify` y `pinocchio-repair` `--features parallel`.
- Crates con bancos: `geppettoray`, `uv-core` (`unwrap_quality` imprime un checksum de las UV),
  `quadriflow-core` (imprime `CHECK … hash=`), `pinocchio-{mesh,repair,spatial,sparse,skeleton,attachment,embedding}`.
  El de `orizon3d` (`libs/orizon3d/src/perf_tests.rs`) está sin commitear porque depende de un
  módulo `poisson` de otra sesión.
- Interfaz: `apps/web/perftest/` (`animBench.mjs`, `historyBench.mjs`, `bvhBench.mjs`, y arneses de
  navegador para la línea de tiempo, `createChunked` y el worker). Se empaquetan con esbuild y corren con node.

Mediciones en la app real: Xvfb + la app Tauri con un overlay temporal que se revierte (ver las
notas del proyecto). Sin GPU el render es por software y el FPS no sirve; sí sirven los conteos
(cuadros, temporizadores) y el tiempo de JS por llamada.

## Lo que falta

### Verificar en la app real (lo más importante)

- Un personaje con muchas keys: reproducir, arrastrar una key, abrir Animar, guardar y recuperar.
  Todo lo anterior se midió con bancos y arneses, no con un proyecto real.
- Calidad adaptativa y LOD al animar con una GPU (la iGPU Vega del usuario): que no parpadee al
  pasar de calidad baja a completa ni de LOD a malla completa, y que el LOD llegue a ~50 000 caras
  en un modelo real (con costuras de UV puede quedar más pesado).
- El worker sobre el protocolo de assets de la app empaquetada (solo se probó sobre el servidor de desarrollo).
- Un autorig completo con un modelo real tras el cambio a Cholesky.

### Pendientes conocidos

- **LOD:** posible carrera de versión entre el visor y el backend al pintar pesos (el backend
  debería devolver un contador de versión); el LOD solo se activa al reproducir, no al arrastrar o hacer scrub.
- **Worker:** `bakeMixer` (mezcla de capas) sigue síncrono al exportar y al renderizar.
- **Picking:** precargar el BVH en reposo para evitar el primer tirón de ~350 ms.
- **Pesos de piel:** reutilizar el ordenamiento AMD entre llamadas sobre la misma malla (~0,4 s a
  164k vértices); un ordenamiento de disección anidada (la factorización tiene 92 entradas por fila);
  el pico de memoria es ~1,3 GB con 164k vértices y 120 huesos.
- **UV:** `rasterize` es ahora el mayor costo del empaquetado con pocas cartas grandes (~730 ms de
  1170 con 1 hilo); el ensamblado de LSCM; el horneado sigue siendo lo más pesado de `unwrapped_skin`
  (`bake_rays`: 14,9 s con 1 hilo antes de la mejora de rayos).
- **Vóxeles:** `collapse_short_edges` (voraz, depende del orden: ~2,4 s a 790k vértices), el número de
  vueltas de Barnes-Hut (~10 µs por punto), `isotropic` (secuencial, 2–4 s), la retopología a quads
  (1,3× con 16 hilos) y `tris_to_quads`.
- **Reproducibilidad:** otros `HashMap` con orden aleatorio en `quadriflow` (`symmetry.rs`,
  `objects.rs`, `quad.rs`, `integer.rs`) podrían tener el mismo problema que tenía el suavizado.
  Hay que auditar cuáles recorren el mapa para decidir un orden.
- **Reparación y `mesh`:** no escalan con hilos (1,0×) pero ya son rápidas (<2,5 s); no urgen.

### GPU (`wgpu`)

Plan de largo plazo, sin implementar. Lo que mejor se paraleliza y es candidato: path tracing
(`geppettoray`), horneado por rayos de UV y campo de distancias (escalan 9–10× con hilos).
Esquema: un crate con la feature `gpu`, un trait de backend CPU/GPU por kernel, elección en tiempo de
ejecución con respaldo a CPU, y pruebas contra el resultado de CPU (también en la iGPU Vega o con
`llvmpipe`). No mejora la fluidez de la interfaz.

### Costos a tener presentes

- `faer` suma ~40 dependencias, +600 líneas en `Cargo.lock` y ~2,5 min de compilación en frío. No
  se probó el build de release con LTO (podría tardar más).
- `three-mesh-bvh` es una dependencia nueva del frontend.
