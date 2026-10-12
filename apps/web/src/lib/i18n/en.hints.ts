import type { Catalog } from "../i18n";

/** Globitos de los paneles laterales (reemplazan párrafos de ayuda) */
export const en_hints: Catalog = {
  "Los quads respetan las islas del mapa original. Desmárcalo si vas a desplegar de nuevo en UV / Piel: salen más parejos.":
    "Quads respect the islands of the original map. Uncheck it if you'll unwrap again in UV / Skin: they come out more even.",
  "Por objeto": "Per object",
  "Cada material se retopologiza aparte con su parte de los quads (según su área): no se funden ni se pierden piezas. Ignora la simetría.":
    "Each material is retopologized on its own with its share of the quads (by area): pieces are neither merged nor lost. Ignores symmetry.",
  "Convierte la malla en quads con estos ajustes": "Turns the mesh into quads with these settings",
  "falló": "failed",
  "La simetría deja un loop de aristas en la línea media, útil para el rig.":
    "Symmetry leaves an edge loop along the midline, useful for rigging.",
  "Esqueleto, pesos, pincel y pose usan la malla que se exporta. Desmarcado usan la original (los pesos se trasladan al exportar).":
    "Skeleton, weights, brush and pose use the exported mesh. Unchecked, they use the original (weights transfer on export).",
  "Islas grandes y derechas, ubicadas como en el modelo (una hoja de vistas).": "Large, straight islands laid out like the model (a sheet of views).",
  "Islas apretadas para aprovechar la textura. Difícil de pintar a mano.": "Tightly packed islands to use the texture. Hard to paint by hand.",
  "Cabeza, torso, patas y cola según el rig. No aplica con varios objetos.": "Head, torso, legs and tail from the rig. Doesn't apply with several objects.",
  "Calcula los pesos del rig para cortar por partes del cuerpo.": "Compute the rig weights to cut by body part.",
  "Extiende la superficie en un plano para pintarla. Desplegar corta islas nuevas y hornea encima las texturas y el relieve originales. Con varios materiales, cada objeto tiene su propio mapa.":
    "Flattens the surface so you can paint it. Unwrapping cuts new islands and bakes the original textures and relief on top. With several materials, each object gets its own map.",
  "Despliega los triángulos del modelo tal cual: la forma no cambia y el rig sigue.": "Unwraps the model's triangles as they are: the shape doesn't change and the rig follows.",
  "Reemplaza sus UV y hornea sus texturas.": "Replaces its UVs and bakes its textures.",
  "Queda listo para pintar.": "Ready to paint.",
  "Deshace el desplegado de la malla original": "Undoes the unwrap of the original mesh",
  "La textura puede verse estirada ahí. Desplegar y hornear lo corrige.": "The texture may look stretched there. Unwrap and bake fixes it.",
  "Corta islas, despliega y hornea las texturas originales sobre el mapa nuevo": "Cuts islands, unwraps and bakes the original textures onto the new map",
  "Vuelve al mapa del modelo original": "Goes back to the original model's map",
  "Cómo funciona": "How it works",
  "Cómo se usa": "How to use",
  "Atajos": "Shortcuts",
  "Crear, ajustar al modelo, pesos y controles, al costado del visor (Ctrl+E). El borde se arrastra para ensancharlo.": "Create, fit to the model, weights and controls, beside the viewer (Ctrl+E). Drag the edge to widen it.",
  "Sin modelo: se exporta solo el esqueleto (GLB, glTF o BVH). Al importar un modelo se conserva.": "No model: only the skeleton is exported (GLB, glTF or BVH). It's kept when you import a model.",
  "Clic en una articulación la selecciona; G la mueve (X/Y/Z fija el eje; clic confirma, Esc cancela). S escala todo el esqueleto.": "Click a joint to select it; G moves it (X/Y/Z locks the axis; click confirms, Esc cancels). S scales the whole skeleton.",
  "Pintar (B) sobre la malla: el mapa de calor muestra el hueso activo. Ctrl invierte, Shift suaviza, F cambia el radio y Shift+F la intensidad. Recalcular descarta lo pintado.": "Paint (B) on the mesh: the heat map shows the active bone. Ctrl inverts, Shift smooths, F changes the radius and Shift+F the intensity. Recalculating discards the painting.",
  "Selecciona una articulación y pulsa R para girarla (X/Y/Z fija el eje). Los giros se suman; Esc deshace el último. Si se deforma mal, mueve esa articulación y recalcula.": "Select a joint and press R to rotate it (X/Y/Z locks the axis). Rotations add up; Esc undoes the last. If it deforms badly, move that joint and recalculate.",
  "Junta primero los pares que dan los quads más rectos y planos, sin cruzar costuras de UV ni normales partidas. Los triángulos sin pareja quedan.": "Pairs up first the triangles that make the straightest, flattest quads, without crossing UV seams or split normals. Unpaired triangles stay.",
  "Subdivide un paso sin cambiar la forma: cuatro veces más caras, ninguna de tres lados.": "Subdivides one step without changing the shape: four times the faces, none with three sides.",
  "Los vértices no se mueven. Los quads se exportan como tales en OBJ y USD; los demás formatos los triangulan.": "Vertices don't move. Quads export as quads in OBJ and USD; other formats triangulate them.",
  "Empareja los triángulos de la grilla: quedan una cuarta parte o menos (tarda un poco más).": "Evens out the grid's triangles: a quarter or fewer remain (takes a bit longer).",
  "Cierra agujeros, une piezas y quita caras internas: queda cerrada, lista para imprimir. Redondea las aristas vivas. Malla nueva sin UV: al aplicar eliges si trasladar textura y pesos.": "Closes holes, joins pieces and removes inner faces: the result is closed and print-ready. Rounds sharp edges. New mesh without UVs: when applying you choose whether to transfer texture and weights.",
  "Los vértices solo se mueven hacia afuera o adentro: quita ruido sin correr la textura. Apagado, también empareja los triángulos.": "Vertices only move outward or inward: removes noise without shifting the texture. Off, it also evens out triangle size.",
  "Malla nueva sin UV: al aplicar eliges si trasladar textura y pesos. Tarda ~20 s cada medio millón de triángulos.": "New mesh without UVs: when applying you choose whether to transfer texture and weights. Takes ~20 s per half million triangles.",
  "Llega a la cantidad pedida aunque junte partes o cierre agujeros chicos. Para reducir muchísimo cuando la forma exacta no importa.": "Reaches the requested count even if it merges nearby parts or closes small holes. For heavy reduction when exact shape doesn't matter.",
  "Reemplazada por la retopología": "Replaced by the retopology",
  "{n} quads": "{n} quads",
  "Material {n}": "Material {n}",
};
