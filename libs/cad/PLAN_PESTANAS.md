# Pestañas de diseño: varias piezas aparte y ensambles (como Onshape)

Plan desde el 2026-10-22. Pedido del usuario: **pestañas como en Onshape para hacer piezas
aparte**, y que **los proyectos guardados antes se sigan abriendo igual** (retrocompatible).

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta · (F*n*) fase que lo cubre.

## Dónde estamos

Un proyecto tiene **un solo diseño**: `AppState.cad_document: Mutex<Option<cad_model::Document>>`
(`apps/desktop/src/state.rs`), guardado en `ProjectState.cad` (`apps/desktop/src/project.rs`,
MessagePack con nombres de campo, `FORMAT_VERSION = 1`). Dentro de ese diseño ya hay:

- ✅ Varias piezas (`PartId { feature, index }`, `Document.parts` con nombre/color/material), booleanas
  entre piezas, separar sólidos, STEP con piezas. Equivale a **un** Part Studio de Onshape.
- ✅ Ensamble, pero **dentro del mismo documento** (`Document.assembly`): sus `Instance.part` solo
  pueden apuntar a piezas de ese diseño.
- ✅ Configuraciones, versiones con nombre, varios planos 2D (`Document.drawing.sheets`).
- ✅ Cada pieza es un objeto de la escena (`ObjectSource { kind: "cad", part, hash }` en
  `apps/web/src/lib/objects.ts`, clave `partKey = feature:index`), con malla por
  `cad_part_to_model(part, known)`.

El frontend edita el documento entero y lo manda (`cad_set_document` / `cad_preview` en
`createCadStore`, `apps/web/src/lib/cad.ts`), con su pila de deshacer propia. El backend guarda
cachés del recálculo (`cad_cache`, `cad_ops: EvalCache`) para ese único documento.

Falta (❌): varias pestañas de diseño, una pestaña de ensamble que use piezas de cualquier
pestaña, piezas derivadas de otra pestaña y planos que apunten a una pestaña cualquiera.

## Decisiones

**Tomadas (usuario, 2026-10-22):**
- Retrocompatibilidad: todo `.pinocchio` (y autoguardado) de formato 1 se abre sin perder nada.

**Confirmadas (usuario, 2026-10-22):**
- Nombres en la interfaz: pestaña de diseño = **«Diseño»** («Diseño 1», «Diseño 2»…); pestaña de
  ensamble = **«Ensamble»**. Nada de «Part Studio» ni «croquis». En inglés: *Part design* /
  *Assembly*.
- Barra de pestañas **abajo del visor de Diseñar**, como Onshape: clic cambia, doble clic
  renombra, arrastrar reordena, menú contextual (renombrar, duplicar, borrar con
  `ConfirmDialog`), botón «+» (Nuevo diseño / Nuevo ensamble). Atajos reasignables en el área de
  Diseñar: Ctrl+RePág / Ctrl+AvPág para pasar de pestaña.
- El ensamble viejo dentro del diseño **se convierte solo** en una pestaña de ensamble al abrir
  (ver «Retrocompatibilidad»); el ensamble dentro del diseño deja de existir en la interfaz.
  La conversión llega con F5: hasta entonces el ensamble sigue dentro de su diseño (si F1 lo
  sacara, la app lo perdería entre F2 y F5).
- Los parámetros siguen siendo **por pestaña**; parámetros compartidos del proyecto quedan para
  después (F8).

## Retrocompatibilidad (regla para todas las fases)

1. **El diseño viejo pasa a ser la pestaña de id 0**, llamada «Diseño 1». Toda referencia vieja
   sin pestaña (`ObjectSource.part`, `Instance.part`, claves `feature:index`) se lee como
   «pestaña 0». Así no hay que reescribir referencias: el campo nuevo `studio` vale 0 por defecto
   (`#[serde(default)]` en Rust, `?? 0` en TS). Ojo: en un proyecto nuevo la pestaña 0 puede
   borrarse; las referencias nuevas llevan siempre `studio` explícito.
