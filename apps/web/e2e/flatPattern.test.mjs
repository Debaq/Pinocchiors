// Desarrollo de chapa en SVG y DXF (lib/flatPattern.ts), sin navegador:
//   node --test apps/web/e2e/flatPattern.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { flatSvg, flatDxf, bendLabel } from "../src/lib/flatPattern.ts";

// Canal en U desarrollado: 100 × 99,05 con dos dobleces
const fp = {
  outline: [
    [[0, 0], [100, 0]],
    [[100, 0], [100, 99.05]],
    [[100, 99.05], [0, 99.05]],
    [[0, 99.05], [0, 0]],
  ],
  bends: [
    { line: [[0, 22.26], [100, 22.26]], angle: 90, radius: 2, up: true },
    { line: [[0, 76.79], [100, 76.79]], angle: 90, radius: 2, up: false },
  ],
  thickness: 2,
  k_factor: 0.44,
  min: [0, 0],
  max: [100, 99.05],
};

test("SVG: contorno, dobleces con su texto y cotas generales a 1:1", () => {
  const svg = flatSvg(fp, "Canal");
  assert.equal((svg.match(/<polyline/g) ?? []).length, 4);
  assert.equal((svg.match(/data-bend=""/g) ?? []).length, 2);
  assert.ok(svg.includes("↑ 90° R2") && svg.includes("↓ 90° R2"));
  assert.ok(svg.includes(">100<") && svg.includes(">99,05<"), "cotas");
  assert.ok(svg.includes('width="132mm"'), "1:1 con margen de 16");
  assert.equal(bendLabel(fp.bends[0]), "↑ 90° R2");
});

test("DXF: capas, una LINE por tramo y por doblez, texto del doblez", () => {
  const dxf = flatDxf(fp);
  const lines = dxf.split("\n");
  const count = (layer) => lines.filter((l, i) => l === "LINE" && lines[i + 2] === layer).length;
  assert.equal(count("CONTORNO"), 4);
  assert.equal(count("DOBLEZ"), 2);
  assert.ok(dxf.includes("\nARRIBA 90 R2\n") && dxf.includes("\nABAJO 90 R2\n"));
  assert.ok(dxf.trimEnd().endsWith("EOF"));
});
