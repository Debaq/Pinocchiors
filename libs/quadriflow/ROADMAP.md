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
| `surface.rs` | Soldado de costuras, subdivisión (arista más larga primero), normales, áreas, adyacencia, bordes y aristas vivas → restricciones |
| `hierarchy.rs` | Niveles por emparejamiento de vértices vecinos + coloreo para Gauss-Seidel paralelo |
| `field.rs` | Campos extrínsecos de orientación (4-RoSy) y posición (4-PoSy), de grueso a fino |
| `extract.rs` | Fusión de vértices del mismo punto del retículo, trazado de caras por órbitas, relleno de agujeros, n-gonos → quads |
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

Rendimiento: ~500k triángulos → 5000 quads en ~7 s (16 hilos).

## Pendiente

- [ ] Densidad adaptativa (quads más chicos donde hay más curvatura).
- [ ] Optimización de singularidades por flujo de costo mínimo (la contribución
      propia de QuadriFlow sobre Instant Meshes).
- [ ] Estructuras más finas que un quad (tubos delgados) pueden cerrarse o
      perder asas; subir el número de quads lo mitiga.
