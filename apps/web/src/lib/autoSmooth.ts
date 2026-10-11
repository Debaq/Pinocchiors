/**
 * Suavizado automático para dibujar: una malla soldada (pieza del CAD, STL)
 * tiene en las aristas vivas una sola normal por vértice, promedio de caras
 * que no se parecen, y las caras planas se ven con manchas. El backend marca
 * por vértice si toca una arista viva (atributo `crease`): los triángulos
 * que tocan una se dibujan con la normal de la cara (planos, como en un
 * laminador) y el resto queda suave. Sin la marca, donde la normal del
 * vértice se aparta mucho de la de la cara se usa la de la cara (entre 30° y
 * 45° se mezclan). No toca los vértices: pesos y pintura siguen por índice.
 *
 * Se agrega a los fragmentos de three.js con el define `AUTO_SMOOTH`; sin
 * él, el shader queda igual.
 */

import * as THREE from "three";

const ACTIVE = "#if defined( AUTO_SMOOTH ) && ! defined( FLAT_SHADED )";

const VERTEX_PARS = /* glsl */ `
${ACTIVE}
attribute float crease;
varying float vCrease;
#endif
`;

const VERTEX = /* glsl */ `
${ACTIVE}
vCrease = crease;
#endif
`;

const FRAGMENT_PARS = /* glsl */ `
${ACTIVE}
varying float vCrease;
#endif
`;

const CODE = /* glsl */ `
${ACTIVE}
{
	vec3 facet = normalize( cross( dFdx( vViewPosition ), dFdy( vViewPosition ) ) );
	float d = dot( normal, facet );
	if ( d < 0.0 ) { facet = - facet; d = - d; }
	// Interpolada: mayor que cero en todo triángulo con un vértice marcado
	normal = vCrease > 0.0 ? facet : normalize( mix( facet, normal, smoothstep( 0.9511, 0.9848, d ) ) );
}
#endif
`;

let installed = false;

/** Agrega el suavizado automático a los shaders de three.js (una vez) */
export function installAutoSmooth(): void {
  if (installed) return;
  installed = true;
  const chunks = THREE.ShaderChunk as unknown as Record<string, string>;
  const marker = "#if defined( USE_NORMALMAP_TANGENTSPACE )";
  const src = chunks.normal_fragment_begin;
  const at = src.indexOf(marker);
  // Otra versión de three sin ese punto: se deja como está
  if (at < 0) return;
  chunks.normal_fragment_begin = src.slice(0, at) + CODE + src.slice(at);
  chunks.uv_pars_vertex += VERTEX_PARS;
  chunks.begin_vertex += VERTEX;
  chunks.uv_pars_fragment += FRAGMENT_PARS;
}

/** Enciende o apaga el suavizado automático en un material (recompila si cambia) */
export function setAutoSmooth(material: THREE.Material, on: boolean): void {
  const m = material as THREE.Material & { defines?: Record<string, unknown> };
  const has = !!m.defines && "AUTO_SMOOTH" in m.defines;
  if (has === on) return;
  const defines = { ...(m.defines ?? {}) };
  if (on) defines.AUTO_SMOOTH = "";
  else delete defines.AUTO_SMOOTH;
  m.defines = defines;
  m.needsUpdate = true;
}
