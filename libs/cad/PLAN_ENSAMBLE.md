# Ensamble: lo que falta para que sea de verdad útil

Plan desde el 2026-11-09, a partir de la auditoría del ensamble (pedido del usuario: «audita el
sistema de ensamble del CAD para ver qué nos falta», y luego «sí, hazlo»).

Leyenda: ✅ hecho · 🟡 a medias · ❌ falta.

## Dónde estamos

- Modelo: `libs/cad/model/src/assembly.rs` (`Assembly`, `Instance`, `Connector`, `Mate`,
  `Relation`, `solve` por Levenberg-Marquardt con jacobiano numérico, `sweep`, choques, holguras,
  engranes). Pestañas y subensambles: `libs/cad/model/src/tabs.rs` (`flat_assembly`,
  `subassembly_part`, variantes por configuración).
- App: `apps/desktop/src/cad.rs` (`cad_assembly_mesh/faces/connector/interference/clearance/
  suggest/gear_warnings`, `cad_export_assembly_animation`, exportar con `assembly`).
- Interfaz: `AssemblyPanel` en `apps/web/src/components/steps/DesignStep.tsx`, arrastre en
  `apps/web/src/lib/assemblyDrag.ts` + `startPartDrag` en `CadView.tsx`, store en `lib/cad.ts`.
- Pruebas: `libs/cad/model/tests/assembly.rs` (6) y `assembly_relations.rs` (19).

Ya hay: instancias (de otras pestañas, en otra configuración, subensambles rígidos), 8 uniones
(pegada, bisagra, deslizante, cilíndrica, plana, rótula, paralela, pasador en ranura),
acoplamientos (engrane con planetarios, piñón-cremallera, tornillo; detección de engranes
enlazados), grados libres, arrastre por bisagra/deslizante, choques y holguras a pedido,
explosionada radial en pantalla, lista de materiales, plano 2D con globos, GLB animado, exportar.

## Fases (en el orden en que conviene hacerlas)

### EN0 — Limpieza chica ❌
- Al quitar una instancia o una unión, quitar también los acoplamientos (`relations`) que
  apuntan a uniones que ya no existen (hoy quedan colgando en la lista).
- Prueba en Rust: borrar unión → `relations` sin referencias muertas (helper `Assembly::prune`).

### EN1 — Uniones ligadas a la pieza ❌ (lo más importante)
Hoy `Connector` guarda origen/Z/X fijos en coordenadas de la pieza: si la pieza cambia, la
unión queda en el aire.
- `Connector.at: Option<ConnectorRef>`: de qué sale (`FaceCenter(FaceRef)`, `FaceAxis(FaceRef)`,
  `EdgeCenter(EdgeRef)` para bordes redondos, `EdgeMid(EdgeRef)`, `Vertex(P3 + etiquetas)`),
  más `offset` (mm a lo largo de Z) y `spin` (giro de X alrededor de Z, °).
- Al recalcular, resolver cada conector contra la pieza de su instancia (como `resolve_face`
  pero sobre `Part.shape` y sus etiquetas) y recalcular origen/Z/X. Los guardados quedan como
  respaldo: si la referencia se pierde, la unión usa el respaldo y sale como «perdida» (aviso,
  igual que las operaciones con referencias perdidas).
- Proyectos viejos: conectores sin `at` siguen igual (serde default).
- Pruebas: cambiar el largo de una caja → la bisagra sigue en su cara; borrar la cara → aviso.

### EN2 — Elegir bien el punto de unión ❌
- Elegir también aristas (centro de un borde redondo, punto medio de una recta) y vértices,
  no solo caras planas/cilíndricas.
- Girar el conector (botones 90° y ángulo libre) y voltearlo; separarlo (offset).
- Ver el conector en el visor (pequeños ejes) al elegir y al pasar el mouse por una unión.
- Vista previa de la unión antes de aceptar (la pieza B se mueve en vivo).