2. **Leer siempre los campos viejos.** `ProjectState.cad: Option<Document>` se sigue leyendo
   (nunca se escribe más); si está y `cad_tabs` viene vacío, se migra en `restore`. Lo mismo
   `Document.assembly` → pestaña de ensamble (F5, con `Tabs::lift_assemblies`, idempotente: sirve
   también para los proyectos de formato 2 guardados entre F2 y F5), y `SceneObject.source` sin
   `studio` en `ui.json`.
3. **La migración es pura y se prueba sola**: `cad_model::Tabs::from_legacy(Option<Document>)` en
   el modelo, sin estado de la app.
4. **`FORMAT_VERSION` sube a 2** al escribir el formato nuevo. Una app vieja que abre un proyecto
   nuevo ya da «El proyecto es de una versión más nueva de la app…» en vez de abrirlo a medias y
   perder las otras pestañas al guardar. (Escribir además `cad` = pestaña 0 para las apps viejas
   se descartó: abrirían solo esa y borrarían el resto al guardar.)
5. **Proyectos de muestra congelados (F0)**: se guardan con el código de hoy *antes* de tocar nada
   y quedan como fixtures; cada fase tiene que abrirlos y dejar todo igual.

## Fases

### F0 — Fixtures de compatibilidad (antes de cambiar nada) ✅ 2026-10-22

- Con el código actual (master `7a81f83`), generar desde un test de `apps/desktop` (o el puente
  `cad_http`) y guardar en `apps/desktop/tests/fixtures/`:
  - `v1-diseno-piezas.pinocchio`: diseño con 2 piezas (nombre, color y material por pieza),
    parámetros, una configuración, una versión con nombre y un plano 2D con cotas a mano.
  - `v1-ensamble.pinocchio`: lo anterior más ensamble con 3 instancias, bisagra, deslizante y una
    relación de engranes.
  - `v1-objetos.pinocchio`: una pieza ya convertida en malla (objeto con `hash`), otra sin malla,
    un objeto importado y un paso de Preparar sobre la pieza (para «rehacer si cambia el diseño»).
- Test `legacy_projects_open` que hoy solo comprueba que abren; cada fase le suma lo suyo.
- Mantener los fixtures chicos (primitivas, no escaneos) para no inflar el repo.
- Hecho: generador `project::tests::generate_v1_fixtures` (`#[ignore]`, corrido una vez sobre
  7a81f83; no volver a correrlo) y `legacy_projects_open` (diseño igual, volúmenes de las dos
  piezas, ensamble igual y resuelto, objeto activo con malla y cubo importado). Las muestras
  pesan 1–2 KB. `ui.json` de las muestras es mínimo (versión y `objects`): el historial de
  Preparar sobre la pieza no está; F4 lo prueba con su propio caso. `read_project` sale de
  `open_project` para que los tests lean igual que la app.

### F1 — Modelo: pestañas en `cad_model` ✅ 2026-10-22

- `libs/cad/model/src/tabs.rs` (nuevo):
  ```rust
  pub type TabId = u32;
  pub struct Tabs { pub tabs: Vec<Tab>, pub active: TabId, pub next_id: TabId }
  pub struct Tab { pub id: TabId, pub name: String, pub kind: TabKind }
  pub enum TabKind { Design(Document), Assembly(AssemblyDoc) }   // serde tag = "type"
  ```
  `AssemblyDoc` = el `Assembly` de hoy más lo que el ensamble necesite propio (plano, explosión).
- `Tabs::from_legacy(Option<Document>) -> Tabs`: el documento va **tal cual** a la pestaña 0
  «Diseño 1» (ensamble incluido: se saca en F5). Sin documento → `Tabs` vacío.
- Operaciones puras: agregar (nombre numerado o único), renombrar (sin repetir, sin vacío),
  duplicar (al lado, «(copia)»), borrar (nunca la única; la activa pasa a la vecina), reordenar,
  activar. Los ids no se reusan. «Quién usa esta pestaña» llega con F5/F6.
