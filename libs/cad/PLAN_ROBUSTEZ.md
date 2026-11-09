# Plan: CAD más robusto que los comerciales

2026-11-08. Meta del usuario: «el horizonte es sobrepasar a los privados»
(Parasolid: SolidWorks, NX, Onshape, Fusion; ACIS). OCCT es el mejor motor
abierto, pero redondeos, vaciados y booleanas difíciles fallan más que en
Parasolid. Base: OCCT 8.0.1 (PLAN_OCCT8.md), ya más rápida y sin la caída del
vaciado.

Lo que tenemos y ellos no: el código de OCCT abierto (vemos por qué falla), el
reintento en cascada (varias formas de la misma operación, en paralelo), la IA
en vivo, escaneo → CAD y diagnósticos que dicen qué hacer.

Regla: cada fase se mide contra el banco (R1); el número de casos resueltos
solo puede subir.

## R1 — Banco de casos difíciles

- ~100 piezas donde OCCT suele fallar: redondeos que se cruzan o llegan a
  vértices de 3+ aristas, redondeo después de chaflán, radios casi del espesor;
  vaciados con caras curvas, con redondeos, espesor casi del radio; booleanas
  casi tangentes, caras coincidentes, aristas sobre caras, patrones grandes.
- Para cada caso: termina / falla limpio / se cae, tiempo, sólido válido.
- `cargo run --release -p cad-occt --example banco_dificil` (como bench_occt).
- Resultado base con 7.9.3 y 8.0.1 en este plan.

## R2 — Redondeos en cascada

Si `BRepFilletAPI_MakeFillet` con todas las aristas falla:
1. Por cadenas tangentes, de a una (orden: las largas primero, después las cortas).
2. Con la pieza sanada antes (`ShapeFix`, `ShapeUpgrade_UnifySameDomain`).
3. Con otros parámetros (`SetParams`, tolerancias).
4. Sin salida: decir qué arista falla y el radio más grande que sí entra
   (búsqueda binaria), para que la UI y la IA lo propongan.

## R3 — Vaciados en cascada

1. `BRepOffsetAPI_MakeThickSolid` con unión `GeomAbs_Intersection` y después `GeomAbs_Arc`.
2. Pieza sanada y caras unidas antes.
3. Propio: desfasar cara por cara (`BRepOffset`), recortar y coser.
4. Sin salida: qué cara impide el espesor y el espesor máximo que entra.

## R4 — Booleanas robustas

- Valor difuso (`SetFuzzyValue`) para caras casi coincidentes, `SetGlue` cuando
  se tocan, `SetNonDestructive`, unir caras partidas después
  (`UnifySameDomain`) y validar siempre (`BRepCheck`), reintentando si sale
  inválida.

## R5 — Comparar con Parasolid

- Las mismas piezas del banco en Onshape (gratis, Parasolid): dónde gana cada
  uno. Anotarlo acá y atacar lo que falte.

## R6 — Lo que ellos no tienen

- Diagnóstico con arreglo propuesto en cada falla (UI y conector IA).
- Operaciones en paralelo (la 8 lo permite: planos 2D, piezas, redondeos
  independientes).

## Avance

- 2026-11-08: plan escrito.
