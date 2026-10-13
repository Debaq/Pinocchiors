# Plan: preparación para impresión 3D

Solo preparar el modelo (nada de gcode ni slicer). Todo en `pinocchio-print3d`
con Z arriba (la plataforma); el escritorio gira la malla al entrar y salir.

| Fase | Contenido | Módulo |
|------|-----------|--------|
| 1 | Verificación de imprimibilidad: cerrada, manifold, aristas abiertas, caras degeneradas, componentes, voladizos, paredes finas | `printability` |
| 2 | Orientación: elegir la cara de apoyo que minimiza voladizos y asentar en la cama | `orientation` |
| 3 | Perfiles de impresora y material (cama, nozzle, densidad, ángulo de voladizo); aviso de que no cabe | `profile` |
| 4 | Uniones reales: pernos y huecos en caras de corte planas, con tolerancia FDM | `planar_joints` |
| 5 | Distribución en la cama (nesting) de varias piezas | `nesting` |
| 6 | Vaciado (cáscara) y estimación de material | `hollow` (escritorio, vóxeles) |
| 7 | Exportar: STL/3MF por pieza o placa completa, en mm | escritorio |

Cada fase: lógica con pruebas, comandos del escritorio y sección en el panel.
