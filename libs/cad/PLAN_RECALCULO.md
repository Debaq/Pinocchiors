# Recálculo incremental y en segundo plano

Objetivo: que cambiar una cota al final de un historial largo no recalcule todo desde cero y
que la interfaz nunca se congele mientras OpenCASCADE trabaja.

## Qué tenemos hoy

- `eval::evaluate(doc)` (`model/src/eval.rs`) recorre todas las operaciones en orden cada vez:
  resuelve parámetros, resuelve cada sketch, construye cada sólido y aplica booleanas.
- `apps/desktop/src/cad.rs` guarda en `cad_cache` solo el último resultado (cuerpo, mallas,
  versión). Cada comando que cambia el documento llama a evaluar entero y luego tesela el
  cuerpo completo (`VIEW_DEFLECTION = 0.05`, `VIEW_ANGLE = 0.25`).
- El cálculo corre dentro del comando de Tauri: no bloquea la ventana, pero el frontend espera
  la respuesta y no hay cancelación; un cambio rápido de valores encola recálculos.

## Qué falta

| Falta | Efecto hoy |
|---|---|
| Caché por operación | Un redondeo al final de 30 operaciones rehace las 29 anteriores |
| Saber qué depende de qué | No se puede invalidar solo lo afectado |
| Cancelar un cálculo viejo | Valores intermedios se calculan aunque ya no sirvan |
| Progreso | En operaciones lentas (redondeos, vaciados) no hay indicio de qué pasa |
| Teselar solo lo que cambió | Se rehace la malla de todo el cuerpo |

## Diseño

### Caché por operación

- `EvalCache { entradas: Vec<Entrada> }` en `cad-model`, una por operación, con:
  - **huella** de la entrada: hash del JSON de la operación ya resuelta (con fórmulas aplicadas)
    + huella de la entrada anterior + versión de los parámetros que usa;
  - estado después de la operación: el `Tagged` del cuerpo (forma + orígenes de caras), las
    herramientas (`tools`) y el estado (`FeatureStatus`).
- `evaluate_with(doc, &mut cache)`: recorre las operaciones; mientras la huella coincide, toma
  el estado guardado; en la primera que difiere, recalcula desde ahí y reemplaza el resto.
- Como el historial es lineal, alcanza con encontrar el primer cambio; no hace falta un grafo
  completo para la primera versión. Excepción: patrones y simetrías que usan herramientas de
  operaciones anteriores (ya quedan en la huella porque se encadenan).
- Los sketches se guardan aparte con su propia huella (geometría + restricciones + plano
  resuelto); cambiar una cota de un sketch solo resuelve ese sketch.
- `Shape` es un manejador a un `TopoDS_Shape` compartido (inmutable), así que guardar estados
  intermedios cuesta poco en memoria.

### Hilo de cálculo y cancelación

- El estado CAD de la app pasa a un hilo propio con una cola de pedidos. Cada pedido lleva la
  versión del documento; si llega uno nuevo, el pendiente se descarta antes de empezar.
- Cancelar un cálculo en curso: OCCT permite `Message_ProgressRange` en las operaciones
  booleanas, redondeos y mallado; se pasa un indicador que el hilo puede cortar. Requiere
  agregar el parámetro en el puente C (`cad_occt.cpp`) para las operaciones lentas.
- El frontend recibe eventos (`cad://progreso`, `cad://resultado`) en vez de esperar la
  respuesta del comando. Mientras calcula, el sólido anterior queda visible con un indicador.

### Teselado por cara

- Teselar por cara y guardar la malla junto con la huella geométrica de la cara (tipo de
  superficie + caja + área). Al cambiar el cuerpo, las caras que no cambiaron reutilizan su
  malla. El historial de OCCT (`Modified`/`IsDeleted`) dice cuáles cambiaron.
- Enviar al frontend solo las caras nuevas o cambiadas, con ids estables.

## Fases

1. **Medición**: registrar el tiempo de cada operación y de teselar (`FeatureStatus.ms`) y
   mostrarlo en el árbol (útil para ver qué es lento). Banco con un documento de 30 operaciones.
   *Hecha el 2026-10-07: `FeatureStatus.ms` (en centésimas, para que el JSON se relea
   exacto); el árbol muestra lo que tarda cada operación de 100 ms o más. Banco:
   `cargo test --release -p cad-model --test cache bench -- --ignored --nocapture`.*
2. **Caché por operación** en `cad-model` con huellas; la app la mantiene entre comandos.
   *Hecha el 2026-10-07: `EvalCache` (mapa huella → estado después de la operación, con
   límite de 128 y descarte de lo menos usado) y `evaluate_with`. La huella encadena la
   anterior con id, suprimida, detrás de la barra y el JSON de la operación ya resuelta
   (fórmulas aplicadas); el nombre no entra. Como es un mapa y no una lista, la vista previa
   y el documento guardado comparten el prefijo. `AppState::cad_ops` la guarda entre
   comandos; `CadResult.recomputed` dice cuántas se calcularon. Prueba: con y sin caché dan
   idénticos estados, volumen y orígenes de caras tras cambiar cada operación de un
   documento de 8; cambiar la i-ésima recalcula n − i. Banco de 30 operaciones: todo 506 ms;
   cambiar el chaflán final con caché 9–10 ms.*
3. **Hilo de cálculo** con cola y descarte de pedidos viejos; eventos al frontend.
4. **Cancelación** dentro de OCCT para booleanas, redondeos y vaciados.
5. **Teselado por cara** con reutilización.

## Pruebas

- Modelo: evaluar con caché da exactamente el mismo resultado (volumen, orígenes de caras) que
  sin caché, después de cambiar cualquier operación (prueba que recorre cada operación de un
  documento de ejemplo y cambia un valor).
- Contadores: cambiar la última operación recalcula 1; cambiar un parámetro usado solo al final
  no recalcula lo de antes.
- Banco: tiempo de cambiar la distancia del último redondeo en el documento de 30 operaciones,
  antes y después (objetivo: proporcional a la última operación, no al total).
- E2E: escribir rápido varios valores; el resultado final corresponde al último.

## Riesgos

- **Huellas incompletas** → resultados viejos. Mitigación: la huella incluye todo lo que la
  operación lee (incluidos parámetros y referencias resueltas), y un modo de depuración compara
  contra la evaluación completa.
- **Memoria** con muchos estados intermedios grandes: límite configurable y descartar los más
  antiguos.
- **Hilos y OCCT**: OCCT es seguro entre hilos si cada forma se usa en uno a la vez; el historial
  de operaciones es por hilo (`thread_local`), lo que ya encaja con un hilo de cálculo único.
