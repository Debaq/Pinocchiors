# Un solo visor

Objetivo: que Diseñar (`apps/web/src/lib/CadViewer.ts`) y los demás espacios
(`apps/web/src/lib/Viewer3D.ts`) se manejen y se vean igual, sin código copiado.
Pedido del usuario el 2026-10-08 ("el 5 me parece ideal").

## Qué hay hoy

Dos clases que crean cada una su renderer, cámara, `OrbitControls` + `NavDrag`, cubo de
vistas, giro animado hacia una vista, dibujo bajo demanda, tamaño con `ResizeObserver` y
fondo según el tema. Lo mismo, copiado, con diferencias que se notan:

| | Viewer3D | CadViewer |
|---|---|---|
| Giro hacia una vista (cubo) | `lookFrom` + `stepViewTransition` | lo mismo, copiado |
| Encuadrar | vuelve a la vista isométrica, distancia = 2 × lado | conserva la dirección, por esfera envolvente |
| Amortiguación del zoom | sí | no |
| Cursor sobre el cubo | mano | flecha |
| Grilla | por unidades (mayor y menor) | `GridHelper` fijo |
| Luces | configurables (Luces), entorno PBR, tono ACES | dos fijas, sin tono |
| Botón derecho | menú | desplazar (sin arrastre: menú) |

## Fases

1. **Cámara común** (`lib/cameraRig.ts`): navegación, cubo (clic, hover con cursor de mano),
   giro animado hacia una vista y encuadrar por esfera conservando la dirección. Las dos
   clases la usan; se borra lo copiado. Encuadrar queda como en Diseñar en todos lados.
   *Hecha el 2026-10-09: `lib/cameraRig.ts`. De paso: con el puntero bloqueado (órbita sin
   tope) `OrbitControls` quería capturar el puntero y Chromium lo rechazaba con un error;
   `NavDrag` los apaga mientras dura el arrastre.*
2. **Mismos botones**: el derecho desplaza arrastrando en los dos y sin arrastre abre el menú;
   Ctrl + medio hace zoom; modo notebook (rueda gira) también en Diseñar.
   *Hecha el 2026-10-09 (el modo notebook pasó a `CameraRig.trackpadWheel`, con la órbita
   sin tope, y Diseñar lo respeta): el derecho desplaza en el visor
   principal y el menú sale al soltarlo sin arrastrar; Ctrl + medio acerca en Diseñar. e2e
   "visor principal: derecho desplaza o abre el menú, Ver todo encuadra".*
3. **Mismo aspecto**: luces y entorno de `Viewer3D` (con el panel Luces) y la grilla por
   unidades en Diseñar.
   *Hecha el 2026-10-09: `lib/lightRig.ts` (luces, entorno PBR, tono ACES y densidad de
   píxeles) y `lib/gridLines.ts` (grilla con mayores cada 10 y ejes), usados por los dos.*
4. **Un solo canvas** (más adelante): el diseño como una capa más del visor principal, así
   pasar a Diseñar no cambia de visor. Exige mover elegir caras/aristas, sketches, vista de
   corte, manijas y planos a capas; es el paso grande.
   *Primer paso hecho el 2026-10-09: la misma cámara. `CameraPose` en metros con los ejes
   internos; Diseñar arranca con la del visor principal y al salir se la devuelve (si llega
   enseguida la malla de la pieza, el principal la conserva en vez de reencuadrar). e2e
   "visor: la misma cámara en Diseñar y en Fabricar". Lo que falta para un canvas único:
   que el visor principal dibuje las capas del CAD (cuerpo con caras y aristas elegibles,
   sketches, planos, manijas, corte) y CadView use ese canvas.*
