// Íconos de las herramientas del sketch: trazo fino (24×24), los puntos que
// se marcan con clic como círculos rellenos y lo auxiliar punteado.

import { JSX } from "solid-js";

interface IconProps {
  size?: number;
  class?: string;
}

const SketchIcon = (props: IconProps & { children: JSX.Element }) => (
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

/** Punto marcado con un clic */
const Dot = (p: { x: number; y: number }) => <circle cx={p.x} cy={p.y} r="1.7" fill="currentColor" stroke="none" />;
const dashed = { "stroke-dasharray": "2 2" };

export const Select = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M6 3.5 L6 18 L10 14 L12.8 20 L15 19 L12.3 13 L18 13 Z" />
  </SketchIcon>
);

export const Line = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M5 19 L19 5" />
    <Dot x={5} y={19} />
    <Dot x={19} y={5} />
  </SketchIcon>
);

/** Línea desde el centro: el punto del medio y los dos extremos */
export const LineMid = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M4 18 L20 6" />
    <circle cx="12" cy="12" r="2.4" />
    <Dot x={4} y={18} />
    <Dot x={20} y={6} />
  </SketchIcon>
);

export const Rect = (p: IconProps) => (
  <SketchIcon {...p}>
    <rect x="4" y="6" width="16" height="12" />
    <Dot x={4} y={18} />
    <Dot x={20} y={6} />
  </SketchIcon>
);

export const RectCenter = (p: IconProps) => (
  <SketchIcon {...p}>
    <rect x="4" y="6" width="16" height="12" />
    <path d="M12 12 L20 6" {...dashed} />
    <Dot x={12} y={12} />
    <Dot x={20} y={6} />
  </SketchIcon>
);

export const Circle = (p: IconProps) => (
  <SketchIcon {...p}>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 12 L20 12" {...dashed} />
    <Dot x={12} y={12} />
  </SketchIcon>
);

export const ArcCenter = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M20 16 A9 9 0 0 0 8 6.6" />
    <path d="M11 16 L20 16 M11 16 L8 6.6" {...dashed} />
    <Dot x={11} y={16} />
    <Dot x={20} y={16} />
    <Dot x={8} y={6.6} />
  </SketchIcon>
);

export const Arc3 = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M4 18 A9.5 9.5 0 0 1 20 18" />
    <Dot x={4} y={18} />
    <Dot x={12} y={8.6} />
    <Dot x={20} y={18} />
  </SketchIcon>
);

export const TangentArc = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M3 18 L11 18" />
    <path d="M11 18 A6 6 0 0 0 17 12 A6 6 0 0 0 14 6.8" />
    <Dot x={11} y={18} />
    <Dot x={14} y={6.8} />
  </SketchIcon>
);

export const Ellipse = (p: IconProps) => (
  <SketchIcon {...p}>
    <ellipse cx="12" cy="12" rx="9" ry="5.5" />
    <path d="M12 12 L21 12 M12 12 L12 6.5" {...dashed} />
    <Dot x={12} y={12} />
  </SketchIcon>
);

export const Polygon = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M12 3.5 L19.4 7.75 L19.4 16.25 L12 20.5 L4.6 16.25 L4.6 7.75 Z" />
    <Dot x={12} y={12} />
  </SketchIcon>
);

export const Slot = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M8 7.5 L16 7.5 A4.5 4.5 0 0 1 16 16.5 L8 16.5 A4.5 4.5 0 0 1 8 7.5 Z" />
    <path d="M8 12 L16 12" {...dashed} />
    <Dot x={8} y={12} />
    <Dot x={16} y={12} />
  </SketchIcon>
);

export const Spline = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M3 17 C7 4 11 4 12 12 S17 20 21 7" />
    <Dot x={3} y={17} />
    <Dot x={12} y={12} />
    <Dot x={21} y={7} />
  </SketchIcon>
);

export const Point = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M12 5 L12 9 M12 15 L12 19 M5 12 L9 12 M15 12 L19 12" />
    <Dot x={12} y={12} />
  </SketchIcon>
);

export const Text = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M5 6 L19 6 M12 6 L12 19 M9 19 L15 19" />
  </SketchIcon>
);

export const Trim = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M4 12 L9 12" />
    <path d="M9 12 L20 12" {...dashed} />
    <path d="M9 4 L9 20" />
    <path d="M14.5 9 L18.5 15 M18.5 9 L14.5 15" stroke-width="1.3" />
  </SketchIcon>
);

export const Extend = (p: IconProps) => (
  <SketchIcon {...p}>
    <path d="M4 12 L10 12" />
    <path d="M10 12 L18 12" {...dashed} />
    <path d="M15 9 L18 12 L15 15" />
    <path d="M20 4 L20 20" />
  </SketchIcon>
);
