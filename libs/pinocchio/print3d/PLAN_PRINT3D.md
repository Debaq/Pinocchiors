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
- Cierre de cuarto de vuelta (`PlanarShape::Bayonet`): perno con cabeza de barra que entra por una ranura y se traba al girar 90°.

## Límites conocidos

- Las uniones solo van en caras de corte **planas** y dentro de la cara (redonda, cuadrada, trapecio
  o cuarto de vuelta). **No hay colas de milano deslizantes ni puzzles**: el riel tiene que salir por la
  pared de la pieza, y eso pide recortar una superficie cualquiera (hoy solo se recorta la tapa plana).
- El drenaje pide una cara plana abajo (al exportar, tras apoyar la pieza) y un solo agujero; sin venteo.
- La verificación mide paredes con rayos desde una muestra de caras.

## Ideas siguientes

- Venteo (segundo agujero) y drenaje sin cara plana.
- Orientación por pieza visible antes de exportar.
- Colas de milano deslizantes y puzzles (recorte de la pared de la pieza).
