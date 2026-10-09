import * as THREE from "three";
import { themeHex } from "./theme";

/**
 * Grilla del piso de los dos visores: líneas menores, mayores cada 10 y los
 * ejes X e Y (de la vista, Z arriba), en el plano y = 0 de la escena. El paso
 * es una potencia de 10 de la unidad elegida, según el tamaño del modelo.
 * Vacía `group` y lo vuelve a llenar; devuelve el paso y el de las mayores
 * (en la unidad).
 */
export function buildGridLines(
  group: THREE.Group,
  extent: number,
  /** Escena → unidad que se muestra */
  toUnit: number,
): { step: number; major: number } {
  for (const child of [...group.children]) {
    group.remove(child);
    const line = child as THREE.LineSegments;
    line.geometry.dispose();
    (line.material as THREE.Material).dispose();
  }
  const extentUnits = Math.max(extent * toUnit, 1e-9);
  const step = Math.pow(10, Math.floor(Math.log10(extentUnits)) - 1);
  const major = step * 10;
  const half = Math.max(Math.ceil((extentUnits * 1.5) / major), 1) * major;
  const toScene = 1 / toUnit;
  const lines = (every: number, color: number, opacity: number, skipEvery?: number) => {
    const points: number[] = [];
    const n = Math.round(half / every);
    for (let i = -n; i <= n; i++) {
      if (i === 0 || (skipEvery && i % skipEvery === 0)) continue;
      const c = i * every * toScene;
      const h = half * toScene;
      points.push(-h, 0, c, h, 0, c, c, 0, -h, c, 0, h);
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(points, 3));
    return new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ color, transparent: true, opacity, depthWrite: false }));
  };
  const axis = (from: THREE.Vector3, to: THREE.Vector3, color: number) =>
    new THREE.LineSegments(
      new THREE.BufferGeometry().setFromPoints([from, to]),
      new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.6, depthWrite: false }),
    );
  const h = half * toScene;
  group.add(
    lines(step, themeHex("grid-minor"), 0.35, 10),
    lines(major, themeHex("grid-major"), 0.45),
    axis(new THREE.Vector3(-h, 0, 0), new THREE.Vector3(h, 0, 0), themeHex("axis-x")),
    // Z de la escena = Y de la vista (Z arriba): verde
    axis(new THREE.Vector3(0, 0, -h), new THREE.Vector3(0, 0, h), themeHex("axis-y")),
  );
  return { step, major };
}
