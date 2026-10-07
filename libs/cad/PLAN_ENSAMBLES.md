# Ensambles

Objetivo: lo que Onshape llama *Assembly*: juntar piezas (del mismo diseño, de otros diseños
o importadas) como instancias, unirlas con relaciones (*mates*) que dejan los grados de libertad
que corresponden, moverlas para ver el mecanismo y detectar choques.

Depende de [PLAN_PIEZAS.md](PLAN_PIEZAS.md): sin piezas separadas no hay qué ensamblar.

## Qué tenemos hoy

- Un diseño produce un solo cuerpo; no hay piezas con identidad ni instancias.
- El esqueleto y la animación de la app (rig, poses, línea de tiempo) resuelven cadenas de
  huesos para mallas, no relaciones entre sólidos; algunas ideas se pueden reutilizar (línea de
  tiempo para animar, exportar animado a glTF).
- `cad-solver` resuelve sistemas 2D por Newton/LM; un solver 3D de ensambles es otro problema
  (transformaciones rígidas), aunque la técnica numérica es la misma.

## Diseño

### Datos

- `Assembly` como documento del proyecto:
  - `instances: Vec<Instance { id, fuente: PartSource, transform, fija: bool, nombre }>`;
    `PartSource = { diseño, PartRef } | Importada { STEP }` | `SubEnsamble`;
  - `mates: Vec<Mate { id, kind, a: MateConnector, b: MateConnector, límites? }>`.
- **Conector** (*mate connector*): un sistema de coordenadas sobre una instancia, definido por
  una referencia de la pieza (centro de una cara circular, vértice, centro de cara plana) +
  desplazamiento opcional. Se guarda con los orígenes de caras de la pieza, así sobrevive a
  cambios del diseño.
- Tipos de relación (como Onshape):

  | Relación | Grados libres |
  |---|---|
  | Fija (*fastened*) | 0 |
  | Revoluta | 1 giro |
  | Deslizante (*slider*) | 1 traslación |
  | Cilíndrica | giro + traslación sobre el mismo eje |
  | Plana | 2 traslaciones + 1 giro |
  | Pasador en ranura | traslación en ranura + giro |
  | Esférica | 3 giros |
  | Paralela, tangente | restricciones parciales |

  Más relaciones entre relaciones: engranaje (razón entre dos revolutas), cremallera-piñón,
  tornillo (giro ↔ avance).

### Solver de ensambles

- Incógnitas: posición y rotación de cada instancia no fija (6 por instancia; rotación como
  vector de giro incremental para evitar singularidades).
- Cada relación aporta ecuaciones entre los sistemas de los dos conectores (ejes iguales,
  orígenes coincidentes, etc.). Resolver con Levenberg-Marquardt como `cad-solver`, en un crate
  nuevo `cad-assembly` o como módulo 3D del solver.
- Arrastre: igual que `solve_drag` del sketch, mover una instancia con el resto siguiendo las
  relaciones.
- Diagnóstico de grados libres por espacio nulo (ya existe la técnica en el sketch).

### Interfaz

- Espacio "Ensamble" dentro de Diseñar: lista de instancias y relaciones, insertar pieza,
  arrastrar en el visor, crear relación eligiendo dos conectores (aparecen al pasar sobre caras
  circulares y planas, como en Onshape).
- **Animar relación**: recorrer el grado libre de una relación entre dos valores; exportar como
  glTF animado reutilizando la exportación de animación de la app.
- **Interferencias**: intersección booleana entre pares de instancias (`BodyOp::Intersect`
  sobre las formas transformadas), resaltar el volumen que choca.
- **Vista explosionada** (después): pasos de desplazamiento por instancia.
- **Lista de materiales**: piezas, cantidades, material y masa; base para el plano 2D.

### Exportación

- STEP de ensamble (estructura de producto con XCAF; las mismas bibliotecas que STEP con
  nombres de [PLAN_PIEZAS.md](PLAN_PIEZAS.md)).
- glTF/3MF con las piezas en su posición.

## Fases

1. **Instancias** de piezas del diseño, mover/girar a mano, fijar; visor con varias instancias.
2. **Conectores** y relaciones **fija, revoluta, deslizante, cilíndrica**; solver 3D. *Fases 1 y 2 hechas el 2026-10-07 (más plana), dentro del mismo documento
   (`Document.assembly`, sin crate ni carpeta nueva: módulo `cad_model::assembly`):
   instancias con posición y vector de giro, fija o libre; conectores (origen, Z, X en
   coordenadas de la pieza) tomados de una cara plana (centro y normal) o cilíndrica/cónica
   (punto del eje y el eje) con `cad_assembly_connector`; relaciones fija, bisagra,
   deslizante, cilíndrica y plana, con invertir, ángulo y distancia impuestos (para posar o
   animar). Solver Levenberg-Marquardt con jacobiano numérico (6 incógnitas por instancia
   libre; sin ninguna fija, la primera queda quieta) y grados libres por rango del
   jacobiano (SVD). Pruebas: bisagra deja 1, con ángulo 0; deslizante 1; fija 0; dos fijas
   contradictorias no convergen. Interfaz: pestaña Diseño | Ensamble, insertar piezas,
   editar posición y giro, relaciones con dos clics en el visor, estado de grados libres.
   El ensamble no entra en el hash del recálculo del diseño.*
3. **Arrastre** respetando relaciones y diagnóstico de grados libres.
4. **Interferencias** y **lista de materiales**. *Interferencias hechas el 2026-10-07 (cajas envolventes y
   luego intersección booleana; volumen común exacto). Falta la lista de materiales.*
5. **Animar** relación y exportar glTF animado.
6. Relaciones avanzadas (engranaje, tornillo, pasador en ranura), **sub-ensambles**, vista
   explosionada, STEP de ensamble.

## Pruebas

- Solver: bisagra (revoluta) deja exactamente 1 grado libre; dos fijas sobre-definen y se
  reporta conflicto; biela-manivela (4 barras) se mueve sin saltos al arrastrar.
- Asociatividad: cambiar el diámetro del agujero en el diseño no rompe el conector del eje.
- Interferencias: dos cajas solapadas 1 mm → volumen de choque exacto.

## Riesgos

- **Tamaño del trabajo** (XL): solver 3D, conectores, interfaz nueva. Empezar con pocas
  relaciones y un mecanismo de ejemplo.
- **Varios documentos** (diseño + ensamble) en un proyecto: hoy el proyecto guarda un solo
  documento CAD; hay que pasar a una lista con referencias entre ellos y recalcular en orden.
- **Rendimiento** de interferencias con muchas piezas: filtrar por cajas envolventes antes de
  la booleana.
