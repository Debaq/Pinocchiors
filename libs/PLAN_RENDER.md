# Render

Plan acordado el 2026-10-12. Meta final: un **motor de render propio en Rust** (path tracing,
imágenes y animaciones). Se llega por pasos: primero una imagen en alta calidad con el motor
del visor, sin cámaras todavía; después cámaras y luces como objetos; al final el motor.
Path tracing dentro del visor (three-gpu-pathtracer) queda **descartado**.

## Dónde vive en la interfaz

- Botón **Render** en la pestaña **Exportar** (`ExportStep`), junto al de exportar el modelo.
  Abre el espacio Render (`RenderStep`, panel flotante sin velo, sin diálogos nativos) con las opciones; el formato de imagen elegido en Exportar (PNG/JPG/WebP) también abre Render.
- Sin cámaras (fase 1), el render sale **de lo que se ve en pantalla en ese momento**: misma
  posición, giro y zoom de la vista. Si la relación de aspecto elegida no es la de la ventana,
  se recorta alrededor del centro; el visor muestra un **marco de render** translúcido con el
  área exacta que saldrá.
- Con cámaras (fase 2), el render sale de la **cámara activa**; sin ninguna activa, de la vista.

## Fase 1 — Imagen con el motor del visor

**Estado (2026-10-12): núcleo hecho y commiteado.** Hecho: botón Render en Exportar (también
Ctrl+Shift+R, área «file», `openRender` en `App.tsx`), panel Render (`RenderStep.tsx`; la fase 1 lo tuvo como `RenderDialog`, ya no existe), marco de render en el visor,
`Viewer3D.renderStill` + `lib/renderStill.ts` (mosaicos de 2048, supersampling 1/4/16 + MSAA 4x,
fondo transparente/color/visor, PNG/JPG/WebP, qué se oculta), halo del glow incluido, guardado por
el comando `write_render_image` (cuerpo crudo, sin JSON), opciones y carpeta recordadas, i18n.
Verificado en Chromium headless (tamaño, color exacto, alfa, mosaicos sin costuras, cancelar).
Verificado a ojo en Chromium headless (2026-10-12): sombras suaves (penumbra con 4 cuadros), GTAO (contacto esfera/losa) y suelo con sombra (a la altura de la base del modelo) funcionan y se activan/desactivan por separado. Falta: el atajo (definido en `App.tsx`) no se ha probado en Tauri real,
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

## Fase 2 — Cámaras, luces y mundo (HECHA, 2026-10-12, simplificada)

El programa es para mostrar y explicar, no un editor visual de escenas: en vez de objetos del
Outliner, todo son **opciones del panel Render** (espacio Render, Alt+7).

- **Cámaras predefinidas** (`lib/renderCameras.ts`): vista actual, frente, atrás, izquierda,
  derecha, arriba, abajo y 3/4. Encuadran el modelo entero para la relación de aspecto de la imagen.
  Zoom compartido o por cámara; el ojo lleva el visor a esa cámara. Salida: una imagen por cámara
  (sufijo = nombre en el idioma de la interfaz) u hoja de 2 a 4 columnas con rótulos.
- **Iluminación** (`lib/renderLights.ts`): presets (visor, estudio, suave, contraluz, dramática)
  que reemplazan las luces solo mientras se renderiza.
- **Mundo**: fondo del visor / transparente / color / degradado vertical (se pinta al final bajo
  la imagen) y suelo invisible que recibe la sombra (`ShadowMaterial`).
- Descartado: cámaras/luces como nodos del `Scene` y HDR propio. Si el motor Rust (fase 3)
  los necesita, salen de estos presets.

## Fase 3 — GeppettoRay, motor de render propio en Rust

Motor llamado **GeppettoRay** (el carpintero que da vida a Pinocchio). Crate `libs/geppettoray/` (ya creado). CPU primero, con rayon;
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

