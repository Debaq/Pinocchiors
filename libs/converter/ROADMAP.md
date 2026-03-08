# Converter — Roadmap

Conversor de formatos 3D en Rust puro, inspirado en `usd_from_gltf` de Google.

## Arquitectura

```
┌────────────┐     ┌─────────────────┐     ┌────────────────┐
│ glTF / GLB │────▶│                 │────▶│ GLB  (export)  │
└────────────┘     │                 │     └────────────────┘
┌────────────┐     │  converter-scene│     ┌────────────────┐
│    STL     │────▶│  (formato       │────▶│ STL  (export)  │
└────────────┘     │   pivote)       │     └────────────────┘
┌────────────┐     │                 │     ┌────────────────┐
│    OBJ     │────▶│                 │────▶│ OBJ  (export)  │
└────────────┘     └─────────────────┘     └────────────────┘
                                           ┌────────────────┐
                                           │ USDA (export)  │
                                           └────────────────┘
                                           ┌────────────────┐
                                           │ USDZ (package) │
                                           └────────────────┘
```

### Crates

| Crate | Responsabilidad |
|-------|-----------------|
| `converter-scene` | Representación intermedia: meshes, materiales PBR, texturas, esqueletos, animaciones, grafo de escena |
| `converter-gltf-io` | Lectura glTF/GLB → Scene, escritura Scene → GLB |
| `converter-usda` | Escritura Scene → USDA (texto), UsdSkel (esqueletos + animaciones), empaquetado USDZ |
| `converter-stl` | Lectura/escritura STL (ASCII y binario) |
| `converter-obj` | Lectura/escritura OBJ + MTL (materiales y texturas) |
| `converter-core` | Fachada unificada: `convert()`, `import()`, `export()`, detección automática de formato |
| `converter-wasm` | Bindings WASM con `wasm-bindgen` para uso desde JavaScript |

### Dependencias externas

- `gltf` — parsing glTF/GLB (5.4M descargas, maduro)
- `stl_io` — lectura/escritura STL (1.9M descargas, maduro)
- `tobj` — parsing OBJ/MTL (maduro)
- `glam` — vectores, matrices, quaterniones (ligero, alineado SIMD)
- `zip` — empaquetado USDZ
- `image` — procesamiento de texturas (resize, split canales, conversión formato)

Sin dependencias C/C++. Sin FFI. Todo Rust puro.

---

## Fase 1: Scene intermedia + STL (lo más simple primero) ✅

La base sobre la que se construye todo lo demás.

### 1.1 Completar `converter-scene` ✅
- [x] Estructura de datos `Scene` con nodos, meshes, materiales, texturas, esqueletos, animaciones
- [x] `Transform` TRS y matrix con conversión
- [x] `Material` PBR metallic-roughness completo
- [x] `Skeleton` y `Animation` con keyframes
- [x] Validación de escena (índices en rango, meshes no vacías)
- [x] `Scene::merge()` — combinar múltiples escenas en una
- [x] `Scene::compute_bounding_box()` — AABB global

### 1.2 Implementar `converter-stl` ✅
- [x] **Import STL**: leer con `stl_io`, crear `Scene` con un solo mesh (triángulos + normales)
- [x] **Export STL**: extraer triángulos de todas las primitivas, triangular quads si hay, escribir binario
- [x] Test roundtrip: STL → Scene → STL, comparar geometría
- [x] Manejar mallas con múltiples primitivas (fusionar en una)

---

## Fase 2: glTF/GLB — lectura completa ✅

Lectura de glTF/GLB convirtiendo a Scene intermedia.

### 2.1 Importar geometría ✅
- [x] Leer positions, normals, tangents, texcoords, vertex colors
- [x] Soportar índices u16 y u32
- [x] Manejar primitivas con diferentes modos (TRIANGLES, TRIANGLE_STRIP, TRIANGLE_FAN)
- [x] Mallas sin índices (non-indexed)

### 2.2 Importar materiales y texturas ✅
- [x] Material PBR metallic-roughness → `Material`
- [x] Leer texturas embebidas (data URIs, buffer views) y externas
- [x] Mapeo de sampler (wrap, filter) para referencia

### 2.3 Importar grafo de escena ✅
- [x] Jerarquía de nodos con transforms TRS y matrix
- [x] Múltiples escenas (seleccionar la default o la primera)

### 2.4 Importar esqueletos y animaciones ✅
- [x] Skins → `Skeleton` (joints, inverse bind matrices)
- [x] Animations → `Animation` (channels por nodo, interpolación)
- [x] Joint indices y weights por vértice

