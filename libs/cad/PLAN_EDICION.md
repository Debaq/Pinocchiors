# Edición de operaciones con vista previa

Objetivo: crear y editar operaciones como en Onshape: se abre un diálogo de la operación, el
sólido muestra la vista previa mientras se cambian los valores y la selección, y recién al
confirmar (✓) queda en el historial. Las referencias rotas se ven y se arreglan eligiendo de
nuevo.

## Qué tenemos hoy

- Agregar una operación (`AddSection` en `DesignStep.tsx`) la crea de inmediato con valores por
  defecto y lo elegido en el visor (regiones, aristas o caras); después se ajustan los campos en
  el árbol.
- Cada cambio de campo hace `commit` del documento y recalcula todo: deshacer guarda cada paso
  intermedio (escribir "25" deja dos pasos: "2" y "25").
- El árbol muestra el estado de cada operación (`FeatureStatus`) y permite suprimir, borrar,
  renombrar y "Retroceder hasta acá" (`Document.rollback`).
- Las referencias a caras y aristas (`FaceRef`/`EdgeRef`) se resuelven por origen; si no se
  encuentran, la operación queda con error, pero no se muestra **qué** referencia falló ni se
  puede volver a elegir sin borrarla.

## Qué falta

| Falta | Por qué importa |
|---|---|
| Diálogo de operación con ✓ / ✗ | Probar valores sin ensuciar el historial; cancelar vuelve atrás de verdad |
| Vista previa en vivo | Ver el resultado antes de aceptar; en Onshape la herramienta se ve de otro color |
| Selección dentro del diálogo | Cada campo de referencia (caras, aristas, regiones, sketch, eje) es una "caja" que se llena eligiendo en el visor, con lista y quitar |
| Referencias rotas marcadas | Hoy solo "error"; hay que decir cuál y dejar elegir otra |
| Doble clic para editar | Abrir el diálogo desde el árbol o desde el sólido (clic en una cara → su operación) |
| Barra de retroceso arrastrable | Hoy es un menú; Onshape la arrastra en el árbol |
| Carpetas en el árbol | Agrupar operaciones en historiales largos |
| Insertar en medio | Crear una operación con el retroceso puesto la inserta ahí (ya funciona en el modelo, falta hacerlo visible) |

## Diseño

### Borrador de operación

- Estado nuevo en `cadUi.ts`: `draft: { featureId | null, kind, campos, selecciones }`.
- Mientras hay borrador, el documento que se evalúa es una **copia** con la operación insertada
  (crear) o reemplazada (editar). El documento real no cambia hasta ✓.
- ✓ hace un único `commit` (un solo paso de deshacer). ✗ descarta la copia.
- Atajos: Enter = ✓, Esc = ✗ (si no hay un campo con foco).

### Vista previa

- Primera versión: se evalúa la copia completa con retardo corto (150 ms después del último
  cambio), igual que hoy pero sin `commit`. Con el recálculo incremental
  ([PLAN_RECALCULO.md](PLAN_RECALCULO.md)) esto pasa a ser instantáneo en documentos largos.
- La herramienta de la operación (el prisma de una extrusión, la pieza que se resta) se devuelve
  aparte en `CadResult` (`tool_mesh`) y se dibuja transparente: verde si suma, roja si resta,
  como Onshape.
- Si la operación falla, el sólido de antes queda visible y el diálogo muestra el error.

### Cajas de selección

- Cada campo de referencia del diálogo tiene una caja: activa = lo que se elija en el visor va
  ahí; muestra la lista ("Cara 3", "Arista 7"); cada ítem se resalta al pasar el mouse y tiene ✗.
- Filtro automático: la caja de aristas solo acepta aristas, la de caras solo caras planas si
  la operación lo exige (p. ej. desmoldeo con plano neutro).
- Reutiliza la selección de `cadUi.ts` (`picks`); al abrir el diálogo lo ya elegido se carga en
  la caja que corresponda.

### Referencias rotas

- `eval.rs` devuelve, por operación, qué referencias no se resolvieron (índice dentro del campo)
  en vez de un error genérico.
- En el diálogo esas referencias salen en rojo con "No encontrada"; elegir otra en el visor la
  reemplaza. En el árbol la operación muestra el ícono de advertencia.

### Árbol

- Doble clic en una operación → abre el diálogo de edición.
- Clic en una cara del sólido con Alt (o menú contextual "Editar operación") → busca qué
  operación creó la cara por sus orígenes (`FaceTag.feature`) y abre su diálogo.
- **Barra de retroceso**: línea arrastrable entre filas; soltarla equivale a "Retroceder hasta
  acá". Mientras está puesta, lo de abajo se ve atenuado (ya pasa) y lo nuevo se inserta ahí.
- **Carpetas**: `Document.folders: Vec<Folder { name, features: Range }>`; solo presentación,
  no cambian el orden de evaluación. Plegables. Arrastrar operaciones para reordenar (validando
  que no queden antes de lo que usan).

## Fases

1. **Borrador y ✓/✗** para las operaciones existentes, con vista previa por evaluación de la
   copia. Un paso de deshacer por operación. *Hecha el 2026-10-05: el borrador vive en el store
   (`Draft`); el backend lo recibe con `cad_preview` y lo evalúa sin tocar el documento guardado
   (ni el `.pinocchio`); la vista previa se calcula hasta la operación en edición. Los sketches
   siguen entrando directo (tienen su propia edición).*
2. **Cajas de selección** en el diálogo (regiones, caras, aristas, sketch). *Hecha el 2026-10-06: `SelectionBox`
   (aristas, caras, una cara para "hasta una cara") y `RegionBox`; activa, la vista previa
   muestra el sólido de antes de la operación (`Draft.selecting`) y `cad_resolve_refs` dice
   qué arista o cara es cada referencia para resaltarla, quitarla con otro clic o marcarla "no
   encontrada". El sketch sigue en una lista (no hay nada que elegir en el visor).*
3. **Herramienta transparente** (verde/roja) en la vista previa. *Hecha el 2026-10-06: `cad_tool_mesh`
   tesela la herramienta guardada en la evaluación (`Evaluation::tool`) en vez de sumarla a
   `CadResult`; el store la pide tras cada vista previa de una operación con `op`.*
4. **Referencias rotas**: reporte por referencia, marcado y reemplazo. *Hecha el 2026-10-07:
   `MissingRef` por campo y posición; en listas se sigue con lo que queda
   (`FeatureState::Warning`), con referencia única falla; la caja marca las perdidas, se activa
   sola al editar y lo elegido reemplaza a la primera perdida.*
5. **Árbol**: doble clic, editar desde la cara, barra arrastrable, reordenar.
6. **Carpetas.**

## Pruebas

- Modelo: evaluar con una operación de borrador no modifica el documento; reporte de
  referencias no resueltas indica operación e índice.
- E2E: crear extrusión, cambiar distancia tres veces, ✗ → documento igual al inicial; repetir y
  ✓ → un solo paso de deshacer. Romper un redondeo (borrar la cara de la que dependía), ver la
  arista en rojo, elegir otra y confirmar.
- E2E: arrastrar la barra de retroceso y verificar el volumen del sólido en cada posición.

## Riesgos

- **Dos fuentes de verdad** (documento y borrador) pueden desincronizarse si otra acción cambia
  el documento con el diálogo abierto. Mitigación: con un borrador abierto, las acciones que
  cambian el documento quedan bloqueadas o cierran el diálogo preguntando.
- **Costo de la vista previa** en historiales largos hasta tener el recálculo incremental.
- **Reordenar** puede dejar referencias hacia adelante; validar con el grafo de dependencias.