**Estado (2026-10-12): 3a hecha.** `libs/geppettoray/`: BVH propia (SAH por cubetas), `RayScene::from_scene` (vía `world_primitives`), cámara pinhole, path tracer difuso + emisión con ruleta rusa, entorno uniforme y sol con muestreo directo, PNG sRGB. Tests: horno blanco, caja cerrada (serie geométrica), BVH vs fuerza bruta, sol sobre plano. Siguiente: 3b.

**3b hecha (2026-10-12):** BSDF glTF (difuso Ashikhmin–Shirley + GGX Smith correlacionado + Schlick, barniz como 2.º lóbulo GGX, muestreo VNDF y mezcla de lóbulos), texturas PNG/JPEG/WebP con filtrado bilineal y repetición (base y emisión en sRGB→lineal), metal/rugosidad desde textura, mapa de normales (tangentes por triángulo desde las UV, no MikkTSpace), color de vértice, opacidad Mask y Blend estocástica (también en rayos de sombra), unlit. Decisiones: el mapa de oclusión se **ignora** a propósito (el path tracing calcula la oclusión real; aplicarlo la duplicaría); sin mipmaps (el antialiasing por muestreo lo compensa); solo set UV 0 y escala UV. Tests: reciprocidad, energía ≤ 1, pdf, textura, máscara/mezcla, espejo metálico, mapa de normales plano, horno blanco con material completo.

**3c hecha (2026-10-12):** `Light::{Sun, Point, Spot}` exactas; luces de **área = cualquier triángulo emisivo sin textura** (`quad()` arma un rectángulo; se eligen por potencia luminancia×área, muestreo uniforme sobre el triángulo); entorno uniforme o **HDRI equirectangular** (`EnvMap::from_bytes`, .hdr de Radiance, intensidad y giro; distribución por luminancia×senθ). **MIS** con heurística de potencia entre el muestreo de luces y el del BSDF; `max_depth` cuenta rebotes y el último vértice solo recoge emisión (así el MIS no pierde energía al cortar). Pendiente menor: luces de área con emisión texturizada se ven pero no se muestrean; el foco y el sol no tienen tamaño (sombras duras; el sol con disco llega con la cámara real). Tests: ley del cuadrado inverso, cono del foco, emisor enorme = cielo uniforme, pdf del HDRI normalizada y sin sesgo, horno blanco con HDRI+MIS, MIS = referencia por fuerza bruta.

**3d hecha (2026-10-12):** vidrio y líquidos con interfaz lisa: Fresnel dieléctrico exacto, Snell, reflexión total interna, elección reflexión/refracción por probabilidad de Fresnel; `transmission` mezcla estocástica con el material opaco; volumen cerrado (`thickness > 0`) con absorción de Beer–Lambert donde el color base es la transmitancia tras recorrer `thickness`; `thickness = 0` = lámina delgada (sin refracción, tiñe con el color base). Los eventos de vidrio no cuentan como rebote. El IOR también fija el F0 del especular dieléctrico. Sombras de vidrio: el rayo de sombra pasa con la probabilidad de transmisión (sin cáusticas; la luz no se desvía). Límites: sin rugosidad en la transmisión (vidrio esmerilado), la escena de converter no trae `attenuationColor/Distance` (se usa color base + grosor), sin medios anidados (un líquido dentro de un vaso), sin cáusticas. Tests: Fresnel, Snell, TIR, Beer–Lambert, lámina con absorción contra solución analítica con reflexiones múltiples, horno blanco de vidrio (IOR 1.33/1.5/2.4), esfera como lente que invierte la imagen.

**3e hecha (2026-10-12):** `Camera::from_focal_length` (focal en mm y alto de sensor → campo de visión), lente fina con `Lens {radius, focus_distance, blades}` (`aperture_radius(focal, f/N, mm_a_unidades)`, `focused_on(punto)`), disco de apertura por mapeo concéntrico o polígono de N aspas (bokeh poligonal), y desenfoque de movimiento **solo de cámara** (`with_motion`: pose inicial → final dentro del obturador). Sin lente ni movimiento el rayo es el mismo de siempre. El de objetos en movimiento queda para 3h (animación). Tests: focal→campo de visión, radio de apertura, convergencia de los rayos en el plano focal, bokeh sin sesgo, movimiento, y una luz puntual fuera de foco que se reparte en un disco.