### 2.5 Extensiones glTF ✅
- [x] `KHR_materials_unlit` — mapear a emissive (import + USDA export)
- [x] `KHR_texture_transform` — bakear en UVs durante import
- [x] `KHR_draco_mesh_compression` — detección y error si es extensión requerida
- [x] `KHR_materials_pbrSpecularGlossiness` — conversión a metallic-roughness

---

## Fase 3: USDA — escritor puro en Rust ✅

Recrear la funcionalidad core de `usd_from_gltf` sin OpenUSD.

### 3.1 Writer USDA base ✅
- [x] Header con metersPerUnit, upAxis, defaultPrim
- [x] Jerarquía de Xforms con transforms (translate, orient quaternion, scale ops)
- [x] Escala raíz configurable (`UsdaExportOptions.scale_factor`) — ajusta metersPerUnit + xformOp:scale en Root
- [x] Serialización correcta de arrays USD (VtArray format)

### 3.2 Meshes ✅
- [x] `UsdGeomMesh` con points, faceVertexCounts, faceVertexIndices
- [x] Normals con interpolation (vertex)
- [x] Primvars para texcoords (`primvars:st`, `primvars:st1`)
- [x] Extent (bounding box) por mesh
- [x] SubdivisionScheme = "none" (polígonos, no subdiv)
- [x] DoubleSided attribute
- [x] Vertex colors como `primvars:displayColor` + `primvars:displayOpacity`

### 3.3 Materiales UsdPreviewSurface ✅
- [x] Shader graph: Material → UsdPreviewSurface → outputs:surface
- [x] Inputs escalares: diffuseColor, metallic, roughness, opacity
- [x] Inputs de textura: UsdUVTexture → UsdPrimvarReader_float2
- [x] Connections entre shaders (connect syntax en USDA)
- [x] Normal map con scale/bias ([0,1] → [-1,1])
- [x] Emissive color y textura
- [x] Occlusion (canal R de textura)
- [x] Metallic (canal B) y roughness (canal G) de textura combinada
- [x] MaterialBindingAPI para asignar materiales a meshes

### 3.4 Procesamiento de texturas ✅
- [x] Split de canales ORM: `split_orm_channels` separa occlusion(R)/roughness(G)/metallic(B) en texturas individuales PNG
- [x] Conversión de formatos (WebP → PNG automático, USD no soporta WebP)
- [x] Resize proporcional con Lanczos3 (`max_texture_size`)
- [x] Renombrar texturas con nombres únicos para USDZ
- [x] Texturas PNG/JPEG sin cambios se copian directamente (sin decode/encode)

### 3.5 Conversiones de coordenadas ✅
- [x] glTF Y-up → USD Y-up (sin cambio)
- [x] Escala configurable (`scale_factor`) — ajusta metersPerUnit y agrega xformOp:scale en Root
- [x] Detección de escala negativa → flip winding (swap índices 1,2 por triángulo, XOR acumulativo)

---

## Fase 4: UsdSkel — esqueletos y animaciones ✅

La parte más compleja. Basada en el módulo `convert/converter.cc` de Google.

### 4.1 Esqueletos ✅
- [x] `SkelRoot` como contenedor
- [x] `Skeleton` con joints array (paths relativos), bindTransforms, restTransforms
- [x] Joint ordering consistente (BFS desde raíz)
- [x] `Joint.node_index: Option<usize>` para mapear Channel.node → joint
- [x] `SkelContext` con mapeo node→(skel_idx, joint_idx) y datos preparados por skeleton

### 4.2 Skinning ✅
- [x] `SkelBindingAPI` en meshes skinned
- [x] Joint indices y weights primvars (`primvars:skel:jointIndices`, `primvars:skel:jointWeights`)
- [x] Relación skeleton → mesh vía `skel:skeleton` y `skel:animationSource`

### 4.3 Animaciones ✅
- [x] `SkelAnimation` con translations, rotations, scales por timeCode
- [x] Múltiples clips: cada animación glTF se exporta como `def SkelAnimation` separado
- [x] Conversión de tiempo: segundos (glTF) → timeCodes (USD, fps configurable, default 24)
- [x] Sampling Linear, Step, CubicSpline (hermite spline para vec3, slerp para quaterniones)
- [x] Baking de step → linear (duplicación de valores)
- [x] Baking de cubicSpline → linear (resampling)
- [x] Pruning de keyframes redundantes (dentro de tolerancia configurable)
- [x] Quaternion glam (x,y,z,w) → USD (w,x,y,z)

