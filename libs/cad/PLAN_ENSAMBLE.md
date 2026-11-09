# Ensamble: lo que falta para que sea de verdad útil

Plan desde el 2026-11-09, a partir de la auditoría del ensamble (pedido del usuario: «audita el
sistema de ensamble del CAD para ver qué nos falta», y luego «sí, hazlo»).

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

## Dónde estamos

- Modelo: `libs/cad/model/src/assembly.rs` (`Assembly`, `Instance`, `Connector`, `Mate`,
  `Relation`, `solve` por Levenberg-Marquardt con jacobiano numérico, `sweep`, choques, holguras,
  engranes). Pestañas y subensambles: `libs/cad/model/src/tabs.rs` (`flat_assembly`,
  `subassembly_part`, variantes por configuración).
- App: `apps/desktop/src/cad.rs` (`cad_assembly_mesh/faces/connector/interference/clearance/
  suggest/gear_warnings`, `cad_export_assembly_animation`, exportar con `assembly`).
- Interfaz: `AssemblyPanel` en `apps/web/src/components/steps/DesignStep.tsx`, arrastre en
  `apps/web/src/lib/assemblyDrag.ts` + `startPartDrag` en `CadView.tsx`, store en `lib/cad.ts`.
- Pruebas: `libs/cad/model/tests/assembly.rs` (6) y `assembly_relations.rs` (19).

Ya hay: instancias (de otras pestañas, en otra configuración, subensambles rígidos), 8 uniones
(pegada, bisagra, deslizante, cilíndrica, plana, rótula, paralela, pasador en ranura),
acoplamientos (engrane con planetarios, piñón-cremallera, tornillo; detección de engranes
enlazados), grados libres, arrastre por bisagra/deslizante, choques y holguras a pedido,
explosionada radial en pantalla, lista de materiales, plano 2D con globos, GLB animado, exportar.

## Fases (en el orden en que conviene hacerlas)

Todas hechas el 2026-11-09 («dale con PLAN_ENSAMBLE completo»). Probado con `cargo test`
(`libs/cad/model/tests/assembly_plan.rs`, 13 pruebas, más `tabs.rs`) y pruebas de node
(`apps/web/e2e/assemblyTools.test.mjs`); **falta probarlo en la app real**.

### EN0 — Limpieza chica ✅
- `Assembly::prune` (Rust) y `pruneAssembly` (TS, en cada `editAssembly`): uniones de instancias
  borradas, acoplamientos de uniones borradas, grupos, patrones y pasos colgando.

### EN1 — Uniones ligadas a la pieza ✅
- `Connector.at: Option<ConnectorRef>` (`Face`, `EdgeCenter`, `EdgeMid`, `Vertex`) + `offset`,
  `spin`, `reverse`, `radius`. `assembly_refs::prepared/solve_with` rehace los conectores contra la
  pieza de su instancia (por etiquetas y, si no, por geometría) antes de cada resolución; lo
  guardado queda de respaldo y `AssemblyView.lost` avisa. `AssemblyView.connectors` lleva los
  rehechos al front (arrastre y dibujo con `resolvedAssembly`). Proyectos viejos: sin `at`, igual.

### EN2 — Elegir bien el punto de unión ✅
- PickMode `asm_point`: cara, arista (centro si es redonda; Alt = punto medio) o vértice;
  `cad_assembly_connector_at`. Girar ±90°/libre, voltear, separar (`adjustConnector`: los de antes
  sin `at` lo llevan puesto). Conectores dibujados (`CadViewer.setTriads`). Vista previa: la unión
  se agrega enseguida y queda en el editor (la pieza B se mueve); «Cancelar» la saca.

### EN3 — Editar una unión ✅
- `MateEditor` (components/design/AssemblyParts.tsx): tipo, volver a elegir cada lado, nombre.
  Al pasar el mouse por una unión: sus caras resaltadas (`cad_assembly_mate_faces`) y sus conectores.

### EN4 — Uniones y opciones que faltan ✅
- Separación en plana y pegada (`distance`), `Angle`, `Tangent` (radios de los conectores),
  `Width` (tercer conector `c`). Topes `angle_limits`/`distance_limits` (fila que solo pesa al
  pasarse; lo impuesto y `sweep` se quedan en el tope). Acoplamientos `Linear`, `Belt`, `Cam`
  (perfil por tramos, `cam_lift`). La correa se arma a mano con la razón de radios (no lee `belts.rs`).

