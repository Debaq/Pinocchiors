/**
 * Suavizado automático para dibujar: donde la normal del vértice se aparta
 * mucho de la de la cara (una arista viva con la normal promediada, como en
 * una malla soldada de una pieza del CAD o un STL), se usa la de la cara.
 * Entre 30° y 45° se mezclan; por debajo queda la suave. Así una caja se ve
 * con caras planas y una malla orgánica no cambia, sin tocar los vértices
 * (los pesos y la pintura siguen yendo por índice).
 *
 * Se agrega al fragmento de normales de three.js con el define
 * `AUTO_SMOOTH`; sin él, el shader queda igual.
 */

import * as THREE from "three";

const CODE = /* glsl */ `
#if defined( AUTO_SMOOTH ) && ! defined( FLAT_SHADED )
{
	vec3 facet = normalize( cross( dFdx( vViewPosition ), dFdy( vViewPosition ) ) );
	float d = dot( normal, facet );
	if ( d < 0.0 ) { facet = - facet; d = - d; }
	normal = normalize( mix( facet, normal, smoothstep( 0.9511, 0.9848, d ) ) );
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
