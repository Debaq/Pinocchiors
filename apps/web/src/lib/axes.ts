/**
 * Ejes que ve el usuario, como en Blender: Z arriba y el frente mirando
 * hacia −Y. Por dentro todo sigue en la convención de glTF y del visor
 * (Y arriba, frente hacia +Z); este módulo traduce en la interfaz.
 *
 *   Blender (x, y, z) = interno (x, −z, y)
 *   interno (x, y, z) = Blender (x, z, −y)
 */
export type Axis = "x" | "y" | "z";
export type Vec3 = [number, number, number];

export const AXES: Axis[] = ["x", "y", "z"];

/** Eje interno (y su sentido) que corresponde a cada eje que se muestra */
export const VIEW_AXES: Record<Axis, { axis: Axis; sign: 1 | -1; index: 0 | 1 | 2 }> = {
  x: { axis: "x", sign: 1, index: 0 },
  y: { axis: "z", sign: -1, index: 2 },
  z: { axis: "y", sign: 1, index: 1 },
};

/** Punto o dirección interna → como se muestra */
export const toView = ([x, y, z]: Vec3): Vec3 => [x, -z, y];

/** Punto o dirección mostrada → interna */
export const fromView = ([x, y, z]: Vec3): Vec3 => [x, z, -y];

/** Medidas (siempre positivas) internas → en el orden X, Y, Z que se muestra */
export const viewSize = ([x, y, z]: Vec3): Vec3 => [x, z, y];

/** Medidas mostradas → internas */
export const sizeFromView = ([x, y, z]: Vec3): Vec3 => [x, z, y];
