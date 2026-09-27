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
| `isotropic.rs` | Remallado isótropo con rasgos (Botsch-Kobbelt): partir, colapsar, voltear y relajar; aristas vivas y bordes se conservan, sus vértices se deslizan sobre la línea y las esquinas quedan fijas |
| `sizing.rs` | Densidad adaptativa (opcional): grosor local por rayos hacia adentro y afuera desde muestras de la superficie, gradación geodésica y base calibrada para conservar la cantidad de quads |
| `symmetry.rs` | Simetría espejo: recorte de la mitad positiva, retopología de la mitad y reflexión soldando la costura |
| `surface.rs` | Soldado de costuras, subdivisión (arista más larga primero), normales, áreas, adyacencia, bordes y aristas vivas → restricciones |
| `hierarchy.rs` | Niveles por emparejamiento de vértices vecinos + coloreo para Gauss-Seidel paralelo |
| `field.rs` | Campos extrínsecos de orientación (4-RoSy) y posición (4-PoSy), de grueso a fino; guía de curvatura (tensor de forma suavizado, sin cruzar aristas vivas) en los niveles gruesos |
| `integer.rs` | Desplazamientos enteros por arista y eliminación de singularidades de posición (cargas unitarias por caminos mínimos en el grafo dual, sin invertir caras) |
| `extract.rs` | Fusión de vértices del mismo punto del retículo, medios quads emparejados por su diagonal (tomada del retículo de cada triángulo), relleno de agujeros, n-gonos → quads |
| `cleanup.rs` | Limpieza de la malla poligonal: colapso de aristas degeneradas, fusión de caras, *doublets* y colapso de diagonales mientras baje Σ(valencia−4)² + Σ(lados−4)², sin plegar caras ni cerrar esquinas bajo 30° |
| `features.rs` | Proyección de vértices sobre bordes y aristas vivas |
| `smooth.rs` | Optimización de la malla final: cada vértice va a la mejor de varias posiciones (promedio de vecinos, "paralelogramo" y, si tiene quads malos, búsqueda local) según Σ(1 − calidad)² con barrera antes de plegarse; los de arista viva se deslizan sobre ella y las esquinas quedan fijas |
| `quality.rs` | Informe de calidad público (`quality::analyze`): irregulares, plegados, deformes, estirados, desvío de ángulos y distancia a la original |
| `quad.rs` | `QuadMesh`, análisis topológico, separación de pellizcos, componentes |

## Garantías (tests en `core/tests/remesh.rs`)

- Salida solo de quads, orientada como la entrada, sin aristas ni vértices no-manifold.
- Superficies cerradas salen cerradas con la misma característica de Euler (esfera, toro, cubo).
- Número de caras dentro de ±15 % del objetivo (se corrige la escala si se
  desvía más de 8 %).
- Invariante a la escala (exacta para potencias de 2).
- Bordes abiertos: los vértices de borde quedan sobre el borde original.
- Con `preserve_sharp`, ningún quad cruza una arista viva.
- Determinista (mismo resultado con cualquier número de hilos).
- Toro (admite un campo sin singularidades): < 2 % de vértices irregulares.
- Esfera, toro y cilindro sin quads plegados.

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

| Modelo | Entrada | Quads (obj.) | Irregulares | Ángulos fuera de [60°,120°] | Desalineados >20° | Dist. máx (% diag.) |
|---|---|---|---|---|---|---|
| Esfera (8k tris) | limpia | 1076 (1000) | 1.3 % | 0.7 % | 0 % | 0.12 |
| Gonfoterio | 500k tris, limpia | 4732 (5000) | 3.1 % | 2.6 % | 7.0 %¹ | 1.10 |
| Conejo (STL) | 15 cáscaras, no-manifold | 3076 (3000) | 2.2 % | 4.2 % | —² | 0.55 |
| Oído interno | 9 piezas abiertas | 2856 (3000) | 5.3 % | 5.8 % | 15.5 % | 1.63 |
| Molde CAD `--sharp` | limpia | 2046 (2000) | 1.9 % | 3.1 % | 1.5 % | 1.37 |
| Tapa CAD `--sharp` | astillas de CAD | 1886 (2000) | 3.3 % | 4.5 % | 4.1 % | 3.03 |
| Part 2 CAD `--sharp` | astillas de CAD | 2028 (2000) | 3.3 % | 5.7 % | 7.1 % | 1.86 |
| Ender 3 angle `--sharp` | caras angostas | 2190 (2000) | 4.8 % | 4.7 % | 4.6 % | 1.40 |
| Audiómetro (STL) | no-manifold | 3004 (3000) | 6.4 % | 6.6 % | —² | 2.35 |

¹ Detalle de la piel más fino que un quad. ² Con reconstrucción la métrica
compara contra caras interiores de la entrada rota: no es válida.

Geometría (fase 4; calidad = jacobiano escalado mínimo del quad):

