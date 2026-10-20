// Banco del picking: raycast con y sin BVH sobre una malla sintética (node perftest/bvhBench.mjs)
// Se empaqueta con esbuild porque three-mesh-bvh y meshBvh.ts son TS/ESM del repo.
import { build } from "esbuild";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(mkdtempSync(join(tmpdir(), "bvhbench-")), "bench.mjs");
await build({
  stdin: {
    contents: `
import * as THREE from "three";
import { ensureBvh, markBvhStale, disposeBvh, pickMesh } from "./src/lib/meshBvh.ts";
const TRIS = Number(process.env.TRIS ?? 500000);
const RAYS = Number(process.env.RAYS ?? 200);
// Esfera UV con ~TRIS triángulos
const rows = Math.round(Math.sqrt(TRIS / 4)), cols = rows * 2;
const pos = new Float32Array((rows + 1) * (cols + 1) * 3), idx = new Uint32Array(rows * cols * 6);
let p = 0;
for (let i = 0; i <= rows; i++) for (let j = 0; j <= cols; j++) {
  const th = (i / rows) * Math.PI, ph = (j / cols) * 2 * Math.PI;
  pos[p++] = Math.sin(th) * Math.cos(ph); pos[p++] = Math.cos(th); pos[p++] = Math.sin(th) * Math.sin(ph);
}
let q = 0;
for (let i = 0; i < rows; i++) for (let j = 0; j < cols; j++) {
  const a = i * (cols + 1) + j, b = a + cols + 1;
  idx[q++] = a; idx[q++] = b; idx[q++] = a + 1; idx[q++] = a + 1; idx[q++] = b; idx[q++] = b + 1;
}
const geo = new THREE.BufferGeometry();
geo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
geo.setIndex(new THREE.BufferAttribute(idx, 1));
const idxCopy = idx.slice();
const mesh = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ side: THREE.DoubleSide }));
console.log("triangulos:", idx.length / 3);
const rays = [];
let s = 7; const r = () => ((s = (s * 16807) % 2147483647) / 2147483647) * 2 - 1;
for (let i = 0; i < RAYS; i++) {
  const rc = new THREE.Raycaster();
  rc.set(new THREE.Vector3(r() * 0.9, r() * 0.9, 5), new THREE.Vector3(0, 0, -1));
  rc.firstHitOnly = true;
  rays.push(rc);
}
const time = (fn) => { const t = performance.now(); const v = fn(); return [performance.now() - t, v]; };
// Sin BVH (raycast de three tal cual)
const [tPlain, hitsPlain] = time(() => rays.map((rc) => rc.intersectObject(mesh)[0]?.faceIndex ?? -1));
console.log("sin BVH : " + (tPlain / RAYS).toFixed(2) + " ms/rayo");
const [tBuild] = time(() => ensureBvh(mesh));
console.log("construir BVH (indirect): " + tBuild.toFixed(0) + " ms");
const [tFast, hitsFast] = time(() => rays.map((rc) => pickMesh(rc, mesh)?.faceIndex ?? -1));
console.log("con BVH : " + (tFast / RAYS).toFixed(4) + " ms/rayo  (x" + (tPlain / tFast).toFixed(0) + ")");
let same = 0; for (let i = 0; i < RAYS; i++) if (hitsPlain[i] === hitsFast[i]) same++;
console.log("mismo triangulo que three: " + same + "/" + RAYS);
let intact = true; for (let i = 0; i < idx.length; i++) if (idx[i] !== idxCopy[i]) { intact = false; break; }
console.log("indice de la geometria intacto:", intact);
// Refit tras mover vertices
for (let i = 2; i < pos.length; i += 3) pos[i] += 0.5;
markBvhStale(mesh);
const [tRefit] = time(() => ensureBvh(mesh));
console.log("refit tras mover vertices: " + tRefit.toFixed(0) + " ms");
const rc = rays[0]; rc.set(new THREE.Vector3(0, 0, 5), new THREE.Vector3(0, 0, -1));
const h = pickMesh(rc, mesh); console.log("tras refit, impacto z=" + h?.point.z.toFixed(3) + " (esperado ~1.5)");
disposeBvh(mesh); console.log("dispose ok:", !geo.boundsTree);
`,
    resolveDir: root,
    loader: "ts",
  },
  bundle: true, platform: "node", format: "esm", outfile: out, logLevel: "error",
});
await import(pathToFileURL(out).href);