- Pruebas (`tests/tabs.rs`): migración con y sin diseño, ida y vuelta JSON y MessagePack (con
  bytes de un STEP importado), nombres, duplicar independiente, borrar y mover.
- Hecho: `libs/cad/model/src/tabs.rs` (`Tabs`, `Tab`, `TabKind::{Design { document: Box<Document> },
  Assembly}`), 7 pruebas en `tests/tabs.rs`. Nombres repetidos sin distinguir mayúsculas
  (también «Ñ»/«ñ»).

### F2 — Backend: estado, guardado y comandos ✅ 2026-10-22

- `AppState.cad_document` → `cad_tabs: Mutex<Tabs>`. Para no tocar los ~40 usos de una vez, un
  ayudante `with_active_design(state, |doc| …)` / `active_design(state)`; los comandos de hoy
  (`cad_set_document`, `cad_preview`, `cad_get_document`, `cad_parts_at`, exportar, plano…)
  **siguen actuando sobre la pestaña activa** con la misma firma.
- Cachés por pestaña: `cad_cache` y `cad_ops` pasan a `HashMap<TabId, …>` (cambiar de pestaña no
  recalcula todo; borrar una pestaña suelta su caché).
- Comandos nuevos: `cad_tabs` (lista: id, nombre, tipo, activa), `cad_tab_add { kind, name? }`,
  `cad_tab_rename`, `cad_tab_duplicate`, `cad_tab_delete`, `cad_tab_move`, `cad_tab_activate`.
  Registrar en `lib.rs` y en el puente `cad_http`.
- `ProjectState`: campo nuevo `cad_tabs: Option<Tabs>` (`serde(default)`); `cad` queda solo para
  leer. `restore`: `cad_tabs` o, si falta, `Tabs::from_legacy(cad)`. `capture` escribe solo
  `cad_tabs`. `FORMAT_VERSION = 2`.
- `cad_part_to_model` recibe `studio: Option<TabId>` (falta = 0); `parts_scene` evalúa esa pestaña.
- Pruebas: fixtures F0 abren y quedan iguales (mismas piezas, volúmenes, ensamble resuelto en la
  pestaña nueva); guardar → abrir con formato 2 conserva todo; proyecto con 3 pestañas ida y
  vuelta.
- Hecho: `AppState.cad_tabs` con `cad_doc()` (copia de la activa), `with_cad_doc(f)` (cambia la
  activa; sin pestañas crea «Diseño 1»; error si la activa es un ensamble) y `set_cad_tabs`.
  `cad_cache` sigue siendo el recálculo de lo que se ve; `cad_tab_caches` guarda el de las demás
  y `change_tabs` los intercambia (las ~30 lecturas de `cad_cache` no cambiaron). `cad_ops`
  (`EvalCache` por huella) se comparte entre pestañas. Comandos `cad_tabs`, `cad_tab_add
  { kind: "design" | "assembly", name? }`, `cad_tab_rename`, `cad_tab_duplicate`,
  `cad_tab_delete`, `cad_tab_move { id, to }`, `cad_tab_activate`: devuelven `TabsView
  { tabs: [{ id, name, kind }], active }`; la interfaz relee documento y recálculo después.
  `cad_status.has_document` = hay pestañas (aunque la activa sea un ensamble). `cad_new` = una
  sola pestaña vacía; `cad_close` = sin pestañas. `cad_part_to_model { part, studio? }`: otra
  pestaña se recalcula aparte sin tocar el visor. Pruebas: `legacy_projects_open` (pestaña 0 y
  misma huella de malla: no se regenera), `tabs_survive_save_in_format_2` (sin campo `cad`),
  `newer_format_is_refused`, `switching_tabs_keeps_each_evaluation`,
  `part_of_another_tab_becomes_a_model`. Suite del backend: 160 ok.
