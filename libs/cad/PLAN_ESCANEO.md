# Plan: diseñar sobre escaneos y mallas

Plan desde el 2026-10-10. Pedido del usuario: «quiero trabajar como un profesional creando
piezas sobre escaneos o stl previos». Dos usos, los dos importan, **primero B**:

- **B — Diseñar algo nuevo que calce** sobre el escaneo: adaptador que encaja, soporte o base,
  carcasa, férula, molde o negativo.
- **A — Reconstruir** la pieza escaneada como CAD limpio y editable (ingeniería inversa).

El usuario no tiene escáner por ahora: todo se prueba con STL/OBJ cargados (fase E0).

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

## Punto de partida (lo que ya hay)

| Función | Estado |
|---|---|
| Escaneo visible translúcido en Diseñar | ✅ (`CadViewer.setScan`) |
| Plano de trabajo / contorno + extrusión desde zona plana | ✅ (`ScanFeature::WorkPlane/PlaneOutline`, profundidad por rayos) |
| Agujero o tetón desde zona cilíndrica | ✅ (`ScanFeature::Cylinder`) |
| Corte de la malla por plano → sketch | ✅ (`ScanFeature::Slice`, círculos, paralelas y perpendiculares) |
| Detección automática (plano, cilindro, esfera) | ✅ (`cad_scan::detect_all`, `merge_coplanar`) |
| Desviación diseño ↔ escaneo | ✅ (`cad_scan::deviation`, mapa de color) |
| Limpiar la malla (reparar, remallar, simplificar, suavizar) | ✅ (Preparar) |
| Escaneo como cuerpo para booleanas | ❌ (`Shape::from_mesh` existe en `cad-occt`, sin usar) (E2) |
| Elegir qué objeto es el escaneo | ❌ hoy es «el modelo cargado» (`with_scan`) (E1) |
| Alinear el escaneo por rasgos | ❌ Objeto solo da piso/frente (E1) |
| Referencias (punto, plano, eje) tomadas de la malla | 🟡 solo plano de zona plana (E1) |
| Sketch anclado a la malla | ❌ `solidRefs` solo mira el sólido (E1) |
| Extruir hasta el escaneo | ❌ `UpToFace` solo caras del sólido (E3) |
| Formas que siguen la piel (engrosar parche, carcasa, molde) | ❌ (E4) |
| Curvas sobre la malla | ❌ (E5) |
| Verificar calce (choque/holgura firmada) | 🟡 desviación sin signo de calce (E6) |
| Ajuste de cono, toro, redondeos | ❌ (E7) |
| Revolución / recubrimiento desde cortes | ❌ (E8) |
| Asistente de reconstrucción | ❌ (E9) |
| Superficies libres ajustadas a la malla | ❌ (E10) |

## Fases — B (diseñar sobre el escaneo)

### E0 — Banco sin escáner

- Ejemplo `cad-scan --example scanify <entrada> <salida> [opciones]`: vuelve «escaneo» un STL/OBJ
  limpio: ruido sobre la normal (σ en mm), huecos (parches borrados), densidad dispareja
  (remallado isótropo con tamaño variable), base recortada (como apoyado en la mesa del
  escáner) y pose al azar (rotación + traslación). Semilla fija para repetir.
- Lista de modelos de `~/Descargas` con su uso: orgánicos para B (gonfoterio, conejo, oído
  interno), mecánicos para A (piezas de Onshape «Part Studio …», molde, espéculo, audiómetro).
- **Medición que decide el motor de E2**: booleana OCCT contra el escaneo cosido a 5 k, 20 k y
  50 k triángulos (tiempo y si falla). La bitácora ya anota que OCCT hace mal las booleanas
  orgánicas y que manifold podía volver para eso. Si OCCT pasa de ~2 s en 20 k o falla,
  las booleanas con escaneo van por mallas (manifold) y el resultado es una pieza malla
  (exporta STL/3MF/OBJ, no STEP; se avisa).

### E1 — El escaneo como referencia del diseño

