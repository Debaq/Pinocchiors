# Retopología (quadriflow-core)

Convierte una malla de triángulos en una malla de quads alineada a la forma.
Implementa *Instant Field-Aligned Meshes* (Jakob, Tarini, Panozzo,
Sorkine-Hornung, SIGGRAPH Asia 2015), la base de QuadriFlow.

> El port anterior de QuadriFlow (crates `field`, `hierarchy`, `optimizer`,
> `flow`, `parametrizer`, `extractor`) se reemplazó en 2026-09: su campo de
> posición no respetaba la simetría 4-RoSy y la escala de la grilla estaba
> invertida, así que devolvía 0 quads en una esfera de radio 1 y cientos de
> miles con radio 100.

## Módulos (`core/src`)

| Módulo | Qué hace |
|---|---|
| `rebuild.rs` | Reconstrucción de entradas rotas: número de vueltas rápido (dipolos, Barill 2018) en una grilla, distancias exactas en los cruces, *marching tetrahedra* y colapso de astillas |
| `isotropic.rs` | Remallado isótropo con rasgos (Botsch-Kobbelt): partir, colapsar, voltear y relajar; aristas vivas y bordes se conservan, sus vértices se deslizan sobre la línea y las esquinas quedan fijas |
| `surface.rs` | Soldado de costuras, subdivisión (arista más larga primero), normales, áreas, adyacencia, bordes y aristas vivas → restricciones |
| `hierarchy.rs` | Niveles por emparejamiento de vértices vecinos + coloreo para Gauss-Seidel paralelo |
| `field.rs` | Campos extrínsecos de orientación (4-RoSy) y posición (4-PoSy), de grueso a fino; guía de curvatura (tensor de forma suavizado, sin cruzar aristas vivas) en los niveles gruesos |
| `integer.rs` | Desplazamientos enteros por arista y eliminación de singularidades de posición (cargas unitarias por caminos mínimos en el grafo dual, sin invertir caras) |
| `extract.rs` | Fusión de vértices del mismo punto del retículo, medios quads emparejados por su diagonal (tomada del retículo de cada triángulo), relleno de agujeros, n-gonos → quads |
| `cleanup.rs` | Limpieza de la malla poligonal: colapso de aristas degeneradas, fusión de caras, *doublets* y colapso de diagonales mientras baje Σ(valencia−4)² + Σ(lados−4)², sin plegar caras ni cerrar esquinas bajo 30° |
| `features.rs` | Proyección de vértices sobre bordes y aristas vivas |
| `smooth.rs` | Optimización de la malla final: cada vértice va a la mejor de varias posiciones (promedio de vecinos, "paralelogramo" y, si tiene quads malos, búsqueda local) según Σ(1 − calidad)² con barrera antes de plegarse; los de arista viva se deslizan sobre ella y las esquinas quedan fijas |
| `quad.rs` | `QuadMesh`, análisis topológico, separación de pellizcos, componentes |

## Garantías (tests en `core/tests/remesh.rs`)

- Salida solo de quads, orientada como la entrada, sin aristas ni vértices no-manifold.
- Superficies cerradas salen cerradas con la misma característica de Euler (esfera, toro, cubo).
- Número de caras dentro de ±15 % del objetivo (se corrige la escala si se
  desvía más de 8 %).
- Invariante a la escala (exacta para potencias de 2).
- Bordes abiertos: los vértices de borde quedan sobre el borde original.
- Con `preserve_sharp`, ningún quad cruza una arista viva.
- Determinista (mismo resultado con cualquier número de hilos).
- Toro (admite un campo sin singularidades): < 2 % de vértices irregulares.
- Esfera, toro y cilindro sin quads plegados.

Singularidades de posición eliminadas (vértices interiores con valencia ≠ 4):

| Modelo | Antes | Después | Mínimo teórico |
|---|---|---|---|
| Toro, 1000 quads | 66 | 3 | 0 |
| Esfera, 1000 quads | 34 | 20 | 8 |
| Gonfoterio (500k tris), 5000 quads | 337 | 227 | — |

Rendimiento: ~500k triángulos → 5000 quads en ~7 s (16 hilos).

## Entradas rotas (`RemeshConfig::rebuild`)

Con `Rebuild::Auto` (por defecto) la superficie se reconstruye si tiene aristas
no-manifold, orientación incoherente o cáscaras cerradas metidas unas en otras;
se retopologiza la superficie exterior de la unión. Las superficies abiertas
con cáscaras superpuestas no se reconstruyen (se taparían sus agujeros).
Vóxel = ⅓ del lado de un quad; grilla de 48 a 320 vóxeles en el eje mayor.
Sobre una superficie reconstruida `preserve_sharp` no tiene efecto: sus
esquinas quedan redondeadas a escala de vóxel.

## Banco de modelos reales

`cargo run --release -p quadriflow-core --example quality -- <modelo> <quads> [--sharp] [--rebuild always|never] [--obj salida.obj]`

