# Planes para llegar al nivel de Onshape

Índice de los planes del CAD paramétrico. Lo ya hecho está en [ROADMAP.md](ROADMAP.md)
(fases F0–F9, nombres robustos, parámetros, sketch con cotas al dibujar y selección tipo
Onshape). Cada plan dice qué hay hoy, qué falta y por qué, diseño, fases, pruebas y riesgos.

| # | Plan | Tamaño | Depende de |
|---|---|---|---|
| 1 | [Sketch: igualar la lista completa de un CAD](PLAN_SKETCH.md) (fases 1–4, 6–10 y 12 hechas; quedan 11 y 13–20) | L | — |
| 2 | [Edición con vista previa](PLAN_EDICION.md) | M | — |
| 3 | [Medir e inspeccionar](PLAN_INSPECCION.md) | S–M | — |
| 4 | [Recálculo incremental](PLAN_RECALCULO.md) | M | — (mejora 2) |
| 5 | [Varias piezas](PLAN_PIEZAS.md) | L | — |
| 6 | [Geometría de referencia y operaciones](PLAN_CONSTRUCCION.md) | L | 2 (diálogos), 5 (alcance) |
| 7 | [Planos 2D](PLAN_PLANOS_2D.md) | L | 5 para lista de piezas |
| 8 | [Ensambles](PLAN_ENSAMBLES.md) | XL | 5 |
| — | [Escaneo → CAD](PLAN_ESCANEO.md) | M | — (diferenciador) |
| — | [Más adelante](PLAN_FUTURO.md): chapa, superficies, versiones, configuraciones | XL c/u | varios |

Tamaños: S = días, M = una a dos semanas, L = varias semanas, XL = meses (con el ritmo de
trabajo de este proyecto).

## Orden propuesto

1. **Sketch completo** (empezando por anclajes): es donde más se nota la diferencia con
   Onshape en el uso diario, y el radio como variable del solver conviene hacerlo antes de que
   haya más código encima.
2. **Edición con vista previa**: cambia cómo se crean todas las operaciones; mejor tenerlo
   antes de agregar operaciones nuevas (plan 6) para no hacerlas dos veces.
3. **Medir e inspeccionar**: barato y se usa siempre; los vértices elegibles sirven también a
   anclajes y planos 2D.
4. **Recálculo incremental**: necesario cuando los diseños crezcan y para que la vista previa
   sea instantánea.
5. **Varias piezas**: cambio de fondo en `eval.rs`; habilita ensambles, planos con lista de
   piezas, material por pieza y exportar por pieza.
6. **Geometría de referencia y operaciones**: catálogo completo, ya con diálogos y piezas.
7. **Planos 2D**.
8. **Ensambles**.

**Escaneo → CAD** puede intercalarse en cualquier momento (no depende de lo demás); su primera
fase (mapa de desviación) es corta y muy útil para validar piezas escaneadas.

## Criterios comunes

- Cada fase cierra con pruebas en Rust (`cad-solver`, `cad-occt`, `cad-model`) y, si toca la
  interfaz, un escenario en `apps/web/e2e/cad.mjs`.
- Documentos viejos siguen abriendo igual (campos nuevos con `#[serde(default)]` y prueba de
  migración cuando cambie el significado de un dato).
- Toda función nueva del puente C tiene su versión en el stub sin OCCT y su prueba en
  `occt/tests/occt.rs`; si exige un toolkit nuevo, se mide el aumento del binario estático
  (`scripts/build-occt.sh`).
- Textos de la interfaz en español, Z arriba en lo que ve el usuario, confirmaciones con
  `ConfirmDialog`.
- Un commit por avance y una entrada en la bitácora de [ROADMAP.md](ROADMAP.md).
