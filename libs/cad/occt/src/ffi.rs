//! Declaraciones crudas de `cpp/cad_occt.h`.

#![allow(non_camel_case_types)]

#[repr(C)]
pub struct CadShape {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct CadFaceInfo {
    pub surface: i32,
    pub area: f64,
    pub center: [f64; 3],
    pub normal: [f64; 3],
    pub point: [f64; 3],
    pub axis_origin: [f64; 3],
    pub axis_dir: [f64; 3],
    pub radius: f64,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct CadEdgeInfo {
    pub curve: i32,
    pub length: f64,
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub mid: [f64; 3],
    pub tangent: [f64; 3],
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub radius: f64,
    pub closed: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct CadMassInfo {
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
    pub inertia: [f64; 3],
    pub axes: [f64; 9],
}

#[repr(C)]
pub struct CadMesh {
    pub positions: *mut f64,
    pub normals: *mut f64,
    pub triangles: *mut u32,
    pub triangle_face: *mut i32,
    pub n_vertices: usize,
    pub n_triangles: usize,
    pub edge_points: *mut f64,
    pub edge_offsets: *mut usize,
    pub n_edges: usize,
}

#[repr(C)]
pub struct CadHistory {
    pub offsets: *mut i32,
    pub faces: *mut i32,
    pub n: i32,
}

unsafe extern "C" {
    pub fn cad_take_history(out: *mut CadHistory) -> i32;
    pub fn cad_history_free(h: *mut CadHistory);
    pub fn cad_last_error() -> *const std::ffi::c_char;
    pub fn cad_available() -> i32;
    pub fn cad_occt_version() -> *const std::ffi::c_char;

    pub fn cad_shape_free(s: *mut CadShape);
    pub fn cad_shape_clone(s: *const CadShape) -> *mut CadShape;
    pub fn cad_shape_kind(s: *const CadShape) -> i32;
    pub fn cad_shape_is_valid(s: *const CadShape) -> i32;

    pub fn cad_make_face(
        kinds: *const i32,
        counts: *const i32,
        data: *const f64,
        loop_sizes: *const i32,
        n_loops: i32,
    ) -> *mut CadShape;
    pub fn cad_make_wire(kinds: *const i32, counts: *const i32, data: *const f64, n: i32) -> *mut CadShape;

    pub fn cad_make_box(ax: *const f64, dx: f64, dy: f64, dz: f64) -> *mut CadShape;
    pub fn cad_make_cylinder(ax: *const f64, radius: f64, height: f64) -> *mut CadShape;
    pub fn cad_make_cone(ax: *const f64, r1: f64, r2: f64, height: f64) -> *mut CadShape;
    pub fn cad_make_sphere(center: *const f64, radius: f64) -> *mut CadShape;
    pub fn cad_make_torus(ax: *const f64, r1: f64, r2: f64) -> *mut CadShape;

    pub fn cad_prism(p: *const CadShape, dx: f64, dy: f64, dz: f64) -> *mut CadShape;
    pub fn cad_revol(p: *const CadShape, axis: *const f64, angle: f64) -> *mut CadShape;
    pub fn cad_pipe(p: *const CadShape, spine: *const CadShape) -> *mut CadShape;
    pub fn cad_loft(wires: *const *const CadShape, n: i32, solid: i32, ruled: i32) -> *mut CadShape;
    pub fn cad_boolean(a: *const CadShape, b: *const CadShape, op: i32) -> *mut CadShape;
    pub fn cad_fuse_many(shapes: *const *const CadShape, n: i32) -> *mut CadShape;
    pub fn cad_compound(shapes: *const *const CadShape, n: i32) -> *mut CadShape;
    pub fn cad_fillet(s: *const CadShape, edges: *const i32, n: i32, radius: f64) -> *mut CadShape;
    pub fn cad_chamfer(s: *const CadShape, edges: *const i32, n: i32, distance: f64) -> *mut CadShape;
    pub fn cad_shell(s: *const CadShape, faces: *const i32, n: i32, thickness: f64) -> *mut CadShape;
    pub fn cad_draft(
        s: *const CadShape,
        faces: *const i32,
        n: i32,
        dir: *const f64,
        angle: f64,
        neutral_origin: *const f64,
        neutral_normal: *const f64,
    ) -> *mut CadShape;
    pub fn cad_transform(s: *const CadShape, m12: *const f64) -> *mut CadShape;
    pub fn cad_mirror(s: *const CadShape, origin: *const f64, normal: *const f64) -> *mut CadShape;
    pub fn cad_split_keep(s: *const CadShape, origin: *const f64, normal: *const f64) -> *mut CadShape;
    pub fn cad_from_mesh(
        verts: *const f64,
        n_verts: i32,
        tris: *const i32,
        n_tris: i32,
        tolerance: f64,
    ) -> *mut CadShape;

    pub fn cad_count_faces(s: *const CadShape) -> i32;
    pub fn cad_count_edges(s: *const CadShape) -> i32;
    pub fn cad_face_info(s: *const CadShape, index: i32, out: *mut CadFaceInfo) -> i32;
    pub fn cad_edge_info(s: *const CadShape, index: i32, out: *mut CadEdgeInfo) -> i32;
    pub fn cad_edge_faces(s: *const CadShape, edge: i32, out2: *mut i32) -> i32;
    pub fn cad_closest_face(s: *const CadShape, point: *const f64, normal: *const f64, min_cos: f64, dist: *mut f64) -> i32;
    pub fn cad_closest_edge(s: *const CadShape, point: *const f64, dir: *const f64, min_cos: f64, dist: *mut f64) -> i32;
    pub fn cad_face_distance(s: *const CadShape, index: i32, point: *const f64) -> f64;
    pub fn cad_edge_distance(s: *const CadShape, index: i32, point: *const f64) -> f64;
    pub fn cad_edge_face_pairs(s: *const CadShape, out: *mut i32) -> i32;
    pub fn cad_sub_shape(s: *const CadShape, kind: i32, index: i32) -> *mut CadShape;
    pub fn cad_count_vertices(s: *const CadShape) -> i32;
    pub fn cad_count_solids(s: *const CadShape) -> i32;
    pub fn cad_ray_hit(s: *const CadShape, origin: *const f64, dir: *const f64) -> f64;
    pub fn cad_wire_sample(wire: *const CadShape, n: i32, out_p: *mut f64, out_t: *mut f64) -> i32;
    pub fn cad_make_helix(origin: *const f64, dir: *const f64, radius: f64, pitch: f64, turns: f64, left: i32) -> *mut CadShape;
    pub fn cad_make_thread(origin: *const f64, dir: *const f64, r_minor: f64, r_major: f64, pitch: f64, length: f64, left: i32) -> *mut CadShape;
    pub fn cad_thicken(faces: *const CadShape, thickness: f64) -> *mut CadShape;
    pub fn cad_draft_prism(face: *const CadShape, height: f64, angle: f64) -> *mut CadShape;
    pub fn cad_offset_face(face: *const CadShape, distance: f64) -> *mut CadShape;
    pub fn cad_write_step_parts(
        shapes: *const *const CadShape,
        names: *const *const std::ffi::c_char,
        colors: *const f64,
        n: i32,
        out: *mut *mut u8,
        len: *mut usize,
    ) -> i32;
    pub fn cad_face_indices_in(parent: *const CadShape, child: *const CadShape, out: *mut i32) -> i32;
    pub fn cad_vertex_point(s: *const CadShape, index: i32, out: *mut f64) -> i32;
    pub fn cad_make_vertex(p: *const f64) -> *mut CadShape;
    pub fn cad_min_distance(a: *const CadShape, b: *const CadShape, pa: *mut f64, pb: *mut f64) -> f64;
    pub fn cad_mass_info(s: *const CadShape, out: *mut CadMassInfo) -> i32;

    pub fn cad_tessellate(s: *const CadShape, linear: f64, angular: f64, out: *mut CadMesh) -> i32;
    pub fn cad_mesh_free(m: *mut CadMesh);

    pub fn cad_write_step(s: *const CadShape, out: *mut *mut u8, len: *mut usize) -> i32;
    pub fn cad_read_step(data: *const u8, len: usize) -> *mut CadShape;
    pub fn cad_write_brep(s: *const CadShape, out: *mut *mut u8, len: *mut usize) -> i32;
    pub fn cad_read_brep(data: *const u8, len: usize) -> *mut CadShape;
    pub fn cad_bytes_free(p: *mut u8);
    pub fn cad_hlr(s: *const CadShape, eye: *const f64, xdir: *const f64, deflection: f64, out: *mut *mut u8, len: *mut usize) -> i32;
}
