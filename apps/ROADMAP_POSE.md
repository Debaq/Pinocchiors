# Roadmap: pose, IK, captura de movimiento y animación

Borrador del 2026-09-29. Reúne todo lo que se puede agregar al rig y a la
animación, ordenado por dependencias. Las fases con número menor desbloquean
las siguientes. Dentro de cada fase, los puntos van de más a menos valor.

---

## Estado actual

- **Esqueleto**: cada hueso es solo `nombre + posición + padre`. No tiene
  orientación propia (roll), límites, grupos ni propiedades.
- **Pose**: FK por articulación. El giro de la articulación J mueve a sus
  hijos. Solo la raíz se traslada. En el visor, R gira y G mueve la raíz.
- **Animación**: clips con keys por nombre de hueso, interpolación lineal o
  escalonada, slerp, auto-key, hoja de claves (`Timeline.tsx`) y barra de
  reproducción.
- **Animaciones básicas**: `presetAnimations.ts` analiza el cuerpo en cadenas
  (`ChainKind`: pata, brazo, ala, cola, cabeza, trompa, oreja, antena, pinza,
  aleta, tentáculo…), detecta lados izquierdo y derecho, y genera caminar,
  correr, trotar, saludar, aletear, nadar y reptar. Los pies patinan porque
  todo es FK.
- **Exportación**: glTF con joints como nodos. Las interpolaciones se hornean
  por cuadro en el backend. El import de rigs muestrea clips y pierde las
  traslaciones de las articulaciones que no son raíz.
- **No hay**: IK, restricciones, límites, controles separados de los huesos,
  curvas, capas, espejo de pose, biblioteca de poses, captura de movimiento
  ni retargeting.

## Principios

1. **Los formatos de salida solo entienden FK.** glTF y USD no guardan IK ni
   restricciones. Todo lo de este roadmap se evalúa en la app y se **hornea**
   a giros por cuadro al exportar. El horneado ya existe; hay que ampliarlo.
2. **Hueso de deformación ≠ control.** Los objetivos IK, poles y
   manijas son objetos aparte que no deforman la malla. El animador toca los
   controles y los huesos siguen.
3. **Una sola pila de evaluación por cuadro:** keys FK → restricciones → IK →
   límites → pose final. El visor, el horneado y la captura usan la misma.
4. **Todo se deshace y todo se guarda** en el `.pinocchio`.
5. **La UI muestra Z arriba** (`lib/axes.ts`), también en los paneles 2D de
   límites.
6. **Genérico por especie.** Lo que se pueda derivar de `ChainKind` y del
   BodyPlan se genera solo: cuadrúpedos, aves, artrópodos, delfines, no solo
   humanos.

---

## Fase 0: Fundamentos del modelo de rig (bloquea todo lo demás)

**Hecha (2026-09-29).** `apps/web/src/lib/rig.ts` (modelo, orientación,
grupos, simetría, pila y horneado), `components/panels/RigPanel.tsx` (panel
del hueso, grupos, controles y reposo, en Animar), `apply_rest_pose` en el
backend y traslación de cualquier articulación en glTF, BVH e import.
Decisión: la pila vive solo en el frontend y la exportación recibe los clips
horneados (una key por cuadro, reducida). Los datos del rig van en
`ui.rig` del proyecto (versión 1; los proyectos viejos cargan sin nada).

