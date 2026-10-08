// Íconos de las operaciones de Diseñar (barra de herramientas): mismo trazo
// fino que los del sketch (24×24); el sólido en vista isométrica simple y lo
// que agrega la operación resaltado con relleno suave.

import { JSX } from "solid-js";

interface IconProps {
  size?: number;
  class?: string;
}

const DesignIcon = (props: IconProps & { children: JSX.Element }) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width={props.size ?? 18}
    height={props.size ?? 18}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.6"
    stroke-linecap="round"
    stroke-linejoin="round"
    class={props.class}
  >
    {props.children}
  </svg>
);

const soft = { fill: "currentColor", "fill-opacity": "0.18" };
const dashed = { "stroke-dasharray": "2 2" };

/** Caja isométrica: frente, tapa y lateral */
const Box = (p: { x?: number; y?: number; s?: number }) => {
  const x = p.x ?? 4;
  const y = p.y ?? 8;
  const s = p.s ?? 11;
  const d = s * 0.45;
  return (
    <>
      <path d={`M${x} ${y} h${s} v${s} h${-s} Z`} />
      <path d={`M${x} ${y} l${d} ${-d} h${s} l${-d} ${d}`} />
      <path d={`M${x + s} ${y + s} l${d} ${-d} v${-s}`} />
    </>
  );
};

export const Sketch = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 17 L9 13 L21 13 L15 17 Z" {...dashed} />
    <path d="M14 4 L18 8 L10 16 L6 17 L7 13 Z" />
    <path d="M12.5 5.5 L16.5 9.5" />
  </DesignIcon>
);

export const Extrude = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M4 18 L10 15 L20 15 L14 18 Z" {...dashed} />
    <path d="M4 18 V10 L10 7 H20 V15" />
    <path d="M4 10 H14 L20 7" />
    <path d="M14 10 V18" />
    <path d="M12 6 V1.8 M10 3.8 L12 1.8 L14 3.8" />
  </DesignIcon>
);

export const Revolve = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M12 2 V22" {...dashed} />
    <path d="M12 6 H16 L17 12 L15 18 H12" {...soft} />
    <path d="M7.5 9 A5 2.2 0 1 0 12 7" />
    <path d="M10.5 6 L12 7 L10.5 8.3" />
  </DesignIcon>
);

export const Sweep = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M4 19 C8 19 9 9 14 8 S20 5 20 3" {...dashed} />
    <circle cx="5" cy="18" r="3" {...soft} />
    <path d="M7 16 C10 14 10 9 14 7.5" />
  </DesignIcon>
);

export const Loft = (p: IconProps) => (
  <DesignIcon {...p}>
    <rect x="3" y="15" width="10" height="5" />
    <circle cx="16" cy="6" r="3.5" />
    <path d="M3 15 L12.5 6 M13 15 L19.5 6" />
  </DesignIcon>
);

export const Hole = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={3} y={9} s={12} />
    <ellipse cx="9" cy="6.3" rx="3" ry="1.3" {...soft} />
    <path d="M6 6.3 V14 M12 6.3 V14" {...dashed} />
  </DesignIcon>
);

export const Fillet = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M4 20 V10 A6 6 0 0 1 10 4 H20" />
    <path d="M4 10 A6 6 0 0 1 10 4" stroke-width="2.6" />
  </DesignIcon>
);

export const Chamfer = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M4 20 V10 L10 4 H20" />
    <path d="M4 10 L10 4" stroke-width="2.6" />
  </DesignIcon>
);

export const Shell = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={4} y={9} s={11} />
    <path d="M6.5 8 L8.5 6 H16.5 L14.5 8 Z" {...soft} />
  </DesignIcon>
);

export const Draft = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M6 20 L8 6 H16 L18 20 Z" />
    <path d="M8 20 V6" {...dashed} />
  </DesignIcon>
);

