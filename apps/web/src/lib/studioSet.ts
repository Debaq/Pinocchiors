import * as THREE from "three";

/** Fondo de estudio pedido: el color del ciclorama y qué tan brillante es el piso */
export interface StudioOptions {
  /** #rrggbb */
  color: string;
  /** 0 = piso mate; 1 = brillante */
  gloss: number;
}

/**
 * Ciclorama de estudio: un piso que sube en curva hasta un muro, sin costuras, bajo el modelo y
 * detrás de él (hacia `forward`, la dirección en que mira la cámara). Es lo que usan los fotógrafos
 * para fotos de producto; las medidas son las mismas que usa GeppettoRay (`studio.rs`).
 * Las caras miran hacia el interior (desde abajo no se ve el piso) y recibe sombras sin proyectarlas.
 */
export function makeStudioSet(box: THREE.Box3, forward: THREE.Vector3, opts: StudioOptions): THREE.Mesh {
  const size = Math.max(box.getSize(new THREE.Vector3()).length(), 1e-3);
  const up = new THREE.Vector3(0, 1, 0);
  const back = new THREE.Vector3(forward.x, 0, forward.z);
  if (back.lengthSq() < 1e-8) back.set(0, 0, -1);
  back.normalize();
  const side = new THREE.Vector3().crossVectors(up, back).normalize();
  const [front, behind, halfW, radius, height] = [7 * size, 2.5 * size, 9 * size, 0.9 * size, 7 * size];
  const origin = new THREE.Vector3((box.min.x + box.max.x) / 2, box.min.y, (box.min.z + box.max.z) / 2);

  // Perfil en el plano (hacia atrás, arriba), con su normal hacia el interior
  type P = { b: number; u: number; n: THREE.Vector3 };
  const profile: P[] = [{ b: -front, u: 0, n: up.clone() }];
  const steps = 16;
  for (let i = 0; i <= steps; i++) {
    const th = (Math.PI / 2) * (i / steps);
    const [sn, cs] = [Math.sin(th), Math.cos(th)];
    profile.push({ b: behind + radius * sn, u: radius * (1 - cs), n: up.clone().multiplyScalar(cs).addScaledVector(back, -sn).normalize() });
  }
  profile.push({ b: behind + radius, u: height, n: back.clone().negate() });

  const at = (b: number, u: number, x: number) => origin.clone().addScaledVector(back, b).addScaledVector(up, u).addScaledVector(side, x);
  const positions: number[] = [];
  const normals: number[] = [];
  const push = (p: THREE.Vector3[], n: THREE.Vector3[]) => {
    // La cara frontal mira hacia donde apuntan las normales del perfil
    const g = new THREE.Vector3().subVectors(p[1], p[0]).cross(new THREE.Vector3().subVectors(p[2], p[0]));
    const want = n[0].clone().add(n[1]).add(n[2]);
    const order = g.dot(want) >= 0 ? [0, 1, 2] : [0, 2, 1];
    for (const k of order) {
      positions.push(p[k].x, p[k].y, p[k].z);
      normals.push(n[k].x, n[k].y, n[k].z);
    }
  };
  for (let i = 0; i + 1 < profile.length; i++) {
    const [a, b] = [profile[i], profile[i + 1]];
    const [p0, p1, p2, p3] = [at(a.b, a.u, -halfW), at(a.b, a.u, halfW), at(b.b, b.u, halfW), at(b.b, b.u, -halfW)];
    push([p0, p1, p2], [a.n, a.n, b.n]);
    push([p0, p2, p3], [a.n, b.n, b.n]);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("normal", new THREE.Float32BufferAttribute(normals, 3));
  const gloss = Math.min(1, Math.max(0, opts.gloss));
  const material = new THREE.MeshStandardMaterial({ color: opts.color, roughness: 1 - 0.8 * gloss, metalness: 0, side: THREE.FrontSide });
  const mesh = new THREE.Mesh(geometry, material);
  mesh.receiveShadow = true;
  mesh.castShadow = false;
  mesh.name = "estudio";
  // No debe entrar en la selección ni en los encuadres
  mesh.userData.studio = true;
  mesh.raycast = () => {};
  return mesh;
}

export function disposeStudioSet(mesh: THREE.Mesh): void {
  mesh.geometry.dispose();
  (mesh.material as THREE.Material).dispose();
}