| Modelo | Plegados (antes → ahora) | Calidad < 0.5 | Aspecto > 5 | Dist. quads→original máx | original→quads máx |
|---|---|---|---|---|---|
| Esfera | 0 → 0 | 0 % | 0 | 0.12 % | 0.13 % |
| Gonfoterio | 5 → 3 | 0.72 → 0.38 % | 3 → 5 | 1.10 → 0.78 % | 6.2 % (punta delgada) |
| Conejo | 12 → 9 | 1.24 → 0.65 % | 17 → 9 | 0.50 % | 2.1 % |
| Molde | 16 → 11 | 2.10 → 1.32 % | 18 → 18 | 1.37 → 0.96 % | 1.3 % |
| Ender 3 angle | 9 → 1 | 1.96 → 0.59 % | 8 → 4 | 1.19 % | 1.7 % |
| Espéculo | 29 → 17 | 1.92 → 1.62 % | 45 → 27 | 0.33 % | 0.6 % |
| Part 1 | 28 → 28 | 3.29 → 2.81 % | 23 → 36 | 1.33 % | 2.6 % |
| Tapa | 20 → 14 | 2.51 → 3.08 % | 19 → 39 | 2.99 % | 1.2 % |

Los quads malos que quedan están casi todos en rasgos más angostos que un quad
(chaflanes, ranuras, paredes, puntas): ninguna posición de los vértices los
arregla a esa densidad; es trabajo de la fase 5.

Evolución (antes de la fase 1 → fase 3), irregulares / ángulos malos: esfera
1.9 → 1.3 % / 0.4 → 0.7 %; gonfoterio 4.5 → 3.1 % / 3.9 → 2.6 %; molde 4.6 →
1.9 % / 9.0 → 3.1 % (aspecto máx ~10¹² → 41); tapa 8.4 → 3.3 % / 21.5 → 4.5 %;
conejo 20 → 2.2 % / 31 → 4.2 %; audiómetro 24 → 6.4 % / 59 → 6.6 %. Gonfoterio
en ~11 s (antes ~16 s): el grafo isótropo es más chico que la entrada.

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
- [x] **Fase 3 — Aristas vivas y muestreo**: remallado isótropo con rasgos
      antes del campo (las astillas de CAD y los abanicos contaminaban el
      campo), proyección a aristas vivas sin encimar vértices fijos, guía de
      curvatura en niveles gruesos (toro: 16 → 0 singularidades de
      orientación) y corrección de la cantidad de quads (hasta dos
      extracciones más; queda la más cercana). Queda: recuperar aristas vivas
      tras la reconstrucción; algunos quads sueltos con aspecto > 50 en CAD;
      agujeros más chicos que un quad se tapan (fase 5).
- [x] **Fase 4 — Geometría**: optimización de la malla final con control de
      calidad (`smooth.rs`) en vez del Laplaciano con vértices clavados; los
      vértices de arista viva se deslizan, los fijos sin arista viva real se
      liberan. Formas suaves sin quads plegados. Queda: los quads malos en
      rasgos más angostos que un quad (fase 5).
- [x] **Fase 5 — Personajes y rasgos delgados**:
      - `RemeshConfig::symmetry` (X/Y/Z, plano por el centro de la caja): la
        mitad positiva se retopologiza y se refleja; resultado exactamente
        simétrico con un loop de aristas en la línea media, en la mitad de
        tiempo. Un rasgo más delgado que un quad justo sobre el plano (la
        cola del gonfoterio) puede cerrarse.
      - `RemeshConfig::adaptive_density` (desactivado por defecto): escala
        local ≤ grosor medido por rayos. Conserva rasgos delgados (aleta de
        0.4 con quads de ~1.8: error 5.8 % → 0 %; punta del gonfoterio 6.2 %
        → 1.6 %) pero cada transición de tamaño agrega vértices irregulares
        (gonfoterio 3.1 % → 5.5 %); con transiciones suaves el efecto sobre
        los rasgos casi desaparece. Por eso es opcional.
      - Queda: chaflanes angostos entre dos aristas vivas (el grosor no los
        detecta); transiciones de densidad más regulares.
- [x] **Fase 6 — App**: el panel de retopología expone simetría y, en
      "Avanzado", seguir la curvatura, densidad adaptativa, reparar malla rota
      e iteraciones; al terminar muestra el informe de calidad (resalta
      plegados, irregulares > 8 % y distancia > 2 %).
- [ ] **Fase 7 — Calidad avanzada** (investigada, NO resuelta; solo se
      adoptó el cambio de gradación. Los cuatro problemas siguen abiertos:
      ver "Pendiente"):
      - Transiciones de densidad: gradación 0.5 → 0.3. Con densidad
        adaptativa, gonfoterio: irregulares 6.0 → 5.1 %, ángulos malos 8.3 →
        5.0 %, cola conservada (original→quads 1.8 → 1.15 %); Ender: plegados
        34 → 23. Sigue siendo peor que sin adaptativa en CAD: queda opcional.
        Descartado: niveles diádicos de escala (el promedio 1.5× en la
        frontera no calza con ningún retículo; cientos de segundos) y dejar
        que el refinamiento sume quads al objetivo (sobrerrefina CAD).
      - Chaflanes: soltar aristas vivas a menos de 0.6 quads de otra sin
        extremo común. Banco a 3 densidades × 6 piezas CAD: irregulares 3.07
        → 2.93 % pero plegados 262 → 292. Descartado: los plegados no vienen
        de restricciones contradictorias sino de bandas de un solo quad de
        alto (canto de un disco) donde una dislocación no tiene lugar.
      - Pares 3-5 junto a singularidades de orientación: prohibir que las
        cargas de posición se absorban ahí no converge (quedan sin pareja):
        son estructurales en este esquema.
      - Aristas vivas tras la reconstrucción: llevar vértices del escalonado
        de vóxeles a las aristas vivas de la entrada crea dientes de sierra
        (audiómetro: plegados 33 → 155). Requiere otra extracción (dual
        contouring con QEF).

