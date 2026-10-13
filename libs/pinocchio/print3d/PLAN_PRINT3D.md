# Plan: preparación para impresión 3D

Solo preparar el modelo (nada de gcode ni slicer). La lógica vive en
`pinocchio-print3d` con Z arriba y milímetros; el escritorio
(`apps/desktop/src/print_prep.rs`) traduce desde la escena (Y arriba).

| Fase | Contenido | Módulo | Estado |
|------|-----------|--------|--------|
| 1 | Verificación: estanca, manifold, normales, degenerados, cuerpos, voladizos, paredes finas | `printability` | hecha |
| 2 | Orientación: elegir la cara de apoyo que minimiza voladizos | `orientation` | hecha (se aplica con el piso de Preparar) |
| 3 | Perfiles de impresora y material; aviso de que no cabe | `profile` | hecha |
| 4 | Uniones reales: pernos y huecos en las caras de corte planas | `planar_joints`, `contact`, `capping` | hecha |
| 5 | Distribución en la cama (nesting) | `nesting` | hecha |
| 6 | Vaciado (cáscara por vóxeles) y estimación de material | `hollow` | hecha |
| 7 | Exportar placas STL/3MF/OBJ en mm | `print_prep` (escritorio) | hecha |

Además se rehízo el cierre de cortes (`capping`): antes solo cerraba un contorno
convexo y salían piezas abiertas; ahora admite varios contornos, huecos y
polígonos cóncavos. `find_neighbors` y `calculate_connections` miden contactos
planos reales entre piezas.

## Después de las 7 fases

- Marcas de pared fina en el visor; las uniones se rehacen cuando el diseño cambia.
- Impresora, material, uniones y placas se guardan en el proyecto; impresoras propias se recuerdan entre sesiones.
- Soporte estimado con rayos hacia la cama (las orientaciones se reordenan por él).
- Vista previa de la distribución en la cama.
- Agujero de drenaje al vaciar (túnel desde la cara plana de abajo hasta el hueco).
- Distribución por contorno en planta (`nest_contour`, por celdas).
- Cola de milano deslizante (`PlanarShape::Rail`, `groove::subtract_trench`): riel en una pieza y ranura en la otra, de pared a pared; la ranura se resta de cualquier pared (también curva) con un recorte de triángulos y cierre de las caras de la zanja.
- Cierre de cuarto de vuelta (`PlanarShape::Bayonet`): perno con cabeza de barra que entra por una ranura y se traba al girar 90°.

## Límites conocidos

- Las uniones solo van en caras de corte **planas** y dentro de la cara (redonda, cuadrada, trapecio
  cuarto de vuelta) y el riel deslizante, que corre una línea recta de pared a pared (la cuerda tiene que
  caber en la cara sin huecos). No hay puzzles. El riel no tiene tope: se desliza por los dos extremos.
- El drenaje pide una cara plana abajo (al exportar, tras apoyar la pieza) y un solo agujero; sin venteo.
- La verificación mide paredes con rayos desde una muestra de caras.

## Ideas siguientes

- Venteo (segundo agujero) y drenaje sin cara plana.
- Orientación por pieza visible antes de exportar.
- Tope en la ranura (un extremo ciego) y puzzles.

## Tolerancias de fabricación y su conexión con Diseñar (CAD)

Hecho (lado impresión): `FabTolerances::derive(impresora, material)` en `fab.rs` es la única fuente de
holguras y compensaciones: holgura de base (parte de la boquilla × factor del material), `Fit`
(justo / normal / libre), compensación de agujeros por lado, contracción por eje y pared mínima.
Las uniones toman su holgura de ahí (selector «Ajuste») y las placas se agrandan por la contracción
al exportar (el diseño queda en medidas nominales). Comando `get_fab_tolerances`.

Por hacer (lado CAD; se hace cuando el trabajo de engranes esté commiteado):

1. **Perfil de fabricación en el documento de CAD** (`Document.fabrication`: impresora, material, ajuste
   por defecto), guardado en el `.pinocchio` y compartido con Fabricar: elegirlo en un lado lo elige en el otro.
2. **Operaciones que lo leen**: la holgura de impresión de `GearPart` (`print_clearance`) parte de
   `fit_clearance(Fit)`; agujeros, chaveteros y roscas ofrecen «Ajustar a la impresora» (`compensated_hole`);
   `Shell`/paredes avisan al bajar de `min_wall`.
3. **Ensambles**: `interferences` pasa a medir también la holgura (pares más cerca que `fit_clearance`
   se marcan, no solo los que chocan) y sugiere el ajuste entre piezas conectadas.
4. **Exportar desde Diseñar** con la misma compensación de contracción que las placas.
5. La compensación de **agujeros** al exportar solo es posible en el CAD (cara cilíndrica conocida); en una
   malla suelta no se detectan agujeros, por eso allí solo se aplica la contracción.
