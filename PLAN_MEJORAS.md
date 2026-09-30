# Plan de mejoras de flujo y estabilidad

Plan del 2026-09-30, hecho y ejecutado de forma autónoma. Cada paso se
verifica antes de pasar al siguiente (typecheck, build del frontend,
`cargo test --workspace`, clippy). El estado de cada paso está al lado.

## Criterios

1. **No perder trabajo** es lo primero: cualquier acción que reemplaza el
   proyecto pregunta, y solo cuando hay algo sin guardar.
2. **Ningún error queda mudo** ni deja la interfaz trabada (banderas de
   "procesando", barras de progreso colgadas).
3. **Sin fugas**: listeners, temporizadores y recursos de three.js se
   liberan.
4. Cambios pequeños y verificables; nada que cambie formatos de archivo sin
   migración.

## Pasos

### 1. Flujo de guardado ✅
- `project_changed` (backend): compara la huella del estado con la del
  último guardado o apertura. La copia de recuperación lleva su propia
  huella, así guardarla no cuenta como "guardado".
- Abrir proyecto, proyecto nuevo, importar modelo y crear modelo del
  escáner preguntan **solo si hay cambios sin guardar** (antes: siempre al
  abrir o crear, y nunca al importar un modelo encima del trabajo).
- Cerrar la ventana con cambios sin guardar pregunta antes.
- Copia de recuperación siempre activa (cada 5 minutos, aunque el guardado
  automático esté apagado). Se borra al guardar o al cerrar sin cambios,
  así solo se ofrece después de un cierre inesperado o descartado.

### 2. Estabilidad del frontend ✅
Auditoría de `App.tsx`, `Viewer3D.ts` y componentes (18 hallazgos). Corregidos:
- **Cambio de proyecto protegido**: abrir, importar, modelo del escáner y
  proyecto nuevo no arrancan si hay otra tarea larga, y el guardado
  automático espera a que terminen (antes podía escribir el proyecto nuevo
  sobre el archivo del anterior).
- **Proyecto abierto a medias**: si falla la restauración de la interfaz, el
  próximo Guardar pregunta dónde (no pisa el archivo anterior).
- `isProcessing` es un contador: la tarea que termina primero no libera los
  botones mientras otra sigue.
- Importar cambia el nombre del archivo solo si sale bien.
- Los avisos de progreso tardíos no vuelven a mostrar la barra.
- Una confirmación nueva resuelve la anterior como "no" (antes quedaba
  colgada).
- Deshacer y rehacer muestran el error; el pincel de pesos escribe primero
  en el backend y después en el visor.
- Carga tolerante: configuraciones encima de sus valores por defecto, clips
  mal formados descartados, historial inválido reiniciado.
- Escalar y retiempo rechazan factores ≤ 0 (antes el clip quedaba con el
  fin antes del inicio y la reproducción no paraba).
- El editor de texturas suelta su contexto WebGL al cerrarse.
- "Probar rango" se puede repetir y se corta al cambiar de cuadro o de clip.
- La elección de plantilla de esqueleto ignora respuestas viejas y vuelve
  atrás si falla.
- Los atajos no corren detrás de Configuración; el visor ignora tamaño 0.
- `ErrorBoundary` y aviso en la barra de estado para errores sin atrapar.

### 3. Verificación ✅
- `cargo test --workspace` (incluida la app de escritorio): todo pasa.
- `cargo clippy --workspace --all-targets`: sin errores (se corrigió uno en
  `converter-usda` que cortaba la ejecución). Quedan ~30 avisos de estilo
  del clippy 1.94 (bucles con índice, `if` anidados); no se tocaron para no
  arriesgar el comportamiento de código numérico. CI no corre clippy.
- `tsc --noEmit` y `vite build` limpios; la interfaz compilada arranca en
  Chromium sin caer en la pantalla de error.
- **No probado en la app de escritorio real** (el contenedor no tiene
  pantalla): conviene probar abrir, guardar, cerrar con cambios e importar.

## Pendiente (siguiente ronda)
- `Viewer3D.dispose` no quita sus listeners globales (solo afecta la recarga
  en caliente del desarrollo).
- Algunas llamadas sin `try` propio (`handleOrigin`, vista previa de UV,
  recarga de texturas desde GIMP, conexión del escáner): hoy llegan a la
  barra de estado por el aviso global, pero sin un mensaje específico.
- `applyTransform`, cambio de plan de cuerpo y recarga de materiales no
  descartan respuestas viejas (mismo patrón que se usó en la plantilla).
- Indicador visual de "cambios sin guardar" en el título.
- Avisos de estilo de clippy.

## Hecho antes en la misma sesión
- FK e IK conviven sin pisarse (`chainBlend`, `tidyClips`, ids estables del
  rig automático).
- Pestaña **Biblioteca** en el editor de pose y rig: animaciones básicas,
  poses de fábrica y poses guardadas en un solo lugar.