export const PatternLinear = (p: IconProps) => (
  <DesignIcon {...p}>
    <rect x="2.5" y="9" width="5" height="6" {...soft} />
    <rect x="9.5" y="9" width="5" height="6" />
    <rect x="16.5" y="9" width="5" height="6" />
  </DesignIcon>
);

export const PatternCircular = (p: IconProps) => (
  <DesignIcon {...p}>
    <circle cx="12" cy="12" r="7" {...dashed} />
    <circle cx="12" cy="5" r="2" {...soft} />
    <circle cx="19" cy="12" r="2" />
    <circle cx="12" cy="19" r="2" />
    <circle cx="5" cy="12" r="2" />
  </DesignIcon>
);

export const Mirror = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M12 3 V21" {...dashed} />
    <path d="M10 7 L4 9 V16 L10 18 Z" {...soft} />
    <path d="M14 7 L20 9 V16 L14 18 Z" />
  </DesignIcon>
);

export const CurvePattern = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 18 C8 18 10 6 21 6" {...dashed} />
    <circle cx="4" cy="18" r="1.8" {...soft} />
    <circle cx="10" cy="13" r="1.8" />
    <circle cx="17" cy="7" r="1.8" />
  </DesignIcon>
);

export const Boolean = (p: IconProps) => (
  <DesignIcon {...p}>
    <circle cx="9" cy="12" r="6" />
    <circle cx="15" cy="12" r="6" />
    <path d="M12 7 A6 6 0 0 1 12 17 A6 6 0 0 1 12 7" {...soft} />
  </DesignIcon>
);

export const SplitParts = (p: IconProps) => (
  <DesignIcon {...p}>
    <rect x="3" y="8" width="7" height="8" />
    <rect x="14" y="8" width="7" height="8" />
    <path d="M10.5 12 H13.5" {...dashed} />
  </DesignIcon>
);

export const SplitPlane = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={4} y={9} s={11} />
    <path d="M2 14 L8 9 H22 L16 14 Z" {...soft} {...dashed} />
  </DesignIcon>
);

export const DeleteParts = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={3} y={10} s={9} />
    <path d="M15 14 L21 20 M21 14 L15 20" />
  </DesignIcon>
);

export const Plane = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 16 L9 8 H21 L15 16 Z" {...soft} />
  </DesignIcon>
);

export const Axis = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M4 20 L20 4" />
    <path d="M2.5 21.5 L4 20 M20 4 L21.5 2.5" {...dashed} />
    <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
  </DesignIcon>
);

export const Point = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M12 4 V9 M12 15 V20 M4 12 H9 M15 12 H20" />
    <circle cx="12" cy="12" r="2" fill="currentColor" stroke="none" />
  </DesignIcon>
);

export const Helix = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M6 20 C18 20 18 16 6 16 C18 16 18 12 6 12 C18 12 18 8 6 8 C18 8 18 4 6 4" />
  </DesignIcon>
);

export const Thicken = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 14 C8 8 16 8 21 14" />
    <path d="M3 18 C8 12 16 12 21 18" {...dashed} />
    <path d="M12 10 V14" />
  </DesignIcon>
);

export const MoveFace = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={3} y={10} s={10} />
    <path d="M17 12 H22 M20 10 L22 12 L20 14" />
    <path d="M13 10 L17.5 5.5 V15.5 L13 20 Z" {...soft} />
  </DesignIcon>
);

export const Thread = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M8 3 V21 M16 3 V21" {...dashed} />
    <path d="M7 6 L17 8 M7 10 L17 12 M7 14 L17 16 M7 18 L17 20" />
  </DesignIcon>
);

export const SheetMetal = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 15 L12 19 L21 13 L12 9 Z" {...soft} />
    <path d="M3 15 V16.5 L12 20.5 L21 14.5 V13" />
  </DesignIcon>
);

export const Flange = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 18 H13 A3 3 0 0 0 16 15 V4" />
    <path d="M3 20.5 H13 A5.5 5.5 0 0 0 18.5 15 V4" />
    <path d="M16 4 H18.5" />
  </DesignIcon>
);