- Ojo: entre F2 y F3 la app ya guarda en formato 2; una compilación anterior a F2 no abre esos
  proyectos (avisa). La interfaz de hoy sigue funcionando igual (una sola pestaña).

### F3 — Interfaz: barra de pestañas 🟡 2026-10-22 (falta probarla en la app)

- Componente `DesignTabs` debajo del visor de Diseñar; estado `tabs()` en `createCadStore` con
  `reload` al cambiar de pestaña.
- **Deshacer por pestaña**: `undoStack`/`redoStack` por `TabId` (cambiar de pestaña no pierde el
  historial de la otra). Agregar, borrar, renombrar y reordenar pestañas también se deshacen.
- Lo de la pestaña sale de la pestaña: árbol de operaciones del Outliner, parámetros,
  configuraciones, versiones, planos. El grupo **Objetos** del Outliner muestra las piezas de
  todas las pestañas, agrupadas por pestaña.
- Borrar pestaña: `ConfirmDialog` diciendo qué se pierde (piezas con malla, instancias de
  ensamble que la usan).
- i18n (`t()` con español como clave, catálogo `en.<zona>.ts`), atajos reasignables, 6 temas
  (colores solo en `app.css`). Sin mencionar Blender.
- Pruebas: node para la lógica pura del store; e2e del pedido («pestañas: crear, renombrar,
  duplicar, borrar, deshacer») solo cuando el usuario lo pida.
- Hecho: `components/layout/DesignTabs.tsx` (barra de 28 px bajo el visor de Diseñar, montada en
  App junto a `CadView`; clic, doble clic para renombrar, arrastre con el puntero, menú
  Renombrar/Duplicar/Borrar… con `confirmAction`, «+» = Nuevo diseño). `changeDesignTabs` termina
  el sketch abierto, suelta selecciones y el modo ensamble antes de cambiar de pestaña. Store:
  `tabs()`, `changeTabs(cmd, args)`, `loadActive`; `reload` y `newDesign` releen las pestañas;
  `apply` las relee si el primer cambio creó «Diseño 1». Lógica pura en `lib/designTabs.ts`
  (`neighborTab`, `dropIndex`, `swapTabHistory`), node `e2e/designTabs.test.mjs`. Atajos
  Ctrl+AvPág / Ctrl+RePág (área Diseñar, reasignables). Nombres por defecto del backend
  («Diseño 1», «… (copia)») se muestran con `tBackend` (en inglés «Part design 1»); los
  errores de pestañas están en `en.layout2.ts`.
- Cambios respecto de lo planeado: agregar, borrar, renombrar y mover pestañas **no** se deshacen
  (borrar pide confirmación y lo dice); «Nuevo ensamble» espera a F5; las piezas agrupadas por
  pestaña en el Outliner pasan a F4 (allí los objetos ya saben de qué pestaña son). Copiar y
  pegar operaciones entre pestañas debería andar ya (el portapapeles es del store, no de la
  pestaña): sin probar.

### F4 — Objetos de la escena por pestaña ❌

- `ObjectSource` cad: `{ kind: "cad"; studio?: number; part; hash? }`; `partKey` →
  `studio:feature:index` con `studio ?? 0` (las claves viejas `feature:index` se leen como
  pestaña 0 al cargar `ui.json`).
- `syncCadObjects` recibe las piezas de **todas** las pestañas de diseño; borrar una pestaña deja
  los objetos que ya tenían malla (como hoy con piezas borradas).
- `App.tsx` pasa `studio` a `cad_part_to_model`; Preparar/Rig/Fabricar y «rehacer si cambia el
  diseño» funcionan igual con piezas de cualquier pestaña.
- Pruebas: fixture `v1-objetos` abre con sus mallas y su paso rehecho; pieza de la pestaña 2 se
  vuelve objeto y se exporta con `export_objects`.

### F5 — Ensamble como pestaña propia ❌

- `Tabs::lift_assemblies()`: cada diseño con `assembly` con instancias → pestaña «Ensamble N»
  justo después, instancias con `studio` = ese diseño; se llama en `restore` (idempotente).
