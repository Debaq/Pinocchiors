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
| Escaneo como cuerpo para booleanas | ❌ motor decidido en E0: OCCT + vóxeles/isótropo + sólido directo (E2) |
| Elegir qué objeto es el escaneo | ✅ (E1.1) |
| Alinear el escaneo por rasgos | ✅ (E1.2) |
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

## Problema anotado: las mallas que vienen del CAD se ven y se trabajan mal (2026-10-10) ✅

El usuario: «nuestro gran problema». Un cubo hecho en Diseñar y pasado a Fabricar muestra
abollones en las caras donde se juntan los vértices de los triángulos; con retopología queda
redondeado (malo para piezas cuadradas, sobre todo de CAD a Fabricar); «reparar» al pasar a
malla queda terrible. El STL exportado directo desde el diseño se ve perfecto en OrcaSlicer,
pero **el mismo STL importado en Pinocchiors se ve y se trabaja mal**: el archivo está bien y el
problema es cómo lo toma la app.

Revisado el mismo día:
- ✅ **Abollones = solo sombreado.** La geometría no cambia (al soldar se juntan vértices de
  posición idéntica; Reparar en el STL del diseño solo quitó 2 caras inválidas, mismo
  volumen). Una sola normal por vértice en las aristas vivas, interpolada, hacía manchas en
  las caras planas. Ahora el backend marca por vértice si toca una arista viva (> 40° o de más
  de dos caras, `MeshData.creases`) y el shader de `lib/autoSmooth.ts` dibuja planos los
  triángulos que tocan una: caja plana, esfera y costados de cilindros suaves, escaneos con
  ruido sin facetas falsas. Sin duplicar vértices: pesos, pintura y skinning siguen por índice.
- ✅ **Retopología redondeada**: «Conservar aristas vivas» venía apagado. Con el STL del diseño
  (caja, cilindro, esfera, cono) a 2000 quads: apagado redondea caja y cilindro (máx. 1,13 %
  de la diagonal); encendido quedan vivas (0,49 %). Ahora viene encendido.
- ✅ Puente de pruebas: no tenía `get_scene_structure`, la grilla quedaba en metros y la
  cámara entraba en la pieza al pasar a Fabricar (solo en el arnés, no en la app).
- e2e `node e2e/cad.mjs "mallas:"`.

Si después de esto algo se sigue viendo o trabajando mal, conseguir el caso concreto (archivo
y pasos) antes de tocar Reparar.

Meta: un STL de CAD tiene que verse y medirse igual que en un laminador, y pasar de Diseñar a
Fabricar sin perder caras planas ni aristas vivas. Va antes de E1: B también parte de piezas
de CAD y de STL.

## Fases — B (diseñar sobre el escaneo)

### E0 — Banco sin escáner ✅ (2026-10-10)

- `cargo run --release -p cad-scan --example scanify -- entrada salida.(stl|obj) [opciones]`:
  remallado isótropo sin aristas vivas (vóxeles si la malla no es manifold), oclusión de plato
  giratorio (2 alturas × N giros, buffer de profundidad; no ve desde abajo de la mesa), huecos
  de borde irregular, ruido sobre la normal, ondulación suave y `--pose` (matriz en
  `salida.pose.txt`). Conejo: queda 54 % del área (es una placa: la cara de abajo no se ve);
  gonfoterio 92 %; pieza de Onshape 88 %. 3–11 s.
- Modelos: orgánicos para B (gonfoterio, conejo, oído interno), mecánicos para A (piezas de
  Onshape «Part Studio …», molde, espéculo, audiómetro).
- `cargo run --release -p cad-scan --example mesh_boolean -- malla N [--voxeles]`: malla a N
  triángulos → sólido OCCT → caja − malla, ∩, ∪; revisa corte + intersección = caja.
- **Hallazgos**:
  1. El cosido (`BRepBuilderAPI_Sewing`) tardaba 9–32 s con 20–50 k triángulos. Ahora
     `cad_from_mesh` arma el sólido directo compartiendo vértices y aristas si la malla es
     cerrada, manifold y orientada (20 k: 0,8 s; 50 k: 2 s) y solo cose si no.
  2. meshopt deja pares de caras espalda con espalda, aristas de más de dos caras y cruces:
     con eso OCCT no cierra el sólido o lo da inválido y la resta/unión fallan. Preparar por
     **vóxeles + isótropo** (cerrada, manifold, sin cruces) anduvo en los 4 modelos.
  3. Con malla buena las booleanas son exactas (corte + intersección = caja a 1e-6 %) y
     cuestan ~0,2 ms por cara: 5 k ≈ 1 s, 20 k ≈ 3–4 s, 50 k ≈ 8–11 s.
- **Decisión para E2**: OCCT (sin manifold). Escaneo → vóxeles (con el desfase de holgura ya
  incluido) → isótropo a ~20 k triángulos por defecto (hasta 50 k a pedido) → sólido directo.
  Cerrar los bordes abiertos que deje el isótropo antes de convertir: con 6 aristas abiertas
  cayó al cosido (89 s).

### E1 — El escaneo como referencia del diseño

Partes: E1.1 escaneo elegido ✅, E1.2 alinear por rasgos ✅, E1.3 referencias ligadas al
escaneo, E1.4 sketch anclado al corte.

- ✅ E1.1 (2026-10-10): el escaneo es un objeto importado elegido, no «el activo».
  `Document.scan_object` guarda la elección (si no hay: el importado activo o el primero con
  malla; selector «Escaneo» en la pestaña si hay más de uno). La app le avisa al backend con
  `cad_scan_use`; `with_scan` toma la escena del casillero del objeto (caché por huella de sus
  bytes) o la activa. Con una pieza del diseño activa, el escaneo se sigue viendo y midiendo.
  De paso: el STL importado calculaba las normales con la del archivo (`facet normal 0 0 0`
  dejaba la malla negra); ahora por el orden de los vértices, como los laminadores.
  e2e «escaneo: sigue de referencia con una pieza activa».
- ✅ E1.2 (2026-10-10): «Alinear el escaneo» en la pestaña: cara al piso (normal hacia −Z y
  en z = 0), cara al frente (gira en torno a Z hasta mirar a −Y), eje vertical (cilindro → Z,
  por el origen en planta) y punto al origen. `lib/scanAlign.ts` arma la matriz en el CAD y
  la pasa al modelo (`toSceneMatrix`); App activa el escaneo y la aplica como una orientación
  más (historial y deshacer, mueve quads/esqueleto/rig; queda como modificación del objeto en
  el Outliner). Pruebas node `e2e/scanAlign.test.mjs` y e2e «escaneo: alinear…». Pendiente:
  alinear por 3 puntos y eje → X.
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

- Booleanas OCCT: ~0,2 ms por cara; más de 50 k triángulos se vuelve lento (E0).
- Escaneos abiertos: hay que cerrar antes de sólido; huecos grandes no se inventan.
- Desfase por vóxeles pierde detalle fino según la resolución: tamaño de vóxel ≤ holgura / 2.
