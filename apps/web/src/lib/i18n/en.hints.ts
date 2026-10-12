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
};
