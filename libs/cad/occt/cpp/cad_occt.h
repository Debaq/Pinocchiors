// Puente C ↔ OpenCASCADE para cad-occt.
//
// Convenciones:
// - Toda función que crea una forma devuelve un CadShape* nuevo (liberar con
//   cad_shape_free) o NULL si falló; el motivo queda en cad_last_error().
// - Las funciones nunca dejan escapar excepciones de C++.
// - Índices de caras/aristas: 0-based, en el orden de TopExp::MapShapes (estable
//   para una misma forma, NO entre recálculos: por eso el modelo guarda referencias
//   geométricas y las resuelve contra estos índices).
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CadShape CadShape;

// Último error del hilo actual ("" si no hubo).
const char* cad_last_error(void);
// 1 si el puente se compiló con OpenCASCADE, 0 si es el stub.
int32_t cad_available(void);
// Versión de OCCT ("" en el stub).
const char* cad_occt_version(void);

void cad_shape_free(CadShape* s);
CadShape* cad_shape_clone(const CadShape* s);
// 0 nulo, 1 compound, 2 compsolid, 3 solid, 4 shell, 5 face, 6 wire, 7 edge, 8 vertex.
int32_t cad_shape_kind(const CadShape* s);
int32_t cad_shape_is_valid(const CadShape* s);

// ---------------------------------------------------------------------------
// Curvas de perfil
//
// Una curva se codifica como (tipo, cantidad de doubles, doubles):
//   0 línea   : x0 y0 z0  x1 y1 z1
//   1 arco    : inicio(3) punto medio(3) fin(3)
//   2 círculo : centro(3) normal(3) radio
//   3 spline  : n puntos de paso (3n doubles), interpolada
// Un lazo es una secuencia de curvas encadenadas; `loop_sizes` dice cuántas curvas
// tiene cada lazo.
// ---------------------------------------------------------------------------

// Cara plana: el primer lazo es el borde exterior y los demás, agujeros.
CadShape* cad_make_face(const int32_t* kinds, const int32_t* counts, const double* data,
                        const int32_t* loop_sizes, int32_t n_loops);
// Alambre (abierto o cerrado) de un solo lazo: trayectorias de barrido, loft.
CadShape* cad_make_wire(const int32_t* kinds, const int32_t* counts, const double* data,
                        int32_t n_curves);

// ---------------------------------------------------------------------------
// Primitivas. `ax` = origen(3) + dirección Z(3) + dirección X(3).
// ---------------------------------------------------------------------------
CadShape* cad_make_box(const double* ax, double dx, double dy, double dz);
CadShape* cad_make_cylinder(const double* ax, double radius, double height);
CadShape* cad_make_cone(const double* ax, double r1, double r2, double height);
CadShape* cad_make_sphere(const double* center, double radius);
CadShape* cad_make_torus(const double* ax, double r1, double r2);

// ---------------------------------------------------------------------------
// Operaciones
// ---------------------------------------------------------------------------
CadShape* cad_prism(const CadShape* profile, double dx, double dy, double dz);
// Eje: origen(3) + dirección(3). Ángulo en radianes.
CadShape* cad_revol(const CadShape* profile, const double* axis, double angle);
// Barrido de un perfil (cara o alambre) a lo largo de un alambre.
CadShape* cad_pipe(const CadShape* profile, const CadShape* spine);
// Loft entre alambres cerrados.
CadShape* cad_loft(const CadShape* const* wires, int32_t n, int32_t solid, int32_t ruled);

// op: 0 unión, 1 resta (a − b), 2 intersección.
CadShape* cad_boolean(const CadShape* a, const CadShape* b, int32_t op);
// Unión de muchas formas a la vez (patrones): más rápido y robusto que en cadena.
CadShape* cad_fuse_many(const CadShape* const* shapes, int32_t n);
CadShape* cad_compound(const CadShape* const* shapes, int32_t n);

