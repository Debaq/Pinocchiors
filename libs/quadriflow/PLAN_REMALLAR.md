# Remallar

Plan acordado el 2026-10-08: en el espacio **Preparar**, la pestaña "Retopología" pasa a ser
**"Remallar"**, una pestaña lateral como UV o Animar, con todas las formas de ordenar una
malla. La retopología a quads es una de ellas. Este documento es solo el plan; nada está hecho.

## Por qué

Retopología no es la única forma de ordenar una malla, y según para qué sirva el modelo
conviene otra:

| Modo | Qué hace | Para qué |
|---|---|---|
| **Retopología** | Malla nueva de quads que siguen la forma | Animar, deformar, subdividir |
| **Simplificar** | Quita triángulos manteniendo la forma | Escaneos pesados para web, juegos, laminador |
| **Isótropo** | Triángulos parejos del mismo tamaño | Escaneos con triángulos alargados; base limpia |
| **Vóxeles** | Rehace la superficie desde el volumen | Mallas rotas, piezas sueltas o que se cruzan, antes de imprimir |
| **Triángulos a quads** | Junta pares de triángulos vecinos | Mallas bien hechas pero trianguladas al exportar |
| **Suavizar** | Empareja vértices sin cambiar la conectividad | Ruido de escaneo |

Buena parte del motor ya existe, pero escondido: `rebuild.rs` (vóxeles), `isotropic.rs`,
`smooth.rs` y `cleanup.rs` en `quadriflow-core` son pasos internos de la retopología; meshopt
ya es dependencia del workspace (solo para los perfiles web al exportar); `pinocchio-mesh`
tiene una decimación por agrupado de vértices (tosca: no sirve para esto).

## La interfaz

- Pestaña vertical **Remallar** (en lugar de Retopología) en el panel derecho de Preparar.
- Arriba, el **modo**: lista de los seis con una línea de "para qué" cada uno; al elegir uno se
  ven sus parámetros. Retopología es el panel de hoy (`RetopologyPanel`), tal cual.
- Abajo de los parámetros, siempre igual en todos los modos:
  - **Vista previa**: calcula sin tocar el modelo y muestra el resultado en alambre sobre el
    original (en la barra del visor: "Original / Resultado / Los dos").
  - **Antes → después**: triángulos (o quads), vértices, y la **desviación** del resultado al
    original (máxima y media, en mm y en % del tamaño), medida con la BVH que ya usa
    `rebuild.rs`.
  - **Aplicar**: reemplaza la malla del objeto. Se deshace como reparar o escalar (respaldo +
    paso del historial). Retopología conserva lo de hoy (la malla de quads aparte, que se usa
    al exportar) y suma "Usar como malla del modelo".
- Actúa sobre el **objeto elegido en el Outliner**, como las demás herramientas. Si el objeto
  sale de una pieza de Diseñar, el remallado queda colgado de él y **se rehace solo** cuando
  cambia el diseño (como lo demás hecho sobre la malla, ver PLAN_OBJETOS).
- Corre en segundo plano con progreso y **Cancelar** (la retopología ya lo hace así).

## Qué pasa con UV, textura y pesos

| Modo | UV y textura | Pesos del esqueleto |
|---|---|---|
| Simplificar | Se conservan (meshopt con atributos; costuras bloqueadas) | Se conservan (atributo más) |
| Suavizar | Intactos (misma conectividad) | Intactos |
| Triángulos a quads | Intactos (mismos vértices) | Intactos |
| Isótropo, Vóxeles, Retopología | Se pierden | Se pierden |

Para los que pierden: al aplicar, un aviso (ConfirmDialog) con tres salidas:
- **Trasladar la textura**: se despliega la malla nueva y se hornea la textura vieja por rayos
  (lo de `uv-core` que ya existe).
- **Trasladar los pesos**: cada vértice nuevo toma los pesos del punto más cercano del original,
  interpolados en su triángulo.
- **Descartar** (lo de hoy: `geometry_changed` borra rig y quads).

## Los modos, uno por uno

### 1. Retopología
Se mueve tal cual. Solo se agrega "Usar como malla del modelo".

### 2. Simplificar
- **Motor**: meshopt (`simplify_with_attributes`, con UV y normales como atributos;
  `simplify_sloppy` cuando se pide reducir mucho y la topología no importa). Para usarlo
  fuera de la exportación, la dependencia pasa a `quadriflow-core` (o a donde quede la API,
  ver abajo) con la misma feature para que wasm no compile C++.
- **Parámetros**: objetivo en % o en cantidad de triángulos; error máximo (% del tamaño);
  conservar bordes abiertos; conservar costuras de UV.

### 3. Suavizar
- **Motor**: Taubin (λ/μ), que no encoge como el laplaciano simple.
- **Parámetros**: iteraciones e intensidad; respetar aristas vivas (ángulo); solo bordes
  abiertos fijos.