**3f hecha (2026-10-12):** `render_aov` (mismo color que `render` + albedo, normal, posición, profundidad y varianza del promedio por píxel; capas con hasta 8 muestras, cámara sin lente) y `denoise` (à-trous espacial estilo SVGF: filtra la iluminación = color/albedo y la vuelve a multiplicar por el albedo, así las texturas no se desdibujan; pesos por normal, plano tangente, albedo y luminancia guiada por la varianza prefiltrada 5×5; recorte de luciérnagas contra la media de los 8 vecinos; 4 pasadas por defecto). Open Image Denoise queda descartado por ahora (dependencia C++). Medido: a 4 muestras el error cuadrático baja ~55 % en una escena de esfera con sol y ~83 % en un plano con textura de rayas y penumbra, sin perder contraste de la textura y sin correr los bordes de geometría. Límites: es solo espacial (sin acumulación temporal para animación), los reflejos nítidos y el vidrio no se separan del fondo (el AOV usa el primer cruce), y la profundidad de campo no se refleja en las capas (el desenfoque queda como ruido). Tests: reducción del error contra referencia de 512 muestras, el color de `render_aov` = el de `render`, imagen limpia intacta, bordes de geometría, texturas, luciérnagas. Los tests de `geppettoray` corren con `opt-level = 2` en dev.

**3g hecha (2026-10-12), sin probar en Tauri real:** `render_progressive` (pasadas que se acumulan, mosaicos en paralelo, cancelación entre mosaicos, tiempo estimado, denoise final opcional, fondo transparente con alfa y color de fondo visible aparte del entorno que ilumina). En la app: `apps/desktop/src/ray_render.rs` (hilo aparte; comandos `ray_render_start/status/image/cancel`; PNG RGBA por pasada) y `lib/rayRender.ts` (sigue el avance cada 150 ms, baja la imagen solo si cambió, cancela con el `AbortSignal` del render del visor). Panel Render: selector **Motor** (Rápido (visor) / GeppettoRay), calidad en muestras (32/128/512/2048), "Quitar ruido", barra con pasada, muestras y tiempo que falta; la imagen se ve encima del visor mientras se afina; cámaras predefinidas, hoja de vistas, formatos y degradado siguen funcionando igual. Con GeppettoRay se desactivan sombras suaves, suelo con sombra y oclusión ambiental (la luz ya las produce).
**3h hecha (2026-10-12), sin probar en Tauri real:** `converter-scene/src/pose.rs` (muestreo de canales + `world_primitives_at` con skinning CPU), `geppettoray::{AnimatedScene, render_animation}` (`src/animation.rs`, tests en `tests/animacion.rs`), `apps/desktop/src/ray_render.rs` (`ray_render_animation_start`, PNG por cuadro, reanudar, degradado, ffmpeg), UI en `RenderStep.tsx`/`App.tsx` (`handleRenderAnimation`) y `lib/rayRender.ts` (`renderRayAnimation`). Pendiente: denoise temporal, cámara animada, probar con un rig real en la app.

**Faltas menores hechas (2026-10-12):** sol con disco y luces puntual/foco con esfera (sombras suaves); `ShadowCatcher` (suelo con sombra invisible, opacidad por luz que falta) y `suelo con sombra` activo con GeppettoRay; HDRI elegible desde el panel Render (.hdr, intensidad, giro) vía `RayRequest.hdri`; `ToneMap::Aces` como three.js (`RayRequest.tone_map`, la app lo envía siempre); emisión texturizada como luz de área (`RayScene::emission_at`, `Texture::mean_luminance`); vidrio esmerilado (VNDF + peso G1) y pila de medios para volúmenes anidados (`MediaStack`). Tests en `tests/menores.rs` (11) y `ray_render.rs` (3 nuevos) y `e2e/rayRender.test.mjs`. Quedan sin hacer (revisado 2026-10-15): cáusticas, tamaño de sol/foco editable en la UI (hoy es la constante `SUN_ANGULAR_RADIUS` en `lib/rayRender.ts`), exposición del tono (ACES con `exposure: 1.0` fijo), CLI `apps/geppettoray` (pide permiso para crear la carpeta).

