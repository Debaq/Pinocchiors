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

## Límites conocidos

- Las uniones solo van en caras de corte **planas** y la huella es un prisma
  (redondo, cuadrado o trapecio); no hay colas de milano deslizantes ni puzzles.
- El vaciado deja el hueco cerrado, sin agujero de drenaje (para resina hay que
  abrirlo a mano). Las partes más finas que el doble de la pared quedan macizas.
- El nesting usa el rectángulo en planta de cada pieza, no su contorno.
- Los voladizos del soporte son una cota superior (columnas hasta la cama).
- La verificación mide paredes con rayos desde una muestra de caras.

## Ideas siguientes

- Agujero de drenaje al vaciar; marcadores de pared fina en el visor
  (el backend ya devuelve `thin_points`).
- Orientación aplicada a cada pieza con el volumen de soporte real.
- Uniones sobre cortes no planos y Z-layers con pasadores alternados.
