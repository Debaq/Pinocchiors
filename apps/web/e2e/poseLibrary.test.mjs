// Biblioteca de poses: etiquetas, búsqueda y categorías (node --experimental-strip-types)
import assert from "node:assert/strict";
import { parseTags, poseMatches } from "../src/lib/poseLibrary.ts";

assert.deepEqual(parseTags(" correr, ,saltar,correr "), ["correr", "saltar"]);
assert.deepEqual(parseTags(""), []);

const pose = (id, name, extra = {}) => ({ id, name, bones: {}, ...extra });
const a = pose("a", "Guardia", { category: "Combate", tags: ["espada", "defensa"] });
const b = pose("b", "Saludo");

// Sin filtros, todas
assert.ok(poseMatches(a, "", null) && poseMatches(b, "", null));
// La búsqueda mira nombre, categoría y etiquetas, sin importar mayúsculas
assert.ok(poseMatches(a, "ESPADA", null));
assert.ok(poseMatches(a, "comb", null));
assert.ok(poseMatches(b, "salu", null) && !poseMatches(a, "salu", null));
// Categoría elegida; "" = sin categoría
assert.ok(poseMatches(a, "", "Combate") && !poseMatches(b, "", "Combate"));
assert.ok(poseMatches(b, "", "") && !poseMatches(a, "", ""));

console.log("poseLibrary ok");
