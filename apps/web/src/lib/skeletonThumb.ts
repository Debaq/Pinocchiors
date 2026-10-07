/**
 * Miniatura de un esqueleto: proyección ortográfica en vista 3/4 (de frente
 * y de costado a la vez, un poco desde arriba; desde más arriba si el
 * cuerpo es plano, como una estrella de mar), ajustada a una caja. Cada
 * hueso lleva su profundidad para dibujar más tenue lo que queda atrás.
 */

export interface TemplateShape {
  id: string;
  /** Posiciones (Y arriba, mirando a +Z) */
  points: [number, number, number][];
  /** Padre de cada hueso (−1 en la raíz) */
  parents: number[];
}

export interface Thumb {
  /** Segmentos x1, y1, x2, y2 en la caja y su profundidad (0 adelante, 1 atrás) */
  segments: { x1: number; y1: number; x2: number; y2: number; depth: number }[];
  /** Puntas (hojas) */
  tips: { x: number; y: number; depth: number }[];
}

const deg = (d: number) => (d * Math.PI) / 180;

export function thumbnail(shape: TemplateShape, width: number, height: number, padding = 6): Thumb {
  const ps = shape.points;
  if (ps.length === 0) return { segments: [], tips: [] };
  const extent = [0, 1, 2].map((k) => Math.max(...ps.map((p) => p[k])) - Math.min(...ps.map((p) => p[k])));
  // Plano de verdad: ancho y largo a la vez (estrella de mar), no un pez de costado
  const flat = extent[1] < 0.25 * Math.min(extent[0], extent[2]);
  // Girado para ver el frente y el costado izquierdo, mirando desde arriba
  const yaw = deg(-35);
  const pitch = deg(flat ? 55 : 15);
  const project = ([x, y, z]: [number, number, number]) => {
    const x1 = x * Math.cos(yaw) + z * Math.sin(yaw);
    const z1 = -x * Math.sin(yaw) + z * Math.cos(yaw);
    const y2 = y * Math.cos(pitch) - z1 * Math.sin(pitch);
    const z2 = y * Math.sin(pitch) + z1 * Math.cos(pitch);
    return [x1, y2, z2] as const;
  };
  const q = ps.map(project);
  const [minX, maxX] = [Math.min(...q.map((p) => p[0])), Math.max(...q.map((p) => p[0]))];
  const [minY, maxY] = [Math.min(...q.map((p) => p[1])), Math.max(...q.map((p) => p[1]))];
  const [minZ, maxZ] = [Math.min(...q.map((p) => p[2])), Math.max(...q.map((p) => p[2]))];
  const s = Math.min((width - 2 * padding) / Math.max(maxX - minX, 1e-6), (height - 2 * padding) / Math.max(maxY - minY, 1e-6));
  const ox = (width - s * (maxX - minX)) / 2;
  const oy = (height - s * (maxY - minY)) / 2;
  const X = (p: readonly number[]) => ox + (p[0] - minX) * s;
  const Y = (p: readonly number[]) => height - oy - (p[1] - minY) * s;
  // Más cerca de la cámara = z mayor
  const depth = (z: number) => (maxZ - minZ < 1e-6 ? 0 : (maxZ - z) / (maxZ - minZ));
  const hasChild = new Set(shape.parents.filter((p) => p >= 0));
  const segments = shape.parents.flatMap((parent, i) =>
    parent < 0 ? [] : [{ x1: X(q[parent]), y1: Y(q[parent]), x2: X(q[i]), y2: Y(q[i]), depth: depth((q[parent][2] + q[i][2]) / 2) }]
  );
  // Lo de atrás primero, para que lo de adelante quede encima
  segments.sort((a, b) => b.depth - a.depth);
  const tips = q.flatMap((p, i) => (hasChild.has(i) ? [] : [{ x: X(p), y: Y(p), depth: depth(p[2]) }]));
  return { segments, tips };
}
