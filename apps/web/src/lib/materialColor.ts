// Los factores de glTF son lineales; el selector de color del navegador es sRGB
const toSrgb = (x: number) => {
  const v = Math.min(1, Math.max(0, x));
  return v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055;
};
const toLinear = (x: number) => (x <= 0.04045 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4));

export const linearToHex = (c: number[]) =>
  "#" + [0, 1, 2].map((i) => Math.round(255 * toSrgb(c[i])).toString(16).padStart(2, "0")).join("");

export const hexToLinear = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => toLinear(v / 255)) as [number, number, number];
};
