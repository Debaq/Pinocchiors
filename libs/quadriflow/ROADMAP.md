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
| `surface.rs` | Soldado de costuras, subdivisión (arista más larga primero), normales, áreas, adyacencia, bordes y aristas vivas → restricciones |
| `hierarchy.rs` | Niveles por emparejamiento de vértices vecinos + coloreo para Gauss-Seidel paralelo |
| `field.rs` | Campos extrínsecos de orientación (4-RoSy) y posición (4-PoSy), de grueso a fino |
| `integer.rs` | Desplazamientos enteros por arista y eliminación de singularidades de posición (cargas unitarias por caminos mínimos en el grafo dual, sin invertir caras) |
| `extract.rs` | Fusión de vértices del mismo punto del retículo, medios quads emparejados por su diagonal (tomada del retículo de cada triángulo), relleno de agujeros, n-gonos → quads |
| `cleanup.rs` | Limpieza de la malla poligonal: colapso de aristas degeneradas, fusión de caras, *doublets* y colapso de diagonales mientras baje Σ(valencia−4)² + Σ(lados−4)², sin plegar caras ni cerrar esquinas bajo 30° |
| `features.rs` | Proyección de vértices sobre bordes y aristas vivas |
| `quad.rs` | `QuadMesh`, análisis topológico, separación de pellizcos, componentes |

## Garantías (tests en `core/tests/remesh.rs`)

- Salida solo de quads, orientada como la entrada, sin aristas ni vértices no-manifold.
- Superficies cerradas salen cerradas con la misma característica de Euler (esfera, toro, cubo).
- Número de caras dentro de ±15 % del objetivo.
- Invariante a la escala (exacta para potencias de 2).
- Bordes abiertos: los vértices de borde quedan sobre el borde original.
- Con `preserve_sharp`, ningún quad cruza una arista viva.
- Determinista (mismo resultado con cualquier número de hilos).
- Toro (admite un campo sin singularidades): < 2 % de vértices irregulares.

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

| Modelo | Entrada | Quads (obj.) | Irregulares | Ángulos fuera de [60°,120°] | Aspecto máx | Dist. máx (% diag.) |
|---|---|---|---|---|---|---|
| Esfera (8k tris) | limpia | 1072 (1000) | 10 (mín. 8) | 0.4 % | 2.2 | 0.12 |
| Gonfoterio | 500k tris, limpia | 4774 (5000) | 4.0 % | 2.5 % | 10.1 | 1.18 |
| Conejo (STL) | 15 cáscaras, 1062 aristas no-manifold | 3180 (3000) | 3.8 % | 6.9 % | 24.7 | 0.61 |
| Oído interno | 9 piezas abiertas | 2358 (3000) | 5.5 % | 6.7 % | 81.6 | 1.77 |
| Molde CAD `--sharp` | limpia | 1930 (2000) | 4.0 % | 7.6 % | 33.5 | 1.35 |
| Audiómetro (STL) | 1989 aristas no-manifold | 2102 (3000) | 10.8 % | 9.8 % | 26.9 | 3.01 |
| Audiómetro, 10k | ídem | 9106 (10000) | 4.4 % | 3.6 % | 14.6 | 1.71 |

Evolución de los irregulares (antes de la fase 1 → fase 1 → fase 2): esfera
20 → 20 → 10; gonfoterio 227 → 227 → 190; molde 92 → 92 → 78 (aspecto máx
~10¹² → 33.5); conejo 472 → 126 → 121; oído 153 → 153 → 127.

Sin reconstrucción el conejo y el audiómetro salían rotos (20 % y 24 %
irregulares, 31 % y 59 % de ángulos malos, quads en astillas y aletas).

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
- [ ] **Fase 3 — Aristas vivas**: esquinas fijas, aristas de quads alineadas a
      las curvas vivas (aletas en las esquinas del molde) y aristas vivas
      recuperadas tras la reconstrucción.
- [ ] **Fase 4 — Geometría**: relajación que respete rasgos, sin quads
      doblados; cantidad de quads ±10 %; distancia máx < 0.5 % de la diagonal.
- [ ] **Fase 5 — Personajes**: simetría espejo, densidad adaptativa,
      estructuras delgadas (colmillos, paredes de carcasas).
- [ ] **Fase 6 — App**: exponer reconstrucción, simetría y densidad; métricas.
