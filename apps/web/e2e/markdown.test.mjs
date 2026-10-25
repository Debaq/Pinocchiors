// Markdown de las respuestas de la IA (node --experimental-strip-types)
import { test } from "node:test";
import assert from "node:assert/strict";
import { parseInline, parseMarkdown } from "../src/lib/markdown.ts";

test("negrita, cursiva, código y enlaces en línea", () => {
  assert.deepEqual(parseInline("**Carcasa:** dos `asm_check` y *libre*"), [
    { t: "b", c: [{ t: "text", v: "Carcasa:" }] },
    { t: "text", v: " dos " },
    { t: "code", v: "asm_check" },
    { t: "text", v: " y " },
    { t: "i", c: [{ t: "text", v: "libre" }] },
  ]);
  assert.deepEqual(parseInline("snake_case_name y 2*3*4"), [{ t: "text", v: "snake_case_name y 2*3*4" }]);
  assert.deepEqual(parseInline("[docs](https://x.y/z)"), [{ t: "a", href: "https://x.y/z", c: [{ t: "text", v: "docs" }] }]);
  assert.deepEqual(parseInline("\\*no\\*"), [{ t: "text", v: "*no*" }]);
});

test("respuesta típica: párrafo, título en negrita y lista", () => {
  const b = parseMarkdown("Hice la caja.\n\n**Relaciones**\n- Dos ejes.\n- **1:1:** Ø60.\n\nFin.");
  assert.deepEqual(b.map((x) => x.t), ["p", "p", "list", "p"]);
  assert.equal(b[2].items.length, 2);
  assert.equal(b[2].ordered, false);
  assert.equal(b[2].items[1][0].c[0].t, "b");
});

test("listas numeradas y anidadas", () => {
  const b = parseMarkdown("3. uno\n   - a\n   - b\n4. dos");
  assert.equal(b.length, 1);
  assert.equal(b[0].ordered, true);
  assert.equal(b[0].start, 3);
  assert.equal(b[0].items.length, 2);
  assert.equal(b[0].items[0][1].t, "list");
  assert.equal(b[0].items[0][1].items.length, 2);
});

test("títulos, código, cita, línea y tabla", () => {
  const b = parseMarkdown("## Pasos\n```ts\nconst a = 1;\n\nb();\n```\n> ojo\n---\n| Pieza | Ø |\n|:--|--:|\n| piñón | 40 |\n| rueda | 80 |");
  assert.deepEqual(b.map((x) => x.t), ["h", "code", "quote", "hr", "table"]);
  assert.equal(b[0].level, 2);
  assert.equal(b[1].lang, "ts");
  assert.equal(b[1].v, "const a = 1;\n\nb();");
  assert.deepEqual(b[4].align, ["left", "right"]);
  assert.equal(b[4].rows.length, 2);
  assert.deepEqual(b[4].rows[1][1], [{ t: "text", v: "80" }]);
});

test("texto sin cerrar no se pierde", () => {
  assert.deepEqual(parseInline("a **b"), [{ t: "text", v: "a **b" }]);
  assert.deepEqual(parseInline("`abierto"), [{ t: "text", v: "`abierto" }]);
  assert.deepEqual(parseMarkdown("```\nsin cierre")[0], { t: "code", lang: "", v: "sin cierre" });
});
