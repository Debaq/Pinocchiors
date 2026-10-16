# Informe de sesión — E4.1 (2026-10-11)

Informe histórico de esa sesión. Al 2026-10-15 siguen abiertos solo el caso del conejo a −0,5 mm (E2) y las
carcasas de piezas grandes; el estado actual de E4–E10 está en `PLAN_ESCANEO.md`. Las 8 fallas e2e de abajo
no se volvieron a medir aquí.

Pedido: seguir con E4 de `libs/cad/PLAN_ESCANEO.md` («formas que siguen la piel») y al terminar
correr la suite e2e completa.

## Commits

| Commit | Qué |
|---|---|
| `086ca90` | feat(escaneo): E4.1 carcasa y parche del escaneo (cáscara por vóxeles, recorte bajo un sketch) |
| `1a09273` | test(e2e): remallar isótropo espera antes de deshacer (inestable: el atajo se perdía recién aplicado) |

Sin push.

## Qué hace E4.1

- **Carcasa del escaneo**: pieza que copia la piel con un espesor, hacia afuera o hacia adentro,
  separada de la piel por una holgura. Por defecto: 2 mm hacia afuera, holgura 0,2 mm, pieza nueva.
- **Parche desde sketch**: solo la parte de la carcasa que cae bajo las regiones de un sketch
  (férulas, almohadillas, tapas). Se queda con el primer pedazo que tocan los rayos desde el sketch,
  así que sobre un brazo toma la cara de arriba y no la de abajo.
- **UI**: pestaña «Desde el escaneo» → botones «Carcasa» y «Parche desde sketch». Diálogo con
  «Forma» (Lleno / Carcasa hacia afuera / Carcasa hacia adentro), «Espesor» y «Solo bajo»
  (todo el escaneo o un sketch con sus regiones).

## Cambios por archivo

- `libs/quadriflow/core/src/rebuild.rs`: modo cáscara en la reconstrucción por vóxeles
  (banda `inner < s < outer`, campo `max(inner−s, s−outer)`); soldadura con `tiny = 1e-3·h`
  (con 1e-6 quedaban agujeros).
- `libs/quadriflow/core/src/remesh/voxel.rs`: `VoxelOptions.thickness`.
- `libs/quadriflow/core/src/isotropic.rs`: los vértices se reproyectan solo sobre triángulos que
  miran al mismo lado (`query_closest_facing`); antes podían saltar a la piel de enfrente.
  Afecta también a Remallar → Isótropo.
- `libs/pinocchio/spatial/src/bvh.rs`: `query_closest_facing`.
- `libs/cad/scan/src/body.rs`: `BodyOptions.thickness`; escaneos abiertos se cierran por vóxeles
  antes de armar la carcasa; límites de espesor mínimo y de cantidad de triángulos con mensaje
  que sugiere el espesor; `fix_slivers` (gira capuchones, funde agujas con condición de enlace).
- `libs/cad/occt/cpp/cad_occt.cpp`: `mesh_shell_direct` arma una cáscara por pieza conexa;
  las de volumen negativo son huecos dentro del sólido que las contiene (carcasa de escaneo cerrado).
- `libs/cad/model/src/feature.rs`: `ScanBody.thickness` y `ScanBody.region` (`ScanRegion`),
  dependencia del sketch, etiquetas «Carcasa del escaneo» / «Parche del escaneo».
- `libs/cad/model/src/eval.rs`: `scan_patch` (prisma de las regiones ∩ carcasa, con historia de caras).
- `apps/desktop/src/cad.rs`: caché de cuerpos por (holgura, espesor, caras).
- `apps/web/src/lib/cad.ts`, `designActions.ts` (`startScanShell`), `components/steps/DesignStep.tsx`.
- Pruebas: `isotropic_voxel.rs` (cáscara entre dos desplazamientos), `libs/cad/scan/tests/scan.rs`
  (carcasa y parche sobre una esfera, volúmenes al 2 %), e2e «escaneo: carcasa y parche bajo un sketch».
- `libs/cad/PLAN_ESCANEO.md`: E4 reescrita, E4 en 🟡.

## Problemas encontrados

1. **Booleanas vacías sin error** sobre la carcasa del conejo. Causa: en escaneos abiertos el signo
   de la distancia salta en los huecos y queda un tubo en el borde → ~900 auto-intersecciones →
   OpenCASCADE devuelve vacío sin avisar. Arreglo: cerrar el escaneo antes (vóxel del doble del
   tamaño: 4,7 s en vez de 12 s) y lado del isótropo ≤ espesor.
2. **Primera versión de `fix_slivers` rompía la variedad** (aristas repetidas). Arreglo: condición
   de enlace y bloqueo de vértices por pasada.
3. **Gonfoterio (1 m)**: con 2 mm salía basura → ahora error de espesor mínimo. Con 6 mm tardó
   470 s y dio 307 380 triángulos → ahora error de demasiados triángulos (pide ≥ 7 mm).

## Verificación

- Tests Rust: quadriflow-core, pinocchio-spatial, cad-scan, cad-model y cad-occt pasan.
- Banco conejo, carcasa 2 mm: preparar 12 s, sólido 0,9 s, caja − carcasa 4 s, resultado válido.
- E2e de escaneo: 9/9.

## Suite e2e completa

Resultado: **134 ✓ / 8 ✗** (log: `e2e-completo-2026-10-11.log` en la raíz del repo).

Ya fallaban antes de E4.1 (verificado corriéndolas en `f45ba8d`, fallan igual):

| Escenario | Mensaje |
|---|---|
| referencia perdida: advertencia, en rojo y reemplazo | tras reemplazar: Arista 1,Arista 2 · no encontrada,Arista 3 |
| barrido y transición | no encontré: Transición |
| diálogo de operación: cancelar y aceptar | no encontré: Deshacer |
| usar del modelo: cara, silueta, intersección, otro sketch y romper vínculo | no está Intersección |
| selección como Onshape: región, cara y arista | selección: (vacía) |
| outliner: planos, operaciones y piezas | árbol sin las operaciones esperadas |

Inestables:

| Escenario | Estado |
|---|---|
| remallar: isótropo con lado en mm y aristas vivas | «deshacer no devolvió la malla»: el Ctrl+Z se perdía recién aplicado (también en la base). Arreglado con espera de 3 s (`1a09273`), pasó 3/3. |
| escaneo: sigue de referencia con una pieza activa | «no encontré la pestaña Desde el escaneo»; al repetirlo pasa. Sin arreglar. |

Escenario saltado: «remallar: isótropo traslada textura y pesos» no tiene
`~/Descargas/chilesaurus_animado.glb`, así que se salta esa parte.

## Pendiente

- E2: caja − cuerpo del conejo a −0,5 mm cortado a media altura sigue dando «el resultado no es un
  sólido válido» (no eran las astillas).
- Las 6 fallas e2e de arriba (anteriores a E4.1).
- «escaneo: sigue de referencia» inestable.
- ~~E4.2: región pintada con pincel~~ y ~~E4.3: molde~~: hechos el 2026-10-15 (ver `PLAN_ESCANEO.md`).
- Carcasas en piezas grandes (≥ 1 m) son lentas o piden espesores grandes.