## Pendiente (fase 8 en adelante)

Los problemas abiertos tienen solución conocida; lo que cambia es el
esfuerzo. Los intentos acotados de la fase 7 fallaron, pero identificaron las
causas reales. Cada cambio en su commit, comparado antes/después con el banco
a varias densidades (una sola corrida por modelo es ruido) y con casos límite
con tiempo máximo (dos experimentos se colgaron cientos de segundos).

### Primero: probar personajes reales

El banco no tiene ningún personaje para rigging (humanoide con manos y dedos).
Antes de decidir los puntos 2A y 4, agregar 1–2 humanoides y medir: dedos
separados, loops en rodillas y codos, simetría en la línea media. Eso define
si la densidad adaptativa por niveles hace falta o basta con más quads.

### Propuestas

| # | Problema | Propuesta | ¿Vale la pena? | Esfuerzo | Riesgo |
|---|---|---|---|---|---|
| 1 | Quads plegados en bandas de un quad de alto (canto del disco de Part 1, paredes): una dislocación del retículo no tiene dónde acomodarse | En `integer.rs`, encarecer que las cargas de posición crucen zonas donde dos aristas vivas están a menos de dos quads, para que salgan por una zona ancha | **Sí**: causa la mayoría de los plegados en CAD | Medio | Bajo: cambio de costos en una etapa existente, fácil de revertir |
| 2A | Pares 3-5 sobrantes junto a singularidades de orientación (esfera: 14 irregulares, mínimo 8) | Simplificación posterior de la malla de quads con operadores que desplazan los pares hasta anularlos (Bozzo y Tarini, *Practical quad mesh simplification*) | **Probablemente**: loops más limpios, mejor deformación del rig | Medio | Medio: puede introducir pliegues o romper la variedad; empezar como opción |
| 2B | Ídem | Orientación y posición óptimas en forma global (Bommes et al., *Mixed-Integer Quadrangulation*); `pinocchio-sparse` ayuda | **No por ahora** | Muy grande | Alto: rendimiento del solver con 50k quads, redondeo |
| 3 | Aristas vivas redondeadas al reconstruir mallas rotas (audiómetro) | Extraer con *dual contouring* con QEF (Ju et al. 2002; variante manifold: Schaefer et al. 2007) en `rebuild.rs` | **Solo si importan las STL rotas de CAD** (impresión 3D); para personajes casi no aporta | Medio-grande | Medio: toca la reconstrucción, hoy robusta |
| 4 | La densidad adaptativa sube los irregulares: con escala continua los retículos vecinos nunca calzan | Tamaños por niveles (1×, ½×, ¼×): cada región con su tamaño y fronteras cosidas con plantillas fijas de transición 2:1 (como el mallado por octrees) | **Potencialmente mucho** para personajes (dedos, colas, orejas); lo decide la prueba con personajes | Grande | Alto: el mayor cambio de arquitectura |
| 5 | Chaflanes angostos que la densidad adaptativa no detecta | Medir el ancho de la cara entre dos aristas vivas y sumarlo al criterio de tamaño | Solo después del 4 | Chico | Bajo |

Orden recomendado: personajes → 1 → 2A → (4 según personajes) → (3 si la
impresión 3D de STL rotas es un caso de uso importante).

### Límites matemáticos (no son bugs)

- Una esfera necesita al menos 8 vértices irregulares (Poincaré-Hopf): solo
  se puede bajar de 14 a 8.
- Un rasgo más angosto que un quad no se representa sin quads más chicos ahí
  (agujeros chicos, colas finas, chaflanes a densidad uniforme); la salida es
  la densidad adaptativa.
- Toda transición de tamaño necesita vértices irregulares: se pueden ordenar
  (punto 4), no eliminar.
- Con simetría espejo, un tubo más fino que un quad justo sobre el plano puede
  perderse (la cola del gonfoterio); se mitiga activando además la densidad
  adaptativa.

### Riesgos generales

- Complejidad: el crate creció mucho en las fases 1–7; cada módulo nuevo es
  más código que mantener.
- Regresiones silenciosas: el pipeline es sensible; un cambio que mejora un
  modelo empeora otro.
- Pendiente de verificar a mano: el panel de retopología de la app (fase 6)
  no se probó en pantalla.