export const FlatPattern = (p: IconProps) => (
  <DesignIcon {...p}>
    <rect x="3" y="4" width="18" height="16" {...soft} />
    <path d="M3 9 H21 M3 15 H21" stroke-dasharray="3 1.5" />
  </DesignIcon>
);

export const Bolt = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M6 3 H18 V8 H6 Z" {...soft} />
    <path d="M9 8 V19 L12 21 L15 19 V8" />
    <path d="M9 11 L15 12.5 M9 14 L15 15.5 M9 17 L15 18.5" />
  </DesignIcon>
);

export const Nut = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M12 3 L19.8 7.5 V16.5 L12 21 L4.2 16.5 V7.5 Z" {...soft} />
    <circle cx="12" cy="12" r="3.5" />
  </DesignIcon>
);

export const Washer = (p: IconProps) => (
  <DesignIcon {...p}>
    <circle cx="12" cy="12" r="8.5" {...soft} />
    <circle cx="12" cy="12" r="3.5" />
  </DesignIcon>
);

export const Rib = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 20 H21 M5 20 V6 H8 V20" />
    <path d="M8 9 L18 20 H8 Z" {...soft} />
  </DesignIcon>
);

export const ReplaceFace = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={3} y={10} s={10} />
    <path d="M19 3 V21" {...dashed} />
    <path d="M14 15 H18 M16 13 L18 15 L16 17" />
  </DesignIcon>
);

export const Scale = (p: IconProps) => (
  <DesignIcon {...p}>
    <rect x="3" y="11" width="8" height="8" {...soft} />
    <path d="M3 11 V5 H19 V19 H11" {...dashed} />
    <path d="M12 12 L18 6 M14 6 H18 V10" />
  </DesignIcon>
);

export const BoxShape = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={4} y={9} s={11} />
  </DesignIcon>
);

export const Cylinder = (p: IconProps) => (
  <DesignIcon {...p}>
    <ellipse cx="12" cy="6" rx="7" ry="2.5" />
    <path d="M5 6 V18 A7 2.5 0 0 0 19 18 V6" />
  </DesignIcon>
);

export const Sphere = (p: IconProps) => (
  <DesignIcon {...p}>
    <circle cx="12" cy="12" r="8" />
    <ellipse cx="12" cy="12" rx="8" ry="3" {...dashed} />
  </DesignIcon>
);

export const Cone = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M12 3 L5 18 M12 3 L19 18" />
    <ellipse cx="12" cy="18" rx="7" ry="2.5" />
  </DesignIcon>
);

export const Torus = (p: IconProps) => (
  <DesignIcon {...p}>
    <ellipse cx="12" cy="12" rx="9" ry="5" />
    <ellipse cx="12" cy="11.5" rx="3.5" ry="1.6" />
  </DesignIcon>
);

export const Import = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M6 3 H14 L18 7 V21 H6 Z" />
    <path d="M14 3 V7 H18" />
    <path d="M12 10 V17 M9 14 L12 17 L15 14" />
  </DesignIcon>
);

export const Drawing = (p: IconProps) => (
  <DesignIcon {...p}>
    <rect x="3" y="4" width="18" height="16" />
    <rect x="6" y="7" width="5" height="5" />
    <rect x="13" y="7" width="5" height="5" />
    <path d="M13 16 H21 M13 16 V20" />
  </DesignIcon>
);

export const Assembly = (p: IconProps) => (
  <DesignIcon {...p}>
    <Box x={3} y={11} s={8} />
    <circle cx="17" cy="8" r="4" />
    <path d="M11 13 L14 10" {...dashed} />
  </DesignIcon>
);

export const Measure = (p: IconProps) => (
  <DesignIcon {...p}>
    <path d="M3 17 L17 3 L21 7 L7 21 Z" />
    <path d="M7 13 L9 15 M10 10 L12 12 M13 7 L15 9" />
  </DesignIcon>
);
