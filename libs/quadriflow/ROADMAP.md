# QuadriFlow-RS Roadmap

Port de [QuadriFlow](https://github.com/hjwdzh/QuadriFlow) a Rust.

**Paper**: "QuadriFlow: A Scalable and Robust Method for Quadrangulation" (SGP 2018)

---

## Estructura de Módulos

```
libs/quadriflow/
├── field/          # Campos de orientación y posición (4-RoSy)
├── hierarchy/      # Jerarquía multi-escala de malla
├── optimizer/      # Optimización de campos (suavizado, alineación)
├── flow/           # Solver de flujo de red (min-cost flow)
├── parametrizer/   # Parametrización entera de superficie
├── extractor/      # Extracción de malla quad final
└── core/           # API principal
```

---

## Fases de Implementación

### Fase 1: Infraestructura Base ✅

- [x] Estructura de crates
- [x] Tipos básicos (OrientationField, PositionField, QuadMesh)
- [x] API pública (`remesh()`)
- [x] Configuración (RemeshConfig)
- [x] Integración con pinocchio-mesh

### Fase 2: Campo de Orientación (4-RoSy) ✅

El campo de orientación es un campo vectorial 4-RoSy (4-fold rotational symmetry) que define la dirección local de los quads.

#### 2.1 Inicialización del Campo ✅
- [x] Calcular normales por cara
- [x] Inicializar direcciones aleatorias en plano tangente
- [x] Representación: 1 vector por cara (las otras 3 direcciones son rotaciones 90°)

#### 2.2 Suavizado Local (Instant Meshes) ✅
- [x] Operador de suavizado local por cara
- [x] Promedio ponderado de vecinos respetando simetría 4-RoSy
- [x] Paralelización con rayon

#### 2.3 Detección de Singularidades ✅
- [x] Calcular índice de singularidad por vértice
- [x] Identificar vértices con valencia ≠ 4
- [x] Contar singularidades positivas/negativas

### Fase 3: Jerarquía Multi-escala ✅

Para acelerar la propagación del campo, se usa una jerarquía de mallas simplificadas.

#### 3.1 Construcción de Jerarquía ✅
- [x] Decimación progresiva de la malla
- [x] Mapeo bidireccional entre niveles (fino ↔ grueso)
- [x] Preservar correspondencia de vértices/caras

#### 3.2 Propagación Coarse-to-Fine ✅
- [x] Propagar campo de nivel grueso a fino (`propagate_field_to_finer`)
- [x] Propagar campo de nivel fino a grueso (`propagate_field_to_coarser`)
- [x] Transport de direcciones entre planos tangentes con 4-RoSy

### Fase 4: Minimización de Singularidades (Min-Cost Flow) ✅

El core de QuadriFlow: formular la ubicación de singularidades como un problema de flujo de red.

#### 4.1 Construcción del Grafo de Flujo ✅
- [x] Construir grafo dual de la malla (`FlowNetwork::from_mesh`)
- [x] Asignar capacidades y costos a aristas (proporcional a longitud)
- [x] Definir supply/demand en nodos (basado en índice de singularidad)

#### 4.2 Solver de Min-Cost Flow ✅
- [x] Implementar Successive Shortest Paths (`SuccessiveShortestPaths`)
- [x] Dijkstra con reduced costs y potenciales
- [x] NetworkSimplex como wrapper (delega a SSP)

#### 4.3 Aplicar Solución de Flujo ✅
- [x] `optimize_singularities()` - API principal
- [x] `compute_vertex_adjustments()` - interpretar flujo
- [x] `apply_singularity_optimization()` - modificar campo
- [x] `SingularityOptResult` con estadísticas

### Fase 5: Campo de Posición ✅

El campo de posición define dónde se ubicarán los vértices de los quads.

#### 5.1 Cálculo del Campo Continuo ✅
- [x] Resolver sistema de Poisson para coordenadas UV (`PositionField::from_orientation_field`)
- [x] Alinear gradiente UV con campo de orientación
- [x] Usar solver iterativo Gauss-Seidel (pinocchio-sparse)

#### 5.2 Transiciones Seamless ✅
- [x] Calcular rotaciones entre caras adyacentes (`SeamData::from_orientation_field`)
- [x] Asegurar consistencia global del campo (`SeamTransition::compose`, `verify_consistency`)
- [x] Manejar cortes (seams) en la parametrización (`SeamTransition::apply`, `apply_int`)

### Fase 6: Parametrización Entera ✅

Convertir coordenadas UV continuas a enteros para definir la topología de quads.

#### 6.1 Redondeo Greedy ✅
- [x] Redondear cada vértice al entero más cercano (`simple_rounding`, `greedy_rounding`)
- [x] Respetar restricciones de consistencia (`propagate_constraints`)

#### 6.2 Optimización Local ✅
- [x] Buscar mejores posiciones enteras localmente (`optimize_integers`)
- [x] Minimizar distorsión total (`compute_local_cost`, `IntegerStats`)

#### 6.3 SAT Solver (Opcional) ✅
- [x] Formular como problema SAT para eliminar T-junctions (`detect_tjunctions`, `remove_tjunctions`)
- [x] Solver greedy + constraint propagation (`greedy_remove_tjunctions`, `incremental_sat_solve`)
- [x] Flag `remove_tjunctions` en IntegerConfig

### Fase 7: Extracción de Malla Quad ✅

#### 7.1 Trazado de Isolíneas ✅
- [x] Para cada triángulo, encontrar cruces de isolíneas enteras (`find_grid_points_in_triangle`)
- [x] Conectar puntos de cruce para formar aristas de quads (`trace_isolines`)
- [x] Interpolar posiciones 3D en los cruces (barycentric interpolation)

#### 7.2 Construcción de Topología ✅
- [x] Ensamblar caras quad desde aristas (`deduplicate_quads`)
- [x] Manejar casos degenerados (quads colapsados) (`remove_degenerate_faces`)
- [x] Verificar orientación consistente (`validate_topology`)

#### 7.3 Post-procesado ✅
- [x] Merge de vértices cercanos (`merge_close_vertices` con spatial hashing)
- [x] Eliminación de caras degeneradas (`remove_degenerate_faces`)
- [x] Verificación de manifold (`is_manifold`, `TopologyInfo`)

### Fase 8: Optimizaciones ✅

#### 8.1 Paralelismo ✅
- [x] Paralelizar suavizado de campo (`smooth_orientation_field` con rayon)
- [x] Paralelizar trazado de isolíneas (`collect_grid_points_parallel`)
- [x] Evaluar granularidad óptima (`parallel_threshold` configurable)

#### 8.2 SIMD ✅
- [x] Operaciones batch con SoA layout (`VectorBatch`, `batch_*` functions)
- [x] Procesar múltiples caras simultáneamente (`smooth_faces_batch`, `BATCH_SIZE=4`)
- [x] SIMD autovectorization via loop unrolling

#### 8.3 Memoria ✅
- [x] Reducir allocaciones en hot paths (`VecPool`, `PooledVec`)
- [x] Pool de buffers reutilizables (`ScratchSpace`, thread-local pools)

### Fase 9: Features Adicionales ✅

#### 9.1 Preservación de Bordes Agudos ✅
- [x] Detectar aristas con ángulo diedro > umbral (`detect_sharp_edges`, `SharpEdgeConfig`)
- [x] Alinear campo de orientación a features (`align_to_features`, `align_to_4rosy`)
- [x] Flag `-sharp` equivalente (`preserve_sharp` en `OptimizerConfig`)

#### 9.2 Resolución Adaptiva ✅
- [x] Mayor densidad en zonas de alta curvatura (`compute_curvature_sizing`)
- [x] Menor densidad en zonas planas (sizing factor basado en variación de normales)
- [x] Basado en tensor de curvatura (aproximación por vecindad de normales)

#### 9.3 Soporte de Bordes Abiertos ✅
- [x] Manejar mallas con boundary (`detect_boundary`, `BoundaryInfo`)
- [x] Alinear quads a bordes (`align_to_boundary`)

---

## Dependencias Clave

| Crate | Uso |
|-------|-----|
| `pinocchio-mesh` | Estructura half-edge, I/O |
| `pinocchio-math` | Vectores, matrices |
| `pinocchio-sparse` | Matrices sparse, solver |
| `pinocchio-graph` | Grafos (para flow) |
| `nalgebra` | Álgebra lineal |
| `rayon` | Paralelismo |
| `petgraph` | Grafos (alternativa) |

---

## Referencias

1. [QuadriFlow Paper (PDF)](https://stanford.edu/~jingweih/papers/quadriflow/quadriflow.pdf)
2. [Instant Field-Aligned Meshes (base del campo)](https://igl.ethz.ch/projects/instant-meshes/)
3. [QuadriFlow GitHub (C++)](https://github.com/hjwdzh/QuadriFlow)
4. [libigl tutorials](https://libigl.github.io/) - referencia para geometría

---

## Métricas de Éxito

- [ ] Procesar malla 100K triángulos en <10 segundos
- [ ] Producir ~4x menos singularidades que Instant Meshes
- [ ] Output manifold y watertight
- [ ] Integración con UI existente
