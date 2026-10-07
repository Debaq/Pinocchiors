/* Stub sin OpenCASCADE: misma interfaz, todo falla con un mensaje claro. */
#include "cad_occt.h"
#include <string.h>

static const char* MSG = "OpenCASCADE no disponible: esta compilación no incluye el CAD";
#define FAIL_PTR return NULL
#define FAIL_INT return 0

const char* cad_last_error(void) { return MSG; }
int32_t cad_available(void) { return 0; }
const char* cad_occt_version(void) { return ""; }
void cad_shape_free(CadShape* s) { (void)s; }
CadShape* cad_shape_clone(const CadShape* s) { (void)s; FAIL_PTR; }
int32_t cad_shape_kind(const CadShape* s) { (void)s; FAIL_INT; }
int32_t cad_shape_is_valid(const CadShape* s) { (void)s; FAIL_INT; }
CadShape* cad_make_face(const int32_t* k, const int32_t* c, const double* d, const int32_t* l, int32_t n) { (void)k; (void)c; (void)d; (void)l; (void)n; FAIL_PTR; }
CadShape* cad_make_wire(const int32_t* k, const int32_t* c, const double* d, int32_t n) { (void)k; (void)c; (void)d; (void)n; FAIL_PTR; }
CadShape* cad_make_box(const double* a, double x, double y, double z) { (void)a; (void)x; (void)y; (void)z; FAIL_PTR; }
CadShape* cad_make_cylinder(const double* a, double r, double h) { (void)a; (void)r; (void)h; FAIL_PTR; }
CadShape* cad_make_cone(const double* a, double r1, double r2, double h) { (void)a; (void)r1; (void)r2; (void)h; FAIL_PTR; }
CadShape* cad_make_sphere(const double* c, double r) { (void)c; (void)r; FAIL_PTR; }
CadShape* cad_make_torus(const double* a, double r1, double r2) { (void)a; (void)r1; (void)r2; FAIL_PTR; }
CadShape* cad_prism(const CadShape* p, double x, double y, double z) { (void)p; (void)x; (void)y; (void)z; FAIL_PTR; }
CadShape* cad_revol(const CadShape* p, const double* a, double t) { (void)p; (void)a; (void)t; FAIL_PTR; }
CadShape* cad_pipe(const CadShape* p, const CadShape* s) { (void)p; (void)s; FAIL_PTR; }
CadShape* cad_loft(const CadShape* const* w, int32_t n, int32_t s, int32_t r) { (void)w; (void)n; (void)s; (void)r; FAIL_PTR; }
CadShape* cad_boolean(const CadShape* a, const CadShape* b, int32_t op) { (void)a; (void)b; (void)op; FAIL_PTR; }
CadShape* cad_fuse_many(const CadShape* const* s, int32_t n) { (void)s; (void)n; FAIL_PTR; }
CadShape* cad_compound(const CadShape* const* s, int32_t n) { (void)s; (void)n; FAIL_PTR; }
CadShape* cad_fillet(const CadShape* s, const int32_t* e, int32_t n, double r) { (void)s; (void)e; (void)n; (void)r; FAIL_PTR; }
CadShape* cad_chamfer(const CadShape* s, const int32_t* e, int32_t n, double d) { (void)s; (void)e; (void)n; (void)d; FAIL_PTR; }
CadShape* cad_shell(const CadShape* s, const int32_t* f, int32_t n, double t) { (void)s; (void)f; (void)n; (void)t; FAIL_PTR; }
CadShape* cad_draft(const CadShape* s, const int32_t* f, int32_t n, const double* d, double a, const double* o, const double* nn) { (void)s; (void)f; (void)n; (void)d; (void)a; (void)o; (void)nn; FAIL_PTR; }
CadShape* cad_transform(const CadShape* s, const double* m) { (void)s; (void)m; FAIL_PTR; }
CadShape* cad_mirror(const CadShape* s, const double* o, const double* n) { (void)s; (void)o; (void)n; FAIL_PTR; }
CadShape* cad_split_keep(const CadShape* s, const double* o, const double* n) { (void)s; (void)o; (void)n; FAIL_PTR; }
CadShape* cad_from_mesh(const double* v, int32_t nv, const int32_t* t, int32_t nt, double tol) { (void)v; (void)nv; (void)t; (void)nt; (void)tol; FAIL_PTR; }
int32_t cad_count_faces(const CadShape* s) { (void)s; FAIL_INT; }
int32_t cad_count_edges(const CadShape* s) { (void)s; FAIL_INT; }
int32_t cad_face_info(const CadShape* s, int32_t i, CadFaceInfo* o) { (void)s; (void)i; (void)o; FAIL_INT; }
int32_t cad_edge_info(const CadShape* s, int32_t i, CadEdgeInfo* o) { (void)s; (void)i; (void)o; FAIL_INT; }
int32_t cad_edge_faces(const CadShape* s, int32_t e, int32_t* o) { (void)s; (void)e; (void)o; FAIL_INT; }
int32_t cad_closest_face(const CadShape* s, const double* p, const double* n, double c, double* d) { (void)s; (void)p; (void)n; (void)c; (void)d; return -1; }
int32_t cad_closest_edge(const CadShape* s, const double* p, const double* n, double c, double* d) { (void)s; (void)p; (void)n; (void)c; (void)d; return -1; }
int32_t cad_mass_info(const CadShape* s, CadMassInfo* o) { (void)s; (void)o; FAIL_INT; }
int32_t cad_tessellate(const CadShape* s, double l, double a, CadMesh* o) { (void)s; (void)l; (void)a; memset(o, 0, sizeof(*o)); FAIL_INT; }
void cad_mesh_free(CadMesh* m) { (void)m; }
int32_t cad_write_step(const CadShape* s, uint8_t** o, size_t* l) { (void)s; (void)o; (void)l; FAIL_INT; }
CadShape* cad_read_step(const uint8_t* d, size_t l) { (void)d; (void)l; FAIL_PTR; }
int32_t cad_write_brep(const CadShape* s, uint8_t** o, size_t* l) { (void)s; (void)o; (void)l; FAIL_INT; }
CadShape* cad_read_brep(const uint8_t* d, size_t l) { (void)d; (void)l; FAIL_PTR; }
void cad_bytes_free(uint8_t* p) { (void)p; }
int32_t cad_hlr(const CadShape* s, const double* e, const double* x, double d, uint8_t** o, size_t* l) { (void)s; (void)e; (void)x; (void)d; (void)o; (void)l; FAIL_INT; }
int32_t cad_take_history(CadHistory* o) { memset(o, 0, sizeof(*o)); return 0; }
void cad_history_free(CadHistory* h) { (void)h; }
int32_t cad_edge_face_pairs(const CadShape* s, int32_t* o) { (void)s; (void)o; return 0; }
CadShape* cad_sub_shape(const CadShape* s, int32_t k, int32_t i) { (void)s; (void)k; (void)i; FAIL_PTR; }
int32_t cad_count_vertices(const CadShape* s) { (void)s; FAIL_INT; }
int32_t cad_count_solids(const CadShape* s) { (void)s; FAIL_INT; }
double cad_ray_hit(const CadShape* s, const double* o, const double* d) { (void)s; (void)o; (void)d; return -1.0; }
int32_t cad_wire_sample(const CadShape* w, int32_t n, double* p, double* t) { (void)w; (void)n; (void)p; (void)t; FAIL_INT; }
CadShape* cad_make_helix(const double* o, const double* d, double r, double p, double t, int32_t l) { (void)o; (void)d; (void)r; (void)p; (void)t; (void)l; FAIL_PTR; }
CadShape* cad_thicken(const CadShape* f, double t) { (void)f; (void)t; FAIL_PTR; }
CadShape* cad_draft_prism(const CadShape* f, double h, double a) { (void)f; (void)h; (void)a; FAIL_PTR; }
CadShape* cad_offset_face(const CadShape* f, double d) { (void)f; (void)d; FAIL_PTR; }
int32_t cad_write_step_parts(const CadShape* const* s, const char* const* nm, const double* c, int32_t n, uint8_t** o, size_t* l) { (void)s; (void)nm; (void)c; (void)n; (void)o; (void)l; FAIL_INT; }
int32_t cad_face_indices_in(const CadShape* p, const CadShape* c, int32_t* o) { (void)p; (void)c; (void)o; FAIL_INT; }
int32_t cad_vertex_point(const CadShape* s, int32_t i, double* o) { (void)s; (void)i; (void)o; FAIL_INT; }
CadShape* cad_make_vertex(const double* p) { (void)p; FAIL_PTR; }
double cad_min_distance(const CadShape* a, const CadShape* b, double* pa, double* pb) { (void)a; (void)b; (void)pa; (void)pb; return -1.0; }
double cad_face_distance(const CadShape* s, int32_t i, const double* p) { (void)s; (void)i; (void)p; return -1.0; }
double cad_edge_distance(const CadShape* s, int32_t i, const double* p) { (void)s; (void)i; (void)p; return -1.0; }