### EN3 — Editar una unión ❌
- Cambiar el tipo, volver a elegir la cara de A o de B, renombrar.
- Clic en una unión del panel → resalta las dos caras y sus conectores en el visor.

### EN4 — Uniones y opciones que faltan ❌
- Separación en plana y pegada (dos caras a N mm) y ángulo entre caras.
- Tangente (cilindro contra plano / cilindro contra cilindro).
- Centrar entre dos caras («ancho»).
- Topes mínimo/máximo en bisagras, deslizantes y cilíndricas (desigualdad: término que solo
  pesa al pasarse; el arrastre y la animación se detienen en el tope).
- Acoplamientos nuevos: lineal entre dos deslizantes, correa con poleas (usar `belts.rs`),
  leva sencilla (seguidor sobre un perfil).

### EN5 — Que diga qué falla, y que aguante muchas piezas ❌
- Desvío por unión: cuál no se cumple (en rojo en el panel y en el visor).
- Uniones que sobran (no agregan nada) o que se contradicen.
- Grados libres pieza por pieza (las que todavía se mueven, resaltadas).
- Rendimiento: resolver por grupos de piezas conectadas por separado; jacobiano analítico en
  vez de numérico. Meta: 100 piezas con 150 uniones en menos de 100 ms (banco en pruebas).

### EN6 — Mover piezas con el mouse ❌
- Manija para mover/girar una pieza libre (o con plana, rótula, paralela), con pasos de
  ajuste.
- Arrastre general con el solver: la pieza sigue al mouse respetando sus uniones (la pose
  pedida entra como objetivo blando), no solo por una bisagra o un deslizante.

### EN7 — Choques en vivo ❌
- Choques revisados solos al soltar un arrastre o mover un ángulo (en segundo plano), marcados
  en rojo en el visor, no solo como texto.
- En el recorrido de una unión: el primer cuadro en que algo choca.

### EN8 — Manejo de piezas ❌
- Repetir una instancia en fila o en círculo, y siguiendo un patrón de agujeros del diseño.
- Espejo de una instancia.
- Reemplazar una pieza por otra conservando las uniones cuyas caras se encuentran.
- Ocultar y suprimir una instancia; color propio en el ensamble.
- Grupos (varias piezas que se mueven juntas sin uniones).

### EN9 — Tornillos que se ponen solos ❌
- Elegir uno o varios agujeros → tornillo de la biblioteca del diámetro y largo justos, con
  arandela y tuerca si hace falta, ya unidos (cilíndrica + plana) en cada agujero.
- Sale en la lista de materiales (pestaña «Piezas compradas»).

### EN10 — Animación dentro de la app ❌
- Reproducir el recorrido en el visor (play/pausa, barra de tiempo).
- Varias uniones en pasos (primero abre la tapa, después gira la manija); exportar el GLB
  desde esos pasos.

### EN11 — Explosionada por pasos ❌
- Pasos guardados: qué piezas, en qué dirección y cuánto; líneas de recorrido.
- Llevarla al plano 2D (vista explosionada con globos) y exportarla.

### EN12 — STEP con estructura ❌
- STEP de ensamble con árbol: cada pieza una vez y sus copias como ubicaciones, subensambles
  anidados, nombres y colores (otros programas lo abren como ensamble, no como piezas sueltas).

### EN13 — Subensambles que se mueven ❌
- Opción por instancia de subensamble: «flexible» (sus uniones entran al solver de arriba) o
  rígido como hoy.

### EN14 — Diseñar dentro del ensamble ❌
- Crear o editar una pieza desde el ensamble usando caras de otras instancias como referencia
  (la tapa toma las medidas de la caja); la referencia sigue a la otra pieza al cambiarla.

## Notas para trabajar
- Textos nuevos de la interfaz con `tl()` y su traducción en el catálogo `en.<zona>.ts`.
- Antes de cada release: `tsc --noEmit` y `cargo check -p pinocchio-app`.
- e2e solo si el usuario lo pide; pruebas del solver en `libs/cad/model/tests/assembly*.rs`.
