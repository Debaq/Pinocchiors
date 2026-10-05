# Varias piezas en un diseño

Objetivo: lo que Onshape llama *Part Studio*: un mismo historial puede crear varias piezas
separadas, cada una con nombre, color y material; las operaciones eligen a qué pieza afectan y
se pueden combinar piezas entre sí. Es la base de los ensambles
([PLAN_ENSAMBLES.md](PLAN_ENSAMBLES.md)) y de exportar o imprimir por pieza.

## Qué tenemos hoy

- `Evaluation.body: Option<Shape>`: hay un solo cuerpo. Toda operación se une, resta o
  intersecta con él (`BodyOp::{Join, Cut, Intersect}`).
- Si dos extrusiones no se tocan, el resultado es un `Compound` con dos sólidos, pero para el
  modelo es "el cuerpo": no se pueden nombrar, colorear ni exportar por separado.
- Los orígenes de caras (`FaceTag`) dicen qué operación creó cada cara, pero no a qué pieza
  pertenece.
- Exportación (`cad_export`): STEP/STL/3MF/OBJ del cuerpo entero con nombre fijo "Diseño".

## Qué falta

| Falta | En Onshape |
|---|---|
| Operación "Nueva pieza" | Extruir/revolucionar/primitiva con `Nueva` crea una pieza aparte |
| Alcance de la operación | Unir, restar e intersectar eligen con qué piezas ("Fusionar con") |
| Lista de piezas | Panel "Piezas" con nombre, visibilidad, color y material |
| Booleanas entre piezas | Unir, restar (conservando o no la herramienta), intersectar |
| Separar en piezas | Un sólido con partes no conectadas se puede partir |
| Redondeos, vaciados, etc. por pieza | Operan sobre las caras/aristas, que ya dicen su pieza |
| Exportar por pieza | STEP con varias piezas, STL/3MF por pieza o todas juntas |
| Borrar / ocultar pieza | Operación "Borrar pieza" en el historial |

## Diseño

### Modelo

- `PartId(u32)` estable, asignado al crear la pieza (como `FeatureId`).
- `Evaluation.parts: Vec<Part { id, name, shape: Tagged, color, material, visible }>`; se quita
  `body` (se mantiene como compatibilidad: la unión de todas para lo que aún lo use).
- `BodyOp` pasa a:
  - `New` — crea una pieza nueva;
  - `Join { parts: PartScope }`, `Cut { parts: PartScope }`, `Intersect { parts: PartScope }`;
  - `PartScope::Auto` = las piezas que la herramienta toca (lo que hace Onshape por defecto),
    o `Parts(Vec<PartRef>)` con la lista explícita.
- `PartRef`: referencia robusta a una pieza, por la operación que la creó y el índice de sólido
  dentro de ella (mismo criterio que `FaceTag`). Así sobrevive a recálculos.
- Nombre, color y material por pieza en `Document.part_props: BTreeMap<PartRef, PartProps>`
  (no son operaciones; no recalculan geometría).
- Operaciones nuevas:
  - `Boolean { op: Union | Subtract { keep_tools } | Intersect, targets, tools }`;
  - `SplitParts { part }` — separa los sólidos no conectados en piezas;
  - `DeletePart { parts }`.
- Documentos viejos: `Join/Cut/Intersect` sin alcance = `Auto`, y la primera operación crea la
  pieza 1. El resultado es idéntico al de hoy.

### Referencias

- `FaceRef`/`EdgeRef` agregan la pieza (opcional, por compatibilidad) para desambiguar caras
  coincidentes de piezas distintas; la resolución busca primero dentro de esa pieza.
- Redondeo, chaflán, vaciado, desmoldeo y partir operan sobre la pieza dueña de las referencias.
  Patrón y simetría aplican a las operaciones (como hoy) y su alcance hereda el de la original.

### Interfaz

- Sección "Piezas" en el panel Diseñar: lista con ojo, color, material, renombrar, "Exportar
  pieza". Clic en una pieza la elige (resalta en el visor).
- Selector "Nueva / Unir / Restar / Intersectar" en las operaciones que crean sólido, con la
  caja "Con" para elegir piezas (por defecto: las que toca).
- `CadViewer`: un objeto three.js por pieza, con su color; la elección de caras ya distingue
  piezas por el id de cara.

### Exportación

- STEP: cada pieza como sólido con nombre (XCAF `STEPCAFControl_Writer` con nombres y colores;
  necesita TKXCAF y TKLCAF en el enlace estático — medir el tamaño).
- STL/3MF/OBJ: todas en una escena con un objeto por pieza, o una pieza sola.
- 3MF lleva varios objetos con nombre, útil para el laminador.

## Fases

1. **Modelo** con `parts` y `BodyOp::New` + alcance `Auto`; compatibilidad con documentos
   viejos; un objeto por pieza en el visor.
2. **Lista de piezas** con nombre, color, visibilidad; exportar STL/3MF/OBJ por pieza.
3. **Alcance explícito** ("Con") y operación **Booleana** entre piezas.
4. **Separar y borrar** piezas.
5. **Material por pieza** (masa por pieza, enlaza con [PLAN_INSPECCION.md](PLAN_INSPECCION.md)).
6. **STEP con nombres y colores** (XCAF).

## Pruebas

- Modelo: dos extrusiones separadas con `New` → 2 piezas; una tercera con `Cut` que toca solo
  una → la otra queda igual (volumen exacto). Documento viejo de las pruebas actuales da el mismo
  volumen.
- `PartRef` sobrevive a cambiar una cota que mueve la pieza.
- Exportar STEP con dos piezas y releerlo con `cad_import_step`: dos sólidos con sus nombres.
- E2E: crear dos piezas, cambiar color de una, exportar 3MF y verificar dos objetos.

## Riesgos

- **Identidad de piezas** cuando una operación une dos piezas o parte una: definir regla clara
  (la unión conserva el `PartRef` de la primera; partir crea nuevas con índice) y probarla.
- **Tamaño del binario** si STEP con nombres exige las bibliotecas XCAF.
- **Cambio amplio** en `eval.rs` y en todo lo que hoy lee `body`; hacerlo en una rama con la
  batería de pruebas actual como red.