- El escaneo es un objeto elegido en el Outliner (no «el modelo cargado»): `Document.scans`
  guarda id de objeto + huella; si el objeto cambia (reparado, remallado) lo que depende de él
  se rehace solo, como el resto de objetos.
- Alinear el escaneo por rasgos: plano detectado → piso (XY), eje de cilindro → Z o X, punto →
  origen; también por 3 puntos. Mueve el objeto con `apply_placement` (deshacer = inversa).
- Referencias tomadas de la malla, como operaciones del árbol ligadas al escaneo: punto (clic
  sobre la malla), plano por 3 puntos, plano ajustado a una zona, eje de un cilindro, centro de
  una esfera.
- Sketch anclado al escaneo: el corte del escaneo por el plano del sketch se ve como curva de
  referencia, con anclajes (cerca, extremos de tramos rectos, centros de arcos reconocidos) y
  «Usar» para copiarlo como entidades (como `Sketch.uses` con las aristas).

### E2 — El escaneo como cuerpo

- `FeatureKind::MeshBody { scan, max_faces, offset }`: simplificar con error medido (lo de
  `remesh::simplify`), cerrar huecos chicos (pinocchio-repair) y convertir a sólido (o
  superficie si sigue abierto). Motor según E0.
- Desfase (holgura) robusto por vóxeles (distancia con signo, `remesh::voxel_grid`):
  escaneo +0,2 mm para encajes, −x para interiores.
- Booleanas con las piezas: restar el escaneo de un bloque (encaje, negativo, cuna), unir,
  intersecar. Ejemplo de prueba: base para el gonfoterio = bloque − (escaneo + 0,3 mm).

### E3 — Extruir hasta el escaneo

- `Extent::UpToScan { offset }`: la extrusión llega a la piel del escaneo (con separación),
  aunque la piel sea curva. Pasante ∩ lado del cuerpo de E2.
- Lo mismo para mover cara y nervio.

### E4 — Formas que siguen la piel

- Región sobre la malla: pintar con pincel o proyectar un contorno cerrado de un sketch.
- Parche → engrosar hacia afuera o adentro con separación → cuerpo cerrado (férula, almohadilla,
  tapa que calza). Carcasa completa = desfase exterior − desfase interior.
- Molde: caja − escaneo desfasado, partido por plano o por la línea de mayor contorno, con
  llaves de registro (`print3d` ya tiene tarugos).

### E5 — Curvas sobre la piel

- Proyectar un sketch sobre la malla (polilínea 3D sobre la piel) y curva por puntos sobre la
  malla (geodésica).
- Usarlas como borde de región (E4) y como recorrido de barrido (canal, reborde, nervio que
  sigue la forma).

### E6 — Verificar el calce

- Holgura e interferencia firmadas pieza ↔ escaneo: rojo donde choca, verde dentro del rango
  pedido, azul donde sobra; mínimo/máximo y dónde.
- Corte vivo que muestra pieza y escaneo juntos.

## Fases — A (reconstruir la pieza)

### E7 — Más ajustes

- Cono, toro, redondeo entre dos planos (radio), extrusión libre (zona de sección constante →
  perfil + largo). Conos y toros también en `detect_all`.

### E8 — Revolución y recubrimiento desde cortes

- Corte por un plano que contiene el eje de un cilindro/cono → perfil → revolución (piezas
  torneadas, el espéculo).
- N cortes paralelos → recubrimiento.

### E9 — Asistente de reconstrucción

- `detect_all` propone un árbol (base, agujeros, tetones, redondeos) que se acepta por partes y
  queda editable; desviación al final.

### E10 — Superficies libres

- Ajustar una B-spline a una zona (`GeomAPI_PointsToBSplineSurface` / `GeomPlate`) con borde en
  las zonas vecinas, coser con las caras analíticas → sólido. La más difícil, al final.

## Riesgos

- Booleanas OCCT con miles de caras planas: lentas o fallan (se decide en E0).
- Escaneos abiertos: hay que cerrar antes de sólido; huecos grandes no se inventan.
- Desfase por vóxeles pierde detalle fino según la resolución: tamaño de vóxel ≤ holgura / 2.
