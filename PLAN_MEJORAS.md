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

### 2. Estabilidad del frontend
Auditoría de `App.tsx`, `Viewer3D.ts` y componentes: promesas sin
capturar, banderas que quedan pegadas, listeners sin limpiar, recursos sin
liberar, divisiones por cero, carreras al cambiar de proyecto, atajos que
se disparan al escribir, `localStorage` sin protección, carga de
proyectos viejos o dañados. Se corrigen los hallazgos confirmados.

### 3. Verificación
- `cargo test --workspace` (incluida la app de escritorio) y clippy sin
  avisos.
- `tsc --noEmit` y `vite build`.

## Hecho antes en la misma sesión
- FK e IK conviven sin pisarse (`chainBlend`, `tidyClips`, ids estables del
  rig automático).
- Pestaña **Biblioteca** en el editor de pose y rig: animaciones básicas,
  poses de fábrica y poses guardadas en un solo lugar.