Aproximaciones y límites de la conexión: las luces del visor pasan como sol principal + relleno (+ luz de cámara) y un cielo uniforme que resume ambiente y entorno (el visor usa una imagen de estudio para reflejos: no coinciden exacto); sin el tono ACES del visor (sale lineal→sRGB); la escena es la de `AppState.scene` (sin pose de esqueleto: eso es 3h). Tests: render progresivo (6), comandos de Rust (4: corre y entrega PNG con alfa, cancelar, escena vacía, validación) y `e2e/rayRender.test.mjs` (8: luces, campo de visión, petición, avance, cancelar, error).

## Fase 4 — Estudio fotográfico (pedido 2026-10-12; 4a hecha, 4b y 4c pendientes)

Motivo: el suelo solo recibía sombra (no se veía), las 4 luces solo movían la principal y el relleno, y las cámaras eran solo direcciones sin lente. Va en GeppettoRay, en el motor rápido y en el visor (con interruptor).

| Paso | Qué |
|---|---|
| 4a | **Fondo de estudio** (ciclorama): piso que sube en curva hasta un muro, sin costuras, bajo el punto más bajo del modelo y detrás de él según la cámara; color y brillo del piso. HECHO (2026-10-12). |
| 4b | PENDIENTE (hoy `lib/renderLights.ts` solo tiene presets de sol/relleno: visor, estudio, suave, contraluz, dramática). **Luces de estudio** reales: softbox de área, aro, contraluz, 3 puntos (GeppettoRay: emisores invisibles a la cámara; visor: aproximación con luces del rig). |
| 4c | PENDIENTE. **Presets de fotografía**: cámara (dirección, focal, apertura, profundidad de campo) + luces + fondo en uno (producto, retrato, macro, catálogo 3/4…). |

**4a hecha (2026-10-12), commiteada (`eafaafd`):** `geppettoray::Studio` (`studio.rs`: perfil piso→curva (16 pasos)→muro, medidas relativas a la diagonal del modelo: 7× al frente, 2,5× al muro, 9× de semiancho, radio 0,9×, alto 7×; una cara, así que desde abajo no tapa) + `RayMaterial::casts_shadow` (el estudio recibe sin proyectar: la luz llega al modelo aunque venga de detrás del muro) + `AnimatedScene::with_studio` y `AnimationSettings.studio` (todos los cuadros). App: `RayRequest.studio {color, gloss}` (mide la escena en reposo; desactiva el suelo con sombra); TS: `lib/studioSet.ts` (misma geometría en three para el motor rápido), `Viewer3D.setStudio` (visor en vivo mientras se está en Render; el muro mira según la cámara actual y se rehace al cambiar de cámara) y tarjeta "Estudio" (fondo de estudio, color, brillo del piso). Con estudio el fondo es el ciclorama (sin transparencia ni degradado) y las sombras del motor rápido se activan solas. Tests: `tests/estudio.rs` (4), `ray_render.rs` (1), `e2e/rayRender.test.mjs` (1). Verificado a ojo con Fox.glb (mate y brillante).

Decisiones abiertas (para cuando lleguemos):
- ~~BVH propia o el crate `bvh`/`embree`~~ → propia (decidido en 3a); antes: `bvh`/`embree` (Embree es rapidísimo pero trae dependencia C++; la
  compilación a wasm y el empaquetado pesan).
- Cuánto del Principled BSDF de Blender copiar; arrancar con el modelo de glTF y sumar.
- ~~Si el motor corre dentro de la app~~ → corre dentro de la app (`ray_render.rs`); el CLI `apps/geppettoray` (como `apps/converter`) sigue sin existir.

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
