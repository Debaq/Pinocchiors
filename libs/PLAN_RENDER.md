# Render

Plan acordado el 2026-10-12. Meta final: un **motor de render propio en Rust** (path tracing,
imágenes y animaciones). Se llega por pasos: primero una imagen en alta calidad con el motor
del visor, sin cámaras todavía; después cámaras y luces como objetos; al final el motor.
Path tracing dentro del visor (three-gpu-pathtracer) queda **descartado**.

## Dónde vive en la interfaz

- Botón **Render** en la pestaña **Exportar** (`ExportStep`), junto al de exportar el modelo.
  Abre un diálogo propio (`ConfirmDialog`-style, sin diálogos nativos) con las opciones.
- Sin cámaras (fase 1), el render sale **de lo que se ve en pantalla en ese momento**: misma
  posición, giro y zoom de la vista. Si la relación de aspecto elegida no es la de la ventana,
  se recorta alrededor del centro; el visor muestra un **marco de render** translúcido con el
  área exacta que saldrá.
- Con cámaras (fase 2), el render sale de la **cámara activa**; sin ninguna activa, de la vista.

## Fase 1 — Imagen con el motor del visor

**Estado (2026-10-12): núcleo hecho, sin commitear.** Hecho: botón Render en Exportar (también
Ctrl+Shift+R), `RenderDialog` (panel flotante sin velo), marco de render en el visor,
`Viewer3D.renderStill` + `lib/renderStill.ts` (mosaicos de 2048, supersampling 1/4/16 + MSAA 4x,
fondo transparente/color/visor, PNG/JPG/WebP, qué se oculta), halo del glow incluido, guardado por
el comando `write_render_image` (cuerpo crudo, sin JSON), opciones y carpeta recordadas, i18n.
Verificado en Chromium headless (tamaño, color exacto, alfa, mosaicos sin costuras, cancelar).
Falta: sombras suaves, GTAO, atajo en `shortcutDefs` ya está pero no se ha probado en Tauri real,
y probar el flujo completo (guardar archivo) en la app de escritorio.

Alcance: captura de calidad, no luz global. Es lo que ya dibuja Three, mejor configurado.

- Diálogo: tamaño (Pantalla, 1080p, 4K, personalizado; relación bloqueable), fondo
  (transparente / color / el del visor), calidad (muestras de antialiasing: 1, 4, 16), formato
  (PNG, JPG, WebP) y qué se oculta (grilla, esqueleto, gizmos, resaltados de selección).
- `Viewer3D.renderStill(opts)`: render fuera de pantalla a un target del tamaño pedido
  (a mosaicos si pasa del máximo de textura de la GPU), con supersampling por acumulación de
  cuadros con la cámara sacudida medio píxel (así no hace falta MSAA gigante).
- Mejoras que sí caben en el motor del visor: sombras suaves de la luz principal (PCF/VSM),
  oclusión ambiental (GTAO), luz de entorno (RoomEnvironment ya está en las bolitas), el halo
  del glow (`glowHalo.ts`) y la transmisión del vidrio (`MeshPhysicalMaterial`).
- Guardar con el diálogo de archivo de Tauri; recordar la última carpeta y las opciones
  (`createPersisted`).
- Atajo reasignable en el área "Exportar/archivo" (Configuración → Atajos).
- i18n: `t()` con el español como clave, inglés en el catálogo de su zona.

Verificación: render del mismo modelo a 1x y a 4x, ver que el encuadre coincide con el marco;
fondo transparente con canal alfa real (abrir en GIMP); no queda ninguna guía.

## Fase 2 — Cámaras, luces y mundo como objetos

Se necesitan antes del motor, porque el motor las usa y el render de fase 1 también mejora.

- **Cámara**: objeto del Outliner (posición, hacia dónde mira, distancia focal en mm, sensor,
  relación de aspecto, recorte). "Mirar por la cámara" y "Crear cámara desde la vista".
  Se guarda en el proyecto (`.pinocchio`, DTOs con `serde(default)` para no romper viejos).