- `Instance`: campos nuevos `studio: TabId` (`serde(default)` = 0) y `configuration: Option<usize>`.
  El ensamble evalúa cada pestaña que usa (con su configuración) y reutiliza las cachés de F2.
- «Insertar» en la pestaña de ensamble: lista de piezas por pestaña (y configuración), como el
  diálogo Insert de Onshape. Un ensamble puede meter otro ensamble como subensamble: **no** en
  esta fase (anotado en F8).
- Lo que hoy hace el ensamble en el diseño se muda a la pestaña: uniones, relaciones
  (engranes, piñón-cremallera, tornillo), arrastre (`assemblyDrag.ts`), vista explosionada,
  choques, lista de materiales y exportar el ensamble (STEP/3MF).
- Si cambia una pieza en su pestaña, el ensamble se actualiza al volver a él (huella por
  pestaña); si la pieza ya no está, la instancia queda marcada en rojo, no se borra.
- Pruebas: fixture `v1-ensamble` abre como pestaña «Ensamble 1» con la misma solución del solver
  (posiciones iguales a 1e-9); ensamble con piezas de dos pestañas; pieza borrada → instancia
  marcada.

### F6 — Pieza derivada (traer una pieza de otra pestaña) ❌

- Operación nueva `FeatureKind::Derived { studio, parts, configuration, placement }`: trae el
  sólido de piezas de otra pestaña como piezas de esta, y se rehace cuando el original cambia
  (como *Derived* de Onshape). Sirve para molde/contramolde, tapa que calza con la caja, etc.
- La evaluación recibe un resolvedor de pestañas; **ciclos** (A deriva de B que deriva de A) se
  detectan y dan error claro antes de evaluar. Borrar una pestaña de la que otra deriva → aviso
  en el `ConfirmDialog` de F3.
- Pruebas: derivar y restar (molde), cambiar el original y que la derivada siga, ciclo detectado.

### F7 — Planos 2D de cualquier pestaña ❌

- Cada hoja (`DrawingSheet`) recuerda de qué pestaña son sus vistas (`source: TabId`, falta = la
  pestaña que la contiene); un plano puede ser de un ensamble (vistas con todas las instancias y
  la lista de materiales como tabla).
- Las hojas viejas siguen en su pestaña de diseño, sin cambios.
- Pruebas: fixture `v1-diseno-piezas` con su plano idéntico (mismo SVG); plano del ensamble con
  globos y tabla de materiales.

### F8 — Después (sin definir con el usuario)

- Subensambles (un ensamble dentro de otro).
- Parámetros del proyecto compartidos entre pestañas (variables de Onshape).
- Copiar/pegar operaciones entre pestañas y «Pasar piezas a una pestaña nueva».
- Exportar todas las pestañas a la vez (un STEP por pestaña o uno con todo).

## Archivos que se tocan (guía)

| Dónde | Qué |
|---|---|
| `libs/cad/model/src/tabs.rs` (nuevo), `lib.rs`, `assembly.rs`, `feature.rs`, `eval.rs` | `Tabs`, migración, `Instance.studio`, `Derived` |
| `apps/desktop/src/state.rs`, `project.rs`, `cad.rs`, `lib.rs` | estado, formato 2, comandos de pestañas, cachés por pestaña, `cad_http` |
| `apps/web/src/lib/cad.ts` (`createCadStore`), `cadUi.ts`, `objects.ts`, `assemblyDrag.ts`, `drawingDoc.ts` | store por pestaña, deshacer, claves de objetos |
| `apps/web/src/components/layout/CadView.tsx`, `steps/DesignStep.tsx`, Outliner, `App.tsx` | barra de pestañas, Outliner, `cad_part_to_model` con `studio` |
| `apps/desktop/tests/fixtures/` | proyectos de formato 1 congelados (F0) |

Lo hecho de cada fase se anota en la bitácora de [ROADMAP.md](ROADMAP.md).