### 4.4 Animación rígida (transforms) ✅
- [x] Animación de Xform ops con timeSamples (nodos no-joint)
- [x] `NodeAnimSamples` con translation/rotation/scale samples por nodo
- [x] `build_node_anim_context()` filtra nodos joint del contexto de animación rígida
- [x] `startTimeCode` / `endTimeCode` en el header del stage
- [x] `UsdaExportOptions` extendido: `fps`, `export_animations`, `keyframe_tolerance`

---

## Fase 5: Empaquetado USDZ ✅

### 5.1 Packager ✅
- [x] Crear archivo ZIP sin compresión (stored)
- [x] Alineamiento a 64 bytes de cada entrada (requerido por spec USDZ)
- [x] Primera entrada: `scene.usda` (la escena principal)
- [x] Entradas adicionales: texturas PNG/JPEG
- [x] Validar que solo contiene tipos permitidos (.usda, .usdc, .png, .jpeg, .m4a, .mp3, .wav)
- [x] API en memoria: `write_usdz_bytes(scene, options)` para WASM y pipelines

### 5.2 Compatibilidad AR Quick Look (Apple) ✅
- [x] Verificar restricciones de Apple: un solo archivo raíz, texturas < 2048x2048
- [x] Modo "ARKit-compatible" con restricciones adicionales (`arkit_compatible: bool`)
- [x] Flag para forzar single UV set (iOS no soporta múltiples)

---

## Fase 6: Exportación GLB ✅

Escribir Scene → GLB binario para conversiones inversas y entre formatos.

### 6.1 Generador GLB ✅
- [x] `GlbBuilder` construye JSON glTF + buffer binario con `serde_json`
- [x] Buffer binario con accessors, buffer views, alineamiento a 4 bytes
- [x] Empaquetado GLB (header + JSON chunk + BIN chunk)

### 6.2 Soporte completo ✅
- [x] Meshes con todos los atributos (positions, normals, tangents, texcoords, colors, joints, weights)
- [x] Índices U16 (componentType 5123) y U32 (componentType 5125)
- [x] Materiales PBR (base_color, metallic, roughness, normal, occlusion, emissive)
- [x] Alpha modes: Opaque, Mask (con alphaCutoff), Blend
- [x] Texturas embebidas en el buffer BIN (PNG, JPEG, WebP)
- [x] Esqueletos (skins): joints como nodos, inverseBindMatrices
- [x] Animaciones: channels + samplers (translation, rotation, scale, weights)
- [x] Extensión KHR_materials_unlit
- [x] API pública: `export_glb(scene, path)`, `export_glb_bytes(scene)`

---

## Fase 7: Formatos adicionales

### 7.1 OBJ (Wavefront) ✅
- [x] Import con `tobj` (triangulate + single_index)
- [x] Import materiales desde .mtl (diffuse, shininess, dissolve, texturas)
- [x] Export OBJ (vértices, normales, UVs, faces con índices 1-based)
- [x] Export .mtl junto al .obj (Kd, Ks, Ns, d, map_Kd)
- [x] Export texturas como archivos PNG/JPEG al directorio de salida
- [x] API pública: `import_obj(path)`, `export_obj(scene, path)`

### 7.2 FBX (evaluar)
- [ ] Investigar `asset-importer` (bindings Assimp) vs implementación parcial
- [ ] Si FFI: lectura FBX → Scene
- [ ] Si nativo: solo FBX binario lectura (formato complejo, Autodesk propietario)

### 7.3 PLY
- [ ] Import/export geometría + vertex colors
- [ ] ASCII y binario

---

## Fase 8: CLI y API pública ✅

### 8.1 Crate `converter-core` (fachada) ✅
- [x] API unificada: `convert(input_path, output_path, options)`
- [x] Detección automática de formato por extensión (`Format::from_extension()`)
- [x] `ConvertOptions` unificado (escala, texturas, animaciones, fps, arkit, split ORM)
- [x] `import(path) → Scene`, `export(scene, path, options)`
- [x] `ConvertError` enum con variantes por cada crate
- [x] Re-export de `Scene` y todos los tipos públicos

### 8.2 CLI `apps/converter` ✅
- [x] Binario de línea de comandos con `clap` 4 (derive)
- [x] `converter input.glb output.usdz`
- [x] `converter input.stl output.glb`
- [x] Flags: `--scale`, `--max-texture-size`, `--no-animations`, `--arkit`, `--split-orm`, `--fps`
- [x] Batch mode: `converter --batch input.glb --format usdz`