CadShape* cad_fillet(const CadShape* s, const int32_t* edges, int32_t n, double radius);
CadShape* cad_chamfer(const CadShape* s, const int32_t* edges, int32_t n, double distance);
// Ahueca el sólido quitando las caras indicadas; grosor negativo = hacia adentro.
CadShape* cad_shell(const CadShape* s, const int32_t* faces, int32_t n, double thickness);
// Ángulo de desmolde (rad) a las caras indicadas respecto de `dir`, con plano neutro.
CadShape* cad_draft(const CadShape* s, const int32_t* faces, int32_t n, const double* dir,
                    double angle, const double* neutral_origin, const double* neutral_normal);

// Matriz 3×4 por filas (rotación+escala | traslación).
CadShape* cad_transform(const CadShape* s, const double* m12);
CadShape* cad_mirror(const CadShape* s, const double* origin, const double* normal);
// Conserva la parte del lado al que apunta la normal del plano.
CadShape* cad_split_keep(const CadShape* s, const double* origin, const double* normal);

// Sólido a partir de una malla triangular (cosido). Sirve para mezclar escaneos con CAD.
CadShape* cad_from_mesh(const double* verts, int32_t n_verts, const int32_t* tris,
                        int32_t n_tris, double tolerance);

// ---------------------------------------------------------------------------
// Topología
// ---------------------------------------------------------------------------
int32_t cad_count_faces(const CadShape* s);
int32_t cad_count_edges(const CadShape* s);

typedef struct {
    // 0 plano, 1 cilindro, 2 cono, 3 esfera, 4 toro, 5 b-spline/bezier, 6 revolución,
    // 7 extrusión, 8 offset, 9 otra
    int32_t surface;
    double area;
    double center[3];  // centro de masa de la cara
    double normal[3];  // normal saliente en el punto de la cara más cercano al centro
    double point[3];   // ese punto (sobre la cara)
    double axis_origin[3];  // cilindro/cono/esfera/toro: eje (esfera: centro)
    double axis_dir[3];
    double radius;      // cilindro/esfera/toro (mayor)/cono (en el origen)
} CadFaceInfo;

typedef struct {
    // 0 recta, 1 círculo, 2 elipse, 3 b-spline/bezier, 4 otra
    int32_t curve;
    double length;
    double start[3];
    double end[3];
    double mid[3];
    double tangent[3];  // en el punto medio
    double center[3];   // círculos
    double axis[3];
    double radius;
    int32_t closed;
} CadEdgeInfo;

int32_t cad_face_info(const CadShape* s, int32_t index, CadFaceInfo* out);
int32_t cad_edge_info(const CadShape* s, int32_t index, CadEdgeInfo* out);
// Caras adyacentes a una arista (hasta 2); devuelve cuántas escribió.
int32_t cad_edge_faces(const CadShape* s, int32_t edge, int32_t* out2);

typedef struct {
    double volume;
    double area;
    double center[3];
    double bbox_min[3];
    double bbox_max[3];
} CadMassInfo;

int32_t cad_mass_info(const CadShape* s, CadMassInfo* out);

// ---------------------------------------------------------------------------
// Teselado
// ---------------------------------------------------------------------------
typedef struct {
    double* positions;       // 3 por vértice
    double* normals;         // 3 por vértice
    uint32_t* triangles;     // 3 por triángulo
    int32_t* triangle_face;  // índice de cara por triángulo
    size_t n_vertices;
    size_t n_triangles;
    double* edge_points;     // polilíneas de aristas, 3 por punto
    size_t* edge_offsets;    // n_edges + 1 offsets (en puntos)
    size_t n_edges;          // = cad_count_edges
} CadMesh;

// deflexión lineal (mm) y angular (rad). Devuelve 1 si salió bien.
int32_t cad_tessellate(const CadShape* s, double linear, double angular, CadMesh* out);
void cad_mesh_free(CadMesh* m);

// ---------------------------------------------------------------------------
// Archivos. Los buffers devueltos se liberan con cad_bytes_free.
// ---------------------------------------------------------------------------
int32_t cad_write_step(const CadShape* s, uint8_t** out, size_t* len);
CadShape* cad_read_step(const uint8_t* data, size_t len);
int32_t cad_write_brep(const CadShape* s, uint8_t** out, size_t* len);
CadShape* cad_read_brep(const uint8_t* data, size_t len);
void cad_bytes_free(uint8_t* p);

#ifdef __cplusplus
}
#endif