- **Orientación local por hueso.** Ejes estables (dirección del hueso + roll)
  calculados al ajustar el esqueleto y editables después. Sin esto no hay
  plano de doblez para rodillas y codos, ni límites por eje, ni espejo
  correcto.
  - Roll automático: el eje de bisagra perpendicular al plano que forman
    padre, hueso e hijo. En cadenas rectas se usa la orientación del cuerpo
    (adelante/arriba del BodyPlan).
  - Herramienta para recalcular el roll ("alinear a vista", "alinear a
    normal", "igual que el espejo").
- **Propiedades por hueso**: grupo/colección, color, visible, bloqueado,
  deforma sí/no, ejes bloqueados (rotar solo en X…), modo de rotación
  (cuaternión o Euler con orden elegido), forma de dibujo.
- **Traslación de cualquier articulación**, con opción de bloquearla por
  hueso. Esto también arregla la pérdida de traslaciones al importar rigs.
- **Entidades de control**: tipo nuevo en el esqueleto, con padre opcional,
  forma (cubo, círculo, flecha, esfera) y keys propias. Los clips pasan a
  tener tracks de huesos **y** tracks de controles.
- **Pila de evaluación** única (ver principios), en el frontend para el visor
  y replicada en Rust para el horneado. Alternativa: hornear desde el
  frontend y mandar los cuadros al backend. Es más simple y evita dos
  implementaciones; decidir antes de F2.
- **Pose de reposo explícita**, separada de la pose del cuadro 0. Acciones
  "aplicar pose como reposo" (vuelve a ligar la piel) y "volver a reposo".
- **Formato `.pinocchio`**: versión nueva del DTO con todo lo anterior y
  migración de los proyectos viejos.

## Fase 1: Herramientas de pose básicas

**Hecha (2026-09-29).** `lib/poseTools.ts` (reiniciar, espejo, copiar/pegar
con portapapeles entre proyectos, intermedia, empujar/relajar, biblioteca),
poses de fábrica en `lib/presetAnimations.ts` (`availablePoses`,
`generatePose`) y `components/panels/PosePanel.tsx`. Atajos: A / Alt+A,
[ / ] (Shift suma), Ctrl+Shift+M, Shift+G, Alt+R / Alt+G, Ctrl+C / Ctrl+V
(Shift: espejada). La barra de estado muestra la articulación activa, su
giro en ejes locales y los ejes bloqueados (los límites llegan con F4).

Todo esto funciona con FK. Da mucho valor con poco riesgo.

- **Selección**
  - Clic en hueso o articulación (ya existe). Mayús suma, doble clic
    selecciona la cadena completa hasta la bifurcación.
  - Seleccionar el espejo (izquierda ↔ derecha), los hijos, el padre o
    todo el grupo.
  - Grupos/colecciones de huesos con color (patas delanteras, columna,
    cara…), generados solos desde `ChainKind` y lados.
  - Ocultar o aislar grupos.
- **Transformar**
  - Gizmo de rotación con anillos por eje y trackball.
  - Espacio local, del padre o global.
  - Restringir a un eje con X/Y/Z mientras se gira, con valor numérico
    tecleado.
  - Rotación incremental con Ctrl (pasos de 5°/15°).
- **Reiniciar**: devolver giro, traslación o todo a reposo, por hueso,
  selección o esqueleto completo.
- **Espejo de pose**
  - Copiar el lado izquierdo al derecho y al revés.
  - Voltear la pose completa.
  - Pegar espejado.
  - Se apoya en los pares que ya detecta `analyzeBody` y en el roll de F0.
- **Copiar/pegar pose** entre cuadros, clips y proyectos (portapapeles
  interno por nombre de hueso).
- **Biblioteca de poses**
  - Guardar la pose completa o de un grupo con miniatura.
  - Aplicarla con un deslizador de mezcla del 0 al 100 %.
  - Poses de fábrica por `ChainKind`: mano abierta/cerrada, sentado,
    agachado, alas plegadas…
- **Pose intermedia (breakdown)**: deslizador entre la key anterior y la
  siguiente, con empujar/relajar (exagerar o suavizar hacia el reposo).
- **Estadística de pose en la barra de estado**: hueso activo, giro en
  grados por eje y si viola un límite (cuando exista F4).

## Fase 2: IK

- **IK analítico de 2 huesos + pole** (ley de cosenos, sin iteraciones).
  Patas, brazos y la mitad superior de las alas.
  - Estiramiento opcional (el hueso se alarga si el objetivo queda fuera de
    alcance) y "suavizado de extensión total" para que la rodilla no salte
    al estirarse.
- **Mezcla IK/FK por cadena**, animable (0–1).
  - **Igualar IK→FK y FK→IK** para cambiar de modo sin que la pose salte.
- **IK automático al arrastrar**: sin controles. Arrastras una punta con G y
  la cadena la sigue hasta la raíz o hasta la articulación fijada. Sirve para
  posar rápido.
- **Pie invertido (reverse foot)**: controles de talón, punta y balanceo,
  para que caminar ruede el pie en vez de levantarlo plano.
- **Fijar (pin)**: keys de "plantado" por pie o mano. En esos cuadros el
  efector queda quieto en el mundo aunque el cuerpo se mueva. Acaba con el
  patinaje de las animaciones básicas.
- **Solucionadores iterativos**: FABRIK para cadenas largas (cuello, cola,
  tentáculo, trompa del mamut) y CCD como alternativa. Ambos con límites.
- **IK por spline**: la cadena sigue una curva con 3–5 controles. Es lo
  natural para colas, trompas, serpientes y tentáculos.
- **Mirar a (look-at)** para cabeza y ojos, con reparto del giro a lo largo
  del cuello (por ejemplo 20/30/50 %).
- **Rig automático de controles** desde el BodyPlan:
  - pata/brazo → IK 2 huesos + pole + pie invertido;
  - cola/trompa/tentáculo → IK por spline;
  - cabeza → look-at;
  - raíz → control de centro de masa.
- **Animaciones básicas con IK**: `presetAnimations` genera trayectorias de
  pies (apoyo en el suelo + arco en el aire) y deja que el IK resuelva las
  piernas. Se hornea a FK al final, así el resto del sistema no cambia.
- **Suelo**: el efector no baja del plano del suelo (y más adelante, de la
  malla del terreno).

## Fase 3: Restricciones (constraints)

Pila ordenada por hueso, con influencia animable en cada una.

- **Copiar**: giro, posición o escala de otro hueso o control (con ejes,
  espacio y mezcla).
- **Hijo de (child of)**: cambiar de padre en el tiempo. Ejemplos: la mano
  agarra un objeto, el pie pasa a ser hijo del suelo.
- **Seguir (damped track)** y **estirar hacia (stretch to)**, con
  preservación de volumen. Sirve para tendones y músculos falsos.
- **Límites de giro y posición** como restricción. Es la base de F4.
- **Mapeo de transformación**: el giro de A en X de 0–90° da el giro de B en
  Z de 0–45°.
- **Drivers**: una expresión o curva que liga un valor a otro. Ejemplo: un
  deslizador "cerrar mano" dobla los 15 huesos de los dedos.
- **Reparto automático de giro**: al girar la columna, el giro se reparte
  entre las vértebras.

## Fase 4: Panel de articulación (la idea de la diseñadora)

**Sí tiene sentido, y bastante.** Es lo que usan los editores de ragdoll de
los motores de juego y herramientas de animación como Cascadeur o KineFX,
pero casi siempre como números sueltos. Un panel lateral visual que junte
relaciones, límites y movimiento sería un diferenciador real. Además, los
mismos límites alimentan el IK (F2), la limpieza de captura (F6) y la física
(F8), así que no es un adorno: es el lugar donde se define cómo se mueve cada
parte.

Idea: clic en un hueso o en un grupo y el panel lateral muestra estas vistas.

### 4.1 Vista de límites de una articulación

El giro de una articulación se descompone en **swing** (hacia dónde apunta
el hueso) y **twist** (cuánto gira sobre su propio eje). Los dos se pueden
dibujar en 2D.

- **Bisagra (rodilla, codo, dedo)**: un transportador (arco) con el rango
  mín–máx arrastrable, la aguja del ángulo actual y el reposo marcado.
- **Rótula (hombro, cadera, cuello)**: un **disco de swing**. El centro es
  la dirección de reposo del hueso y la distancia al centro es cuánto se
  inclina (proyección azimutal). El límite es una curva cerrada editable:
  elipse con dos radios, o polígono con puntos arrastrables para formas
  anatómicas asimétricas. El punto actual se mueve en vivo al posar.
- **Anillo de twist** aparte, con su rango mín–máx.
- **Rigidez por zona** (opcional): un degradado dentro del disco. Cerca del
  borde, el IK y la física "cuestan más", así el límite se siente blando y
  no como una pared.
- Editar en el disco actualiza la vista 3D: se dibuja el cono o arco en la
  articulación seleccionada, con el hueso fantasma en los extremos del rango.
- **Probar rango**: botón que barre la articulación por todo su límite en el
  visor para ver cómo deforma la piel.

### 4.2 Movimiento en el tiempo

- La **trayectoria del clip** se dibuja dentro del disco: una línea que
  recorre las posiciones del hueso cuadro a cuadro, coloreada por tiempo.
  Los tramos fuera del límite salen en rojo.
- Arrastrar un punto de la trayectoria edita la key de ese cuadro.
- Una mini curva por eje debajo (giro vs tiempo), que es un editor de curvas
  reducido para ese hueso.
- En la hoja de claves, marcas rojas donde algún hueso viola su límite.

### 4.3 Vista de grupo o cadena

Al seleccionar varios huesos (una pata, la columna, una mano):

- **Esquema de perfil**: la cadena proyectada en su plano de doblez, como un
  diagrama de rango de movimiento de anatomía. Tiene un arco de rango en cada
  articulación y se posa arrastrando en 2D.
- **Discos pequeños en fila**, uno por articulación, con edición en lote:
  mismo límite para todos, escalar rangos, copiar al lado espejo.
- **Zona de alcance** del efector: la mancha 2D o 3D de dónde puede llegar
  la punta con esos límites. Sirve para ver si una pata alcanza el suelo o
  una mano la boca.

### 4.4 Relaciones

- **Grafo de relaciones**: nodos = huesos y controles, aristas =
  jerarquía, IK, restricciones y drivers, con colores por tipo. Se edita
  arrastrando (conectar = crear restricción). Solo muestra el vecindario del
  hueso seleccionado para no saturar.
- **Influencia en la piel**: qué región de la malla mueve el hueso (ya existe
  la vista de pesos) y qué huesos comparten zona con él.

### 4.5 Límites automáticos

Nadie quiere cargar 100 límites a mano.

- **Anatómicos por `ChainKind`**: rodilla de pata, codo, cuello, cola, aleta…
  con tablas por especie (el codo del cuadrúpedo se dobla al revés que la
  rodilla).
- **Por la malla**: girar la articulación hasta que la piel se autointerseca
  o se aplasta demasiado y poner el límite un poco antes.
- **Por lo observado**: el rango que usa una animación importada o una
  captura, con margen.
- **Simétricos**: calcular un lado y espejar al otro.

### 4.6 Riesgos del panel

- Hay que elegir bien la proyección del disco. La azimutal equidistante no
  deforma cerca del reposo y deja representar hasta 180°.
- Con Euler los límites se ven bien por eje pero fallan cerca del gimbal
  lock. Por eso conviene swing-twist, y mostrar Euler solo como lectura
  numérica.
- Probar primero con la diseñadora sobre un prototipo HTML suelto (ver
  "Probar componentes sueltos" en la memoria del proyecto) antes de
  integrarlo.

## Fase 5: Edición de animación

- **Curvas Bézier** por key: manijas, automático, vector, aceleración/freno
  preestablecidos. También la interpolación "constante" que ya existe.
- **Editor de curvas** (graph editor) completo: por canal, normalizar, marco
  a selección, filtro Euler (quita saltos de ±180°), suavizar, reducir keys
  con tolerancia.
- **Hoja de claves mejorada**: fila resumen por grupo, filtros por
  selección, escalar keys alrededor del cursor, marcadores con nombre.
- **Capas / NLA**:
  - clips en pistas que se suman o reemplazan (caminar + respirar + mirar);
  - repetir con desfase, mezcla entre clips (transiciones), recortar;
  - máscara por grupo (esta capa solo afecta la cabeza).
- **Papel cebolla**: fantasmas de la malla o de los huesos en cuadros
  vecinos.
- **Trayectorias de movimiento** (motion paths) de cualquier hueso o
  control en 3D, con los puntos de key editables arrastrando en el visor.
- **Cerrar ciclo**: igualar el último cuadro al primero con tangentes
  continuas.
- **Retiempo (time warp)**: acelerar o frenar un tramo sin mover keys a mano.
- **Suavizado y ruido**: filtros aplicables a una selección de keys (útiles
  después de una captura).

## Fase 6: Captura de movimiento con MediaPipe

MediaPipe (Google, licencia Apache 2.0) trae modelos listos que corren en el
navegador vía WASM + WebGL (`@mediapipe/tasks-vision`) o en otros runtimes:

| Modelo | Da | Uso en la app |
|---|---|---|
| Pose Landmarker | 33 puntos del cuerpo, también en coordenadas 3D del mundo (metros, origen en la cadera) | cuerpo completo |
| Hand Landmarker | 21 puntos por mano, 3D | dedos |
| Face Landmarker | 478 puntos + 52 blendshapes (estándar ARKit) + matriz de la cabeza | cara, giro de cabeza |
| Holistic | los tres juntos | captura completa |

### 6.1 Fuentes de video

- **Archivo de video** (primero): se procesa offline cuadro a cuadro sin
  problemas de permisos ni de tiempo real. Da el resultado más limpio.
- **Webcam en vivo**: vista previa sobre el modelo mientras se graba.
  - Ojo: en Linux la app corre en WebKitGTK. `getUserMedia` tiene que
    habilitarse en la configuración del webview, y la aceleración WebGL de
    MediaPipe puede caer a CPU. Alternativa: capturar en Rust por V4L2 (ya
    existe en `libs/orizon3d/src/capture.rs`) y pasar cuadros al frontend.
- **Cámara RGB del Revopoint** vía Orizon3D: reutiliza el mismo camino V4L2.
- **Múltiples cámaras** (futuro): triangular dos vistas calibradas para una
  profundidad mucho mejor que la monocular.

### 6.2 Tubería

1. **Detección** por cuadro, en un Web Worker para no trabar la UI.
2. **Limpieza**
   - Filtro 1€ (One Euro): quita el temblor sin agregar retardo.
   - Descartar puntos con baja visibilidad y rellenar huecos por
     interpolación.
   - Corregir cambios de lado (izquierda/derecha intercambiados en un
     cuadro).
3. **Calibración**: T-pose o A-pose al inicio para medir las proporciones del
   actor. Altura para escalar al modelo.
4. **Contactos**: detectar cuándo un pie está quieto en el suelo (velocidad
   baja + altura mínima) y generar keys de "plantado" de F2. Esto quita el
   patinaje, el defecto más visible de la captura monocular.
5. **Puntos → giros (retarget)**
   - Para cada hueso, construir un marco local desde tripletes de puntos
     (hombro-codo-muñeca da la dirección y el plano de doblez → pole).
   - La raíz toma la posición de la cadera y la orientación del torso
     (caderas + hombros).
   - Resolver brazos y piernas con el IK de F2 contra los puntos, en vez de
     copiar ángulos. Así las proporciones distintas del modelo no rompen los
     contactos.
6. **Límites** de F4: recortar lo imposible (rodilla al revés por ruido).
7. **Reducir keys** y guardar como clip normal, editable con todo lo demás.

### 6.3 Qué esqueletos puede animar

- **Humanoides**: mapeo directo por `ChainKind` y lados.
- **Otras especies (títeres)**: MediaPipe solo entiende humanos, pero se
  puede mapear libre. El panel de mapeo asigna puntos del actor a controles
  del modelo. Ejemplos:
  - brazos del actor → patas delanteras del cuadrúpedo, piernas → traseras;
  - la mano del actor mueve el control spline de la trompa;
  - la cabeza del actor mueve la cabeza del animal.
  Plantillas de mapeo por BodyPlan, guardables.
- **Captura parcial**: grabar solo cabeza o solo manos y mezclarlo como capa
  (F5) sobre una animación existente.
- **Cara**: los 52 blendshapes necesitan shape keys en el modelo (F9).
  Mientras no existan, solo se usan el giro de la cabeza y la apertura de
  la mandíbula (si hay hueso).

### 6.4 Limitaciones honestas

- La profundidad desde una sola cámara es ruidosa. El giro del antebrazo
  (twist) y los dedos ocluidos son poco fiables.
- Ropa suelta, poca luz o el actor cortado por el borde de la imagen
  degradan mucho el resultado.
- Una persona por captura para empezar (el modelo soporta varias).
- Hay modelos más precisos de video a 3D (familia HMR/WHAM), pero son
  pesados y en Python/GPU. Quedan como opción futura en un proceso aparte,
  no en la app.
- Para animales reales existen modelos de pose animal (DeepLabCut,
  SuperAnimal). Es investigación, no para las primeras versiones.

### 6.5 UI de grabación

- Cuenta regresiva, grabar/parar, vista previa en vivo (video con los puntos
  dibujados + modelo animándose al lado).
- Recortar el tramo útil antes de convertir.
- Deslizadores de suavizado y umbral de contacto con vista previa inmediata.
- Botón "convertir a clip". El resultado aparece en la línea de tiempo como
  cualquier otro.

### 6.6 Otros formatos de captura

- **Importar BVH** (texto simple, el formato clásico de mocap). Entra por la
  misma tubería desde el paso 5.
- **Exportar BVH** para llevar las animaciones a otras herramientas.
- Animaciones de otros rigs glTF: el import de rig ya existe. Falta
  retargetearlas a *otro* esqueleto (F7).

## Fase 7: Retargeting general

- Pasar un clip de un esqueleto a otro:
  - mapeo automático por `ChainKind`, lados y nombres, con tabla editable;
  - compensar proporciones (escala de zancada, altura de cadera);
  - mantener contactos con IK.
- Plantillas de mapeo guardadas por par de BodyPlans (humano ↔ cuadrúpedo
  ↔ ave…).
- Las animaciones básicas y las capturas se vuelven reutilizables entre
  modelos.

## Fase 8: Movimiento secundario y física

- **Huesos resorte** (jiggle/spring): orejas, trompa, cola, papada, antenas.
  Rigidez, amortiguación y gravedad por hueso. Horneable.
- **Seguimiento automático** (overlap): la cola repite el movimiento del
  cuerpo con retardo y amortiguación, sin keys.
- **Ragdoll**: usa los mismos límites swing-twist de F4 y cuerpos de
  colisión (cápsulas) generados desde el tubo de pesos que ya calcula el
  ajuste del esqueleto.
- **Colisiones** de efectores con el suelo y con el propio cuerpo.
- **Procedurales**: respiración, parpadeo, mirar alrededor, balanceo en
  reposo. Parametrizados por especie y horneables a clip.
- **Terreno irregular**: los pies se adaptan con IK a una malla de suelo.

## Fase 9: Calidad de deformación

Lo que se ve cuando la pose ya es buena pero la piel no.

- **Skinning por cuaterniones duales** (dual quaternion): evita que la muñeca
  o el antebrazo se estrangulen al girar ("caramelo"). Se elige por malla y,
  si se puede, se mezcla con el lineal por vértice.
- **Correctivos por ángulo**: shape keys que se activan según el giro de una
  articulación (codo doblado 90° → bíceps). El panel de F4 es el lugar
  natural para crearlos: en el disco de swing se marca un punto y se esculpe
  la corrección.
- **Shape keys** en general (necesarias para la cara capturada en F6). Hoy
  la escena no las modela: hay que agregarlas a `converter-scene` y a
  glTF/USD.
- **Huesos segmentados** (bendy): un hueso con curvatura interna para
  columnas y colas suaves con pocos huesos.
- **Preservación de volumen** en articulaciones.

## Fase 10: Exportación y compatibilidad

- **Horneado completo**: IK, restricciones, capas, resortes y drivers se
  convierten en giros FK por cuadro, y luego se reducen keys con
  tolerancia.
  - Opción de exportar también los controles como nodos vacíos para
    reimportar.
- **Límites y metadatos** en `extras` de glTF (para motores que armen el
  ragdoll solos) y como atributos personalizados en USD.
- **BVH** de salida (ver F6).
- Verificación como hasta ahora: gltf-validator + abrir en otras
  herramientas + comparar la FK de la app contra el archivo (el test
  `samples` del import de rig).

---

## Orden sugerido

| Orden | Qué | Por qué |
|---|---|---|
| 1 | F0 (roll, propiedades, controles, pila de evaluación) | todo depende de esto |
| 2 | F1 espejo, copiar/pegar, grupos, reiniciar | mucho valor, poco riesgo |
| 3 | F2 IK 2 huesos + pole + fijar pies + animaciones básicas con IK | quita el patinaje, lo más visible |
| 4 | F4 panel de articulación (4.1 + 4.5) | la idea de la diseñadora; lo necesitan el IK y la captura |
| 5 | F5 curvas Bézier y editor de curvas | ya estaba pendiente; necesario para pulir capturas |
| 6 | F6 MediaPipe desde archivo de video, humanoides | primer resultado de captura |
| 7 | F2 FABRIK / spline, F3 restricciones | colas, trompas, agarres |
| 8 | F6 en vivo + títeres para otras especies, F7 retargeting | captura útil para cualquier modelo |
| 9 | F4 resto (trayectorias, grafo, alcance), F5 capas | pulido de la edición |
| 10 | F8 física, F9 deformación, F10 extras | calidad final |

## Preguntas abiertas

- ~~¿La pila de evaluación vive solo en el frontend o se duplica en Rust?~~
  Decidido en F0: solo en el frontend; el backend recibe cuadros horneados.
  Si hace falta exportar desde la CLI, se porta entonces.
- ¿Cuaterniones en todas partes o Euler visible para el animador?
  Recomendación: guardar cuaterniones, mostrar Euler de solo lectura y usar
  swing-twist en los límites.
- ¿El panel de F4 es una pestaña fija del panel derecho en "Rig y
  animación" o un panel flotante que aparece al seleccionar un hueso?
  Decidirlo con la diseñadora sobre el prototipo.
- ¿Se descargan los modelos de MediaPipe (varios MB) con la app o se bajan
  la primera vez? Offline es más coherente con una app de escritorio.