### 8.3 WASM ✅
- [x] `wasm-bindgen` bindings para usar desde JavaScript
- [x] `convert(input_bytes, input_format, output_format, options_json?) → Vec<u8>`
- [x] `supported_formats() → String` (JSON con import/export)
- [x] `import_to_json(input_bytes, format) → String` (metadatos de escena)
- [x] `import_gltf_bytes(data)` en converter-gltf-io para import GLB desde memoria

---

## Referencia: Mapeo de funcionalidad vs `usd_from_gltf` de Google

| Funcionalidad Google | Fase | Estado | Notas |
|----------------------|------|--------|-------|
| Parser glTF/GLB propio (~7700 líneas) | 2 | ✅ | Reemplazado por crate `gltf` |
| Material PBR → UsdPreviewSurface | 3.3 | ✅ | Reimplementación directa |
| Texture channel splitting | 3.4 | ✅ | Con crate `image` (split ORM, WebP→PNG, resize) |
| Specular-glossiness → metallic-roughness | 2.5 | ✅ | Conversión aproximada en import |
| Skeleton → UsdSkel | 4.1-4.2 | ✅ | SkelRoot, Skeleton, SkelBindingAPI, joint paths BFS |
| Animation baking (slerp, step, cubic) | 4.3 | ✅ | Linear, Step, CubicSpline sampling + baking + pruning |
| Quaternion → Euler | 4.4 | ✅ | Quaternion directo (orient op), no Euler — compatible con USD |
| USDZ packaging (ARKit) | 5 | ✅ | Con crate `zip` v4, `with_alignment(64)`, modo ARKit |
| Double-sided emulation (geo duplication) | 3.2 | ❌ | Opcional, configurable |
| Draco decompression | 2.5 | ⚠️ | Detección + error; decodificación no implementada |
| **Morph targets / blend shapes** | **4+** | ❌ | **Google NO lo soporta — nosotros sí (objetivo)** |
| **Vertex colors** | **3.2** | ✅ | **Google los descarta — nosotros los exportamos** |
| **Múltiples animaciones** | **4.3** | ✅ | **Google solo exporta una — nosotros todas (clips SkelAnimation separados)** |
| **WASM** | **8.3** | ✅ | **Google no soporta — nosotros sí (wasm-bindgen)** |

---

## Prioridades

1. ~~**Fase 1-2**: Base funcional — poder leer cualquier glTF/GLB a Scene intermedia~~ ✅
2. ~~**Fase 3**: USDA writer — la pieza que no existe en Rust~~ ✅
3. ~~**Fase 6-7**: GLB export + OBJ import/export~~ ✅
4. ~~**Fase 5**: USDZ — el entregable principal (reemplaza workflow actual)~~ ✅
5. ~~**Fase 4**: Esqueletos — crítico para integración con pinocchio auto-rig~~ ✅
6. ~~**Fase 8**: CLI y WASM para reemplazar herramientas JavaScript actuales~~ ✅

## Tests

84 tests totales, todos pasando:
- `converter-scene`: 7 (validate, merge, bounding_box)
- `converter-stl`: 2 (import, roundtrip)
- `converter-gltf-io`: 19 (import: triangle, material, bounding box, base64, percent decode, unlit, draco error, spec-gloss ×2, texture transform ×2; export: minimal triangle, roundtrip triangle, roundtrip material, roundtrip normals/UVs, roundtrip texture, unlit, alpha modes, indices U16)
- `converter-obj`: 6 (import simple, import material, export triangle, export material, roundtrip, indices 1-based)
- `converter-usda`: 36 (writer: 13 — header, mesh, xform ×2, materiales, texturas, emissive, normal map, sanitize, unlit ×2, scale_factor, flip_winding; packager: 10 — basic, no_compression, alignment_64, with_textures, scene_content, write_to_file, arkit_limits, arkit_smaller_max, validate_allowed, validate_rejected; skeleton: 13 — bfs_ordering ×2, joint_paths ×2, bind_transforms, rest_transforms, skeleton_def_output, find_skeleton_anims, seconds_to_timecode, bake_step, prune ×2, time_range)
- `converter-core`: 14 (format_detection, format_extensions, format_enumeration, import_not_supported, export_stl, export_usda, export_usdz, export_import_glb_roundtrip, convert_glb_to_usdz, format_detection_error, export_import_glb_bytes_roundtrip, export_usdz_bytes, import_bytes_not_supported, export_bytes_not_supported)