| Modelo | Entrada | Quads (obj.) | Irregulares | Ángulos fuera de [60°,120°] | Desalineados >20° | Dist. máx (% diag.) |
|---|---|---|---|---|---|---|
| Esfera (8k tris) | limpia | 1076 (1000) | 1.3 % | 0.7 % | 0 % | 0.12 |
| Gonfoterio | 500k tris, limpia | 4732 (5000) | 3.1 % | 2.6 % | 7.0 %¹ | 1.10 |
| Conejo (STL) | 15 cáscaras, no-manifold | 3076 (3000) | 2.2 % | 4.2 % | —² | 0.55 |
| Oído interno | 9 piezas abiertas | 2856 (3000) | 5.3 % | 5.8 % | 15.5 % | 1.63 |
| Molde CAD `--sharp` | limpia | 2046 (2000) | 1.9 % | 3.1 % | 1.5 % | 1.37 |
| Tapa CAD `--sharp` | astillas de CAD | 1886 (2000) | 3.3 % | 4.5 % | 4.1 % | 3.03 |
| Part 2 CAD `--sharp` | astillas de CAD | 2028 (2000) | 3.3 % | 5.7 % | 7.1 % | 1.86 |
| Ender 3 angle `--sharp` | caras angostas | 2190 (2000) | 4.8 % | 4.7 % | 4.6 % | 1.40 |
| Audiómetro (STL) | no-manifold | 3004 (3000) | 6.4 % | 6.6 % | —² | 2.35 |

¹ Detalle de la piel más fino que un quad. ² Con reconstrucción la métrica
compara contra caras interiores de la entrada rota: no es válida.

Geometría (fase 4; calidad = jacobiano escalado mínimo del quad):

| Modelo | Plegados (antes → ahora) | Calidad < 0.5 | Aspecto > 5 | Dist. quads→original máx | original→quads máx |
|---|---|---|---|---|---|
| Esfera | 0 → 0 | 0 % | 0 | 0.12 % | 0.13 % |
| Gonfoterio | 5 → 3 | 0.72 → 0.38 % | 3 → 5 | 1.10 → 0.78 % | 6.2 % (punta delgada) |
| Conejo | 12 → 9 | 1.24 → 0.65 % | 17 → 9 | 0.50 % | 2.1 % |
| Molde | 16 → 11 | 2.10 → 1.32 % | 18 → 18 | 1.37 → 0.96 % | 1.3 % |
| Ender 3 angle | 9 → 1 | 1.96 → 0.59 % | 8 → 4 | 1.19 % | 1.7 % |
| Espéculo | 29 → 17 | 1.92 → 1.62 % | 45 → 27 | 0.33 % | 0.6 % |
| Part 1 | 28 → 28 | 3.29 → 2.81 % | 23 → 36 | 1.33 % | 2.6 % |
| Tapa | 20 → 14 | 2.51 → 3.08 % | 19 → 39 | 2.99 % | 1.2 % |

Los quads malos que quedan están casi todos en rasgos más angostos que un quad
(chaflanes, ranuras, paredes, puntas): ninguna posición de los vértices los
arregla a esa densidad; es trabajo de la fase 5.

Evolución (antes de la fase 1 → fase 3), irregulares / ángulos malos: esfera
1.9 → 1.3 % / 0.4 → 0.7 %; gonfoterio 4.5 → 3.1 % / 3.9 → 2.6 %; molde 4.6 →
1.9 % / 9.0 → 3.1 % (aspecto máx ~10¹² → 41); tapa 8.4 → 3.3 % / 21.5 → 4.5 %;
conejo 20 → 2.2 % / 31 → 4.2 %; audiómetro 24 → 6.4 % / 59 → 6.6 %. Gonfoterio
en ~11 s (antes ~16 s): el grafo isótropo es más chico que la entrada.

## Pendiente

Plan hacia una retopología lista para producción (una fase por commit):

- [x] Eliminación de singularidades de posición (QuadriFlow): camino mínimo por
      carga unitaria en vez de un flujo global.
- [x] **Fase 1 — Entradas rotas**: reconstrucción volumétrica automática.
- [x] **Fase 2 — Limpieza topológica**: diagonal de cada medio quad tomada de
      su propio retículo (el mapa global por par de vértices daba medios quads
      con dos "diagonales"), limpieza de la malla poligonal (`cleanup.rs`).
      Queda: pares 3-5 junto a singularidades de orientación que absorbieron
      carga de posición (esfera gruesa: 14 irregulares contra 8); son
      dislocaciones que ninguna operación local elimina.
- [x] **Fase 3 — Aristas vivas y muestreo**: remallado isótropo con rasgos
      antes del campo (las astillas de CAD y los abanicos contaminaban el
      campo), proyección a aristas vivas sin encimar vértices fijos, guía de
      curvatura en niveles gruesos (toro: 16 → 0 singularidades de
      orientación) y corrección de la cantidad de quads (hasta dos
      extracciones más; queda la más cercana). Queda: recuperar aristas vivas
      tras la reconstrucción; algunos quads sueltos con aspecto > 50 en CAD;
      agujeros más chicos que un quad se tapan (fase 5).
- [x] **Fase 4 — Geometría**: optimización de la malla final con control de
      calidad (`smooth.rs`) en vez del Laplaciano con vértices clavados; los
      vértices de arista viva se deslizan, los fijos sin arista viva real se
      liberan. Formas suaves sin quads plegados. Queda: los quads malos en
      rasgos más angostos que un quad (fase 5).
- [ ] **Fase 5 — Personajes y rasgos delgados**: densidad adaptativa (quads
      más chicos donde un rasgo es más angosto que un quad: chaflanes,
      ranuras, puntas, paredes), simetría espejo.
- [ ] **Fase 6 — App**: exponer reconstrucción, simetría y densidad; métricas.
