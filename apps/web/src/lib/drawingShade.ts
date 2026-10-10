// Vista sombreada del plano 2D: la malla de la vista (entera o cortada, del
// backend) dibujada con three.js fuera de pantalla, mirando desde la dirección
// de la vista, y devuelta como PNG para ponerla en la hoja. Las caras sobre un
// plano de corte van en otro color.

import * as THREE from "three";
import { viewUp, type P2, type ViewDir } from "./drawing";

/** Lo que devuelve el backend con `shaded` (mm del modelo) */
export interface DrawingMesh {
  positions: number[];
  normals: number[];
  indices: number[];
  /** Triángulos sobre un plano de corte */
  cut: number[];
}

let renderer: THREE.WebGLRenderer | undefined;

/**
 * La imagen (URL de datos PNG, fondo transparente) de la malla vista desde
 * `dir`, cubriendo exactamente `min`–`max` de la vista (las coordenadas de sus
 * líneas), a `pxPerMm` píxeles por mm de la pieza (a lo sumo 2048 de lado).
 */
export function shadeView(mesh: DrawingMesh, dir: ViewDir, min: P2, max: P2, pxPerMm: number): string | undefined {
  const [w0, h0] = [max[0] - min[0], max[1] - min[1]];
  if (!(w0 > 0 && h0 > 0) || !mesh.indices.length) return undefined;
  const k = Math.min(pxPerMm, 2048 / Math.max(w0, h0));
  const [w, h] = [Math.max(1, Math.round(w0 * k)), Math.max(1, Math.round(h0 * k))];
  renderer ??= new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
  renderer.setPixelRatio(1);
  renderer.setSize(w, h, false);
  renderer.setClearColor(0x000000, 0);

  // Sin índices: cada triángulo con su color (gris; lo cortado más claro, como el rayado)
  const cut = new Set(mesh.cut);
  const n = mesh.indices.length;
  const pos = new Float32Array(n * 3);
  const nor = new Float32Array(n * 3);
  const col = new Float32Array(n * 3);
  const body = new THREE.Color(0x9aa3ad);
  const face = new THREE.Color(0xd9c9a3);
  for (let i = 0; i < n; i++) {
    const v = mesh.indices[i];
    for (let c = 0; c < 3; c++) {
      pos[i * 3 + c] = mesh.positions[v * 3 + c];
      nor[i * 3 + c] = mesh.normals[v * 3 + c];
    }
    const color = cut.has(Math.floor(i / 3)) ? face : body;
    col.set([color.r, color.g, color.b], i * 3);
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.BufferAttribute(pos, 3));
  g.setAttribute("normal", new THREE.BufferAttribute(nor, 3));
  g.setAttribute("color", new THREE.BufferAttribute(col, 3));
  const m = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.65, metalness: 0.05, side: THREE.DoubleSide });
  const scene = new THREE.Scene();
  scene.add(new THREE.Mesh(g, m));

  // Cámara ortográfica: x de la cámara = la derecha de la hoja, y = arriba (eye × xdir)
  const up = viewUp(dir);
  const far = 4 * Math.max(1, ...mesh.positions.map(Math.abs));
  const cam = new THREE.OrthographicCamera(min[0], max[0], max[1], min[1], 0.01, 2 * far);
  cam.position.set(dir.eye[0] * far, dir.eye[1] * far, dir.eye[2] * far);
  cam.up.set(up[0], up[1], up[2]);
  cam.lookAt(0, 0, 0);
  // Luz de arriba a la izquierda de quien mira y un poco de ambiente
  scene.add(new THREE.HemisphereLight(0xffffff, 0x666666, 1.2));
  const key = new THREE.DirectionalLight(0xffffff, 1.6);
  const l = new THREE.Vector3(...dir.eye).multiplyScalar(2).add(new THREE.Vector3(...up)).sub(new THREE.Vector3(...dir.xdir));
  key.position.copy(l);
  scene.add(key);
  renderer.render(scene, cam);
  const url = renderer.domElement.toDataURL("image/png");
  g.dispose();
  m.dispose();
  return url;
}

/** Bytes de una URL de datos (para guardar el PNG) */
export function dataUrlBytes(url: string): number[] {
  const bin = atob(url.slice(url.indexOf(",") + 1));
  const out = new Array<number>(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
