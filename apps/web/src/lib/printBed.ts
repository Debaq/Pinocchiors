/**
 * Cama de la impresora para el visor, en mm y con la esquina de la cama en el
 * origen: X hacia la derecha y la profundidad de la cama hacia −Z (escena con
 * Y arriba, como la vista previa del gcode). Con `height` también se dibuja el
 * volumen de impresión.
 */
import * as THREE from "three";

export function buildPrintBed(bx: number, by: number, height?: number, plateOpacity = 0.85): THREE.Group {
  const group = new THREE.Group();
  const plate = new THREE.Mesh(
    new THREE.PlaneGeometry(bx, by).rotateX(-Math.PI / 2).translate(bx / 2, -0.01, -by / 2),
    new THREE.MeshBasicMaterial({ color: 0x3a3d44, transparent: true, opacity: plateOpacity, side: THREE.DoubleSide, depthWrite: false }),
  );
  group.add(plate);
  const outline = new THREE.LineLoop(
    new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(0, 0, 0), new THREE.Vector3(bx, 0, 0), new THREE.Vector3(bx, 0, -by), new THREE.Vector3(0, 0, -by)]),
    new THREE.LineBasicMaterial({ color: 0x8a8f99 }),
  );
  group.add(outline);
  // Cuadrícula cada 10 mm
  const pts: THREE.Vector3[] = [];
  for (let x = 10; x < bx; x += 10) pts.push(new THREE.Vector3(x, 0, 0), new THREE.Vector3(x, 0, -by));
  for (let y = 10; y < by; y += 10) pts.push(new THREE.Vector3(0, 0, -y), new THREE.Vector3(bx, 0, -y));
  group.add(new THREE.LineSegments(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: 0x555a63, transparent: true, opacity: 0.6 })));
  if (height && height > 0) {
    const box = new THREE.BoxGeometry(bx, height, by).translate(bx / 2, height / 2, -by / 2);
    const edges = new THREE.LineSegments(new THREE.EdgesGeometry(box), new THREE.LineBasicMaterial({ color: 0x8a8f99, transparent: true, opacity: 0.35, depthWrite: false }));
    box.dispose();
    group.add(edges);
  }
  return group;
}

/** Libera geometrías y materiales de una cama */
export function disposePrintBed(group: THREE.Group): void {
  group.traverse((o) => {
    if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments || o instanceof THREE.LineLoop) {
      o.geometry.dispose();
      (o.material as THREE.Material).dispose();
    }
  });
}
