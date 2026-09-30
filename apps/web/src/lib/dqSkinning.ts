/**
 * Piel con cuaterniones duales (F9) para el visor.
 *
 * El skinning lineal mezcla matrices: al girar mucho un hueso sobre su eje
 * (antebrazo, muñeca, cuello) la piel se estrangula ("caramelo"). Con
 * cuaterniones duales se mezclan giros rígidos y el volumen se conserva.
 *
 * Se envuelven los fragmentos de skinning de three.js: con el define
 * `USE_DQS` en el material se usa esta versión; sin él, la original tal cual.
 * Solo el visor: glTF no tiene skinning por cuaterniones duales, así que lo
 * exportado se sigue viendo con piel lineal en otras herramientas.
 */

import * as THREE from "three";

const PARS = /* glsl */ `
#if defined( USE_SKINNING ) && defined( USE_DQS )
vec4 pinMatToQuat( mat3 m ) {
	float tr = m[0][0] + m[1][1] + m[2][2];
	if ( tr > 0.0 ) {
		float s = sqrt( tr + 1.0 ) * 2.0;
		return vec4( ( m[1][2] - m[2][1] ) / s, ( m[2][0] - m[0][2] ) / s, ( m[0][1] - m[1][0] ) / s, 0.25 * s );
	} else if ( m[0][0] > m[1][1] && m[0][0] > m[2][2] ) {
		float s = sqrt( 1.0 + m[0][0] - m[1][1] - m[2][2] ) * 2.0;
		return vec4( 0.25 * s, ( m[1][0] + m[0][1] ) / s, ( m[2][0] + m[0][2] ) / s, ( m[1][2] - m[2][1] ) / s );
	} else if ( m[1][1] > m[2][2] ) {
		float s = sqrt( 1.0 + m[1][1] - m[0][0] - m[2][2] ) * 2.0;
		return vec4( ( m[1][0] + m[0][1] ) / s, 0.25 * s, ( m[2][1] + m[1][2] ) / s, ( m[2][0] - m[0][2] ) / s );
	}
	float s = sqrt( 1.0 + m[2][2] - m[0][0] - m[1][1] ) * 2.0;
	return vec4( ( m[2][0] + m[0][2] ) / s, ( m[2][1] + m[1][2] ) / s, 0.25 * s, ( m[0][1] - m[1][0] ) / s );
}
vec4 pinQuatMul( vec4 a, vec4 b ) {
	return vec4( a.w * b.xyz + b.w * a.xyz + cross( a.xyz, b.xyz ), a.w * b.w - dot( a.xyz, b.xyz ) );
}
void pinBoneDQ( mat4 M, out vec4 r, out vec4 d ) {
	r = normalize( pinMatToQuat( mat3( M ) ) );
	d = 0.5 * pinQuatMul( vec4( M[3].xyz, 0.0 ), r );
}
void pinBlendDQ( mat4 a, mat4 b, mat4 c, mat4 e, vec4 w, out vec4 r, out vec4 d ) {
	vec4 r0, d0, r1, d1, r2, d2, r3, d3;
	pinBoneDQ( a, r0, d0 );
	pinBoneDQ( b, r1, d1 );
	pinBoneDQ( c, r2, d2 );
	pinBoneDQ( e, r3, d3 );
	// Todos del mismo lado de la esfera que el primero (camino corto)
	float s1 = dot( r0, r1 ) < 0.0 ? -1.0 : 1.0;
	float s2 = dot( r0, r2 ) < 0.0 ? -1.0 : 1.0;
	float s3 = dot( r0, r3 ) < 0.0 ? -1.0 : 1.0;
	r = w.x * r0 + w.y * s1 * r1 + w.z * s2 * r2 + w.w * s3 * r3;
	d = w.x * d0 + w.y * s1 * d1 + w.z * s2 * d2 + w.w * s3 * d3;
	float len = max( length( r ), 1e-8 );
	r /= len;
	d /= len;
}
vec3 pinRotate( vec4 q, vec3 v ) {
	return v + 2.0 * cross( q.xyz, cross( q.xyz, v ) + q.w * v );
}
vec3 pinDQTransform( vec4 r, vec4 d, vec3 p ) {
	return pinRotate( r, p ) + 2.0 * ( r.w * d.xyz - d.w * r.xyz + cross( r.xyz, d.xyz ) );
}
#endif
`;

const VERTEX = /* glsl */ `
	vec4 dqSkinVertex = bindMatrix * vec4( transformed, 1.0 );
	vec4 dqVR, dqVD;
	pinBlendDQ( boneMatX, boneMatY, boneMatZ, boneMatW, skinWeight, dqVR, dqVD );
	transformed = ( bindMatrixInverse * vec4( pinDQTransform( dqVR, dqVD, dqSkinVertex.xyz ), 1.0 ) ).xyz;
`;

const NORMAL = /* glsl */ `
	vec4 dqNR, dqND;
	pinBlendDQ( boneMatX, boneMatY, boneMatZ, boneMatW, skinWeight, dqNR, dqND );
	objectNormal = ( bindMatrixInverse * vec4( pinRotate( dqNR, ( bindMatrix * vec4( objectNormal, 0.0 ) ).xyz ), 0.0 ) ).xyz;
	#ifdef USE_TANGENT
		objectTangent = ( bindMatrixInverse * vec4( pinRotate( dqNR, ( bindMatrix * vec4( objectTangent, 0.0 ) ).xyz ), 0.0 ) ).xyz;
	#endif
`;

let installed = false;

/** Agrega la variante con cuaterniones duales a los shaders de three.js (una vez) */
export function installDqSkinning(): void {
  if (installed) return;
  installed = true;
  const chunks = THREE.ShaderChunk as unknown as Record<string, string>;
  const wrap = (original: string, dq: string) =>
    `#if defined( USE_SKINNING ) && defined( USE_DQS )\n${dq}\n#else\n${original}\n#endif\n`;
  chunks.skinning_pars_vertex = `${chunks.skinning_pars_vertex}\n${PARS}`;
  chunks.skinning_vertex = wrap(chunks.skinning_vertex, VERTEX);
  chunks.skinnormal_vertex = wrap(chunks.skinnormal_vertex, NORMAL);
}

/** Enciende o apaga los cuaterniones duales en un material (recompila si cambia) */
export function setDqSkinning(material: THREE.Material, on: boolean): void {
  const m = material as THREE.Material & { defines?: Record<string, unknown> };
  const has = !!m.defines && "USE_DQS" in m.defines;
  if (has === on) return;
  const defines = { ...(m.defines ?? {}) };
  if (on) defines.USE_DQS = "";
  else delete defines.USE_DQS;
  m.defines = defines;
  m.needsUpdate = true;
}