- Es lo más rápido de hacer.

### 4. Isótropo
- **Motor**: `isotropic::remesh` hecho público.
- **Parámetros**: largo de arista en mm (automático = el largo medio actual); conservar
  aristas vivas (ángulo); iteraciones; más fino donde hay curvatura (`sizing.rs`, opcional).

### 5. Vóxeles
- **Motor**: `rebuild::rebuild` hecho público.
- **Parámetros**: tamaño de vóxel en mm (o resolución); suavizado después; isótropo después
  (opcional).
- **Antes de correr**: estimar vóxeles y memoria (caja / vóxel³). Con un tamaño muy fino,
  pedir confirmación.
- Cierra agujeros y une piezas que se cruzan. Por eso es la herramienta para imprimir una
  malla rota cuando Reparar no alcanza.

### 6. Triángulos a quads (algoritmo nuevo)
- Emparejar triángulos vecinos, los mejores primero: el quad que formen debe ser plano y con
  ángulos cerca de 90°.
- No cruzar aristas vivas ni costuras de UV.
- Los triángulos que queden sin pareja quedan como triángulos.
- Opción **"Todo quads"**: una subdivisión de un paso; cada triángulo da 3 quads y cada quad 4.
- `QuadMesh` hoy es solo de quads: hace falta una malla mixta, o guardar los triángulos
  sueltos aparte.
- Al exportar a GLB se triangula igual. Los quads valen en OBJ y USD, y para seguir trabajando.

### API
Un módulo público `quadriflow_core::remesh` con `simplify`, `smooth`, `isotropic`, `voxel`
y `tris_to_quads`. Todos toman un `Mesh` y devuelven uno, con un callback de progreso y
cancelación como `remesh_with_callback`. La app agrega un comando `remesh_preview` / `remesh_apply`
(con el modo y sus parámetros) y guarda la vista previa en `AppState` hasta aplicar o descartar.

## Fases

*Fase 1 hecha el 2026-10-08*: sección `remesh` ("Remallar") en lugar de `retopology`
(`legacyStep` abre ahí los proyectos viejos, también en los pasos hechos); `RemeshStep` con
los seis modos en dos columnas (los que faltan marcados "pronto" y con su aviso), el "qué hace
/ para qué" del elegido, la Retopología adentro tal cual (ya tenía "Usar esta malla en las
etapas siguientes", que es el "usar como malla del modelo" del plan) y la tarjeta común
`RemeshSummary` (antes → después y desviación), que hoy usa la retopología. El modo se guarda
en el proyecto (`ui.json` → `remesh.mode`). Prueba: e2e "remallar: los modos con la
retopología adentro" (la caja de Diseñar pasa sola a malla al ir a Preparar).

1. **Sección y estructura**.
   - Pestaña Remallar con el selector de modos y la Retopología adentro.
   - Id de sección nuevo (`remesh`); los proyectos viejos con `retopology` abren ahí.
   - El mismo armazón de vista previa, antes/después y aplicar para todos los modos.
2. **Simplificar**: meshopt, estadísticas, desviación y aplicar con respaldo y deshacer.
3. **Suavizar**: Taubin.
4. **Isótropo y Vóxeles**: exponer lo de `quadriflow-core` con parámetros en mm y la
   estimación de memoria.
5. **Triángulos a quads**: el algoritmo, la malla mixta y "Todo quads".
6. **Conservar textura y pesos** en los modos que los pierden (horneado y transferencia).
7. **Objetos de Diseñar**: el remallado como operación del objeto que se rehace sola.

## Pruebas

- **Rust**, por modo:
  - Simplificar: cantidad pedida ± 5 %, desviación bajo el error pedido, sigue cerrada si
    entró cerrada.
  - Suavizar: el volumen no cae más de 1 % (Taubin) y el ruido baja.
  - Isótropo: el desvío del largo de arista es chico.
  - Vóxeles: la salida es cerrada y manifold aunque la entrada tenga dos cáscaras que se
    cruzan.
  - Triángulos a quads: un plano triangulado vuelve a ser todo quads.
- **Banco** con los modelos de `~/Descargas` (gonfoterio, conejo, audiómetro, molde): tiempo
  y desviación de cada modo a varias densidades. Una sola corrida es ruido.
- **Interfaz**: con el arnés de la app sin Tauri (HTML temporal que imita `__TAURI_INTERNALS__`),
  elegir modo, vista previa, aplicar y deshacer.

## Riesgos

- **Memoria** de Vóxeles con tamaños finos: estimar y avisar antes.
- **meshopt es C++**: mantenerlo detrás de la feature (wasm no lo compila).
- **Perder UV o pesos sin darse cuenta**: siempre el aviso con las tres salidas.
- **Malla mixta tri/quad**: toca `QuadMesh` y la exportación; hacerla al final (fase 5).