### EN5 — Que diga qué falla, y que aguante muchas piezas ✅
- `AssemblySolution.mate_errors`, `redundant`, `instance_dof`; marcas en la lista, piezas de
  uniones que fallan en ámbar, «Resaltar las que se mueven». Solver por grupos de piezas
  conectadas y jacobiano **por bloques** (numérico, solo re-evalúa las uniones de cada pieza; no
  analítico). Banco: 100 piezas / 150 uniones en 10 ms (release).

### EN6 — Mover piezas con el mouse ✅
- Arrastre general con el solver (`assembly::drag`: rótula fantasma y después se resuelve sin ella)
  para piezas sin bisagra/deslizante, en el plano de la pantalla, pasos de 1 mm (Mayús: libre).
  Girar 90° por eje del mundo en la tarjeta de la instancia. No hay manija 3D tipo gizmo.

### EN7 — Choques en vivo ✅
- Al cambiar el ensamble (soltar un arrastre, mover un ángulo) se revisan solos en segundo
  plano (700 ms después): piezas que chocan en rojo. Primer choque de un recorrido:
  `first_clash` / `cad_assembly_first_clash` («¿Choca en el recorrido?» en cada paso).

### EN8 — Manejo de piezas ✅
- `Assembly.patterns` (fila, círculo, sobre los otros agujeros iguales con `hole_offsets`): las
  copias siguen al original en el solver. `Instance.mirror` (+ «Copia en espejo»), `hidden`,
  `suppressed`, `color`, `Assembly.groups`. Reemplazar = cambiar la pieza; las uniones se
  rehacen por geometría y las que no se encuentran salen como perdidas («Quitar las uniones perdidas»).

### EN9 — Tornillos que se ponen solos ✅
- `cad_assembly_fasten`: por cada agujero, tornillo Allen ISO de la medida (`size_for_hole`) y el
  largo normal justo, arandela y tuerca opcionales, en «Piezas compradas», cabeza del lado de la
  cámara, unidos con bisagras ligadas a la cara del agujero.

### EN10 — Animación dentro de la app ✅
- `Assembly.motion` (pasos), `sweep_steps`, `cad_assembly_sweep`; en el visor `moveAssemblyMesh`
  mueve cada pieza en la malla (play/pausa, barra de tiempo). GLB desde los pasos.

### EN11 — Explosionada por pasos ✅
- `Assembly.explode` (instancias, dirección, distancia), deslizador por pasos con recorridos
  punteados; hoja del plano 2D «Explosionada» (`cad_drawing(exploded)`: globos y vistas corridos).

### EN12 — STEP con estructura ✅
- `cad_write_step_assembly` (XCAF: cada pieza una vez, componentes con ubicación, subensambles
  anidados, nombres y colores); `StepTree` en la app para la pestaña de ensamble y el ensamble del
  diseño. Con compensación de impresora sigue pieza por pieza.

### EN13 — Subensambles que se mueven ✅
- `Instance.flexible`: `Tabs::flat_assembly` abre el subensamble (números desde `INLINE_BASE`,
  no se guardan; `set_flat_assembly` los quita): lo fijo de adentro va en un grupo con la instancia
  (`carrier`, no se dibuja) y lo libre entra al solver. Las configuraciones de adentro van con la
  de base; lo de adentro se edita en su pestaña.

### EN14 — Diseñar dentro del ensamble ✅
- «Editar en contexto»: `cad_assembly_in_context` agrega al diseño de la pieza un `Derived` por
  cada otra instancia con `placement` (su pose vista desde la pieza) y `context` (solo de
  referencia: `Evaluation.context`, no se juntan, no se exportan, no son objetos, se ven grises).
  La geometría sigue al original sola; la posición se pone al día volviendo a apretar el botón.

## Notas para trabajar
- Textos nuevos de la interfaz con `tl()` y su traducción en el catálogo `en.<zona>.ts`.
- Antes de cada release: `tsc --noEmit` y `cargo check -p pinocchio-app`.
- e2e solo si el usuario lo pide; pruebas del solver en `libs/cad/model/tests/assembly*.rs`.