- **Luces**: sol, puntual, foco, área (rectángulo). Hoy hay un solo rig (`lightRig.ts`);
  pasan a objetos con color e intensidad en vatios/lux razonables.
- **Mundo**: color de fondo o imagen HDR (equirectangular) para luz de entorno.
- El render de fase 1 usa la cámara activa y las luces del proyecto en vez del rig fijo.
- Se hacen en el `Scene` pivote de converter (nodos con cámara/luz) para que el motor Rust
  lea lo mismo y glTF las exporte (`KHR_lights_punctual`, cámaras de glTF).

## Fase 3 — Motor de render propio en Rust

Crate nuevo `libs/render/` (pide permiso antes de crear la carpeta). CPU primero, con rayon;
el GPU (wgpu compute) queda como fase aparte si hace falta velocidad.

Entrada: el `Scene` de converter (mallas, materiales PBR, texturas, esqueleto y animación) más
cámara, luces y mundo de la fase 2. Los materiales ya son los de glTF, así que el mapeo es 1:1.

| Paso | Qué |
|---|---|
| 3a | Escena a triángulos en mundo, **BVH** (hay una en `rebuild.rs` de quadriflow para tomar la idea), rayos primarios, sombreado difuso + emisión. PNG de salida. |
| 3b | **BRDF completo**: metal/rugosidad GGX, barniz (clearcoat), emisión, normal maps, oclusión, UV y texturas con filtrado. Opacidad (alpha blend y mask). |
| 3c | **Luces y entorno**: sol, puntual, foco, área; HDRI con muestreo por importancia; **MIS** (muestreo múltiple) para bajar ruido. |
| 3d | **Vidrio y líquidos**: transmisión con IOR, Fresnel, absorción por volumen (`KHR_materials_volume`: grosor y color de atenuación), rebotes internos. |
| 3e | **Cámara real**: distancia focal, apertura y profundidad de campo, desenfoque de movimiento opcional. |
| 3f | **Denoise** (filtro à-trous guiado por albedo y normales; Open Image Denoise solo si el empaquetado lo permite). |
| 3g | **Progreso en la app**: pasadas progresivas con vista previa que se afina, mosaicos, **Cancelar**, tiempo estimado (como ya hacen retopología y remallar, en segundo plano). |
| 3h | **Animación**: un fotograma por pose evaluada en CPU (esqueleto + pesos), secuencia PNG y, si ffmpeg está, video; reanudar un render interrumpido. |

Decisiones abiertas (para cuando lleguemos):
- BVH propia o el crate `bvh`/`embree` (Embree es rapidísimo pero trae dependencia C++; la
  compilación a wasm y el empaquetado pesan).
- Cuánto del Principled BSDF de Blender copiar; arrancar con el modelo de glTF y sumar.
- Si el motor corre dentro de la app o también como `apps/render` (CLI como `apps/converter`).

Verificación (obligatoria desde 3a):
- **Prueba del horno blanco**: esfera blanca difusa dentro de luz uniforme sale del mismo
  brillo que el fondo (conserva energía).
- Caja de Cornell y escenas de referencia comparadas por diferencia con imágenes guardadas.
- Mismo material en el visor y en el motor: las bolitas de presets deben parecerse.
- Tests de Rust para cada BRDF (reciprocidad, energía ≤ 1) y para el muestreo.

## Riesgos

- Rendimiento en CPU: un render limpio de una escena grande tarda; por eso 3f y 3g van pronto.
- Esqueleto y pesos: hoy el skinning corre en GPU; para renderizar poses hace falta evaluarlo
  en CPU (`pinocchio-core` ya tiene la matemática).
- Colores y espacio lineal/sRGB: guardar siempre en sRGB al final, trabajar en lineal.
- Diferencias visor ↔ motor: el visor es una aproximación; se documenta qué cambia (sin cáusticas
  en el visor, sin sombras suaves de área, etc.).

## Orden de trabajo

1. Fase 1 entera (botón en Exportar + marco de render + diálogo).
2. Fase 2 (cámaras, luces, mundo).
3. Fase 3 de 3a a 3g; 3h al final.
