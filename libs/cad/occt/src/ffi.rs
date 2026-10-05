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

unsafe extern "C" {
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
    pub fn cad_mass_info(s: *const CadShape, out: *mut CadMassInfo) -> i32;

    pub fn cad_tessellate(s: *const CadShape, linear: f64, angular: f64, out: *mut CadMesh) -> i32;
    pub fn cad_mesh_free(m: *mut CadMesh);

    pub fn cad_write_step(s: *const CadShape, out: *mut *mut u8, len: *mut usize) -> i32;
    pub fn cad_read_step(data: *const u8, len: usize) -> *mut CadShape;
    pub fn cad_write_brep(s: *const CadShape, out: *mut *mut u8, len: *mut usize) -> i32;
    pub fn cad_read_brep(data: *const u8, len: usize) -> *mut CadShape;
    pub fn cad_bytes_free(p: *mut u8);
}
