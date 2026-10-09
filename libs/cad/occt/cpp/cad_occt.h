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

/** Redondeo; con `radius2` distinto, variable del comienzo al final de cada arista */
CadShape* cad_fillet(const CadShape* s, const int32_t* edges, int32_t n, double radius, double radius2);
/** Chaflán: `mode` 0 igual, 1 dos distancias (`second`), 2 distancia y ángulo en grados (`second`) */
CadShape* cad_chamfer(const CadShape* s, const int32_t* edges, int32_t n, double distance, double second, int32_t mode,
                      int32_t flip);
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

// Cara más cercana a `point`. Si `normal` no es NULL, solo considera caras cuya
// normal en el punto más cercano forme con ella un coseno ≥ `min_cos`.
// Devuelve el índice (o -1) y escribe la distancia en `dist`.
int32_t cad_closest_face(const CadShape* s, const double* point, const double* normal,
                         double min_cos, double* dist);
// Arista más cercana; con `dir`, solo aristas cuya tangente cumpla |cos| ≥ `min_cos`.
int32_t cad_closest_edge(const CadShape* s, const double* point, const double* dir,
                         double min_cos, double* dist);

// Distancia de un punto a una cara / arista concreta (−1 si falla).
double cad_face_distance(const CadShape* s, int32_t index, const double* point);
double cad_edge_distance(const CadShape* s, int32_t index, const double* point);

// Sólidos de la forma (un sólido suelto, o los de un compuesto).
int32_t cad_count_solids(const CadShape* s);
// Para cada cara de `child` (que salió de `parent`), su índice en `parent` (−1 si no está).
int32_t cad_face_indices_in(const CadShape* parent, const CadShape* child, int32_t* out);
// Cara (kind 0), arista (1), vértice (2) o sólido (3) `index` de la forma, como forma propia.
CadShape* cad_sub_shape(const CadShape* s, int32_t kind, int32_t index);
int32_t cad_count_vertices(const CadShape* s);
// Coordenadas del vértice `index` (0 si no existe).
int32_t cad_vertex_point(const CadShape* s, int32_t index, double* out3);
// Vértice suelto en un punto.
CadShape* cad_make_vertex(const double* p);
// Distancia mínima entre dos formas y los puntos más cercanos de cada una (−1 si falla).
double cad_min_distance(const CadShape* a, const CadShape* b, double* pa, double* pb);

// STEP con varias piezas, cada una con su nombre (UTF-8) y color (r, g, b en 0..1
// por pieza; r < 0 = sin color).
int32_t cad_write_step_parts(const CadShape* const* shapes, const char* const* names, const double* colors,
                             int32_t n, uint8_t** out, size_t* len);

// Primer cruce de la semirrecta origen + t·dir (t > 0) con las caras de la forma; −1 si no hay.
double cad_ray_hit(const CadShape* s, const double* origin, const double* dir);
// Prisma de una cara plana hacia su normal, con las paredes inclinadas `angle` (rad;
// positivo = se angosta).
CadShape* cad_draft_prism(const CadShape* face, double height, double angle);
// Cara plana desplazada `distance` hacia afuera (negativo = hacia adentro), esquinas redondeadas.
CadShape* cad_offset_face(const CadShape* face, double distance);

// Hélice como alambre: eje (origen y dirección), radio, paso, vueltas; `left` = a izquierdas.
CadShape* cad_make_helix(const double* origin, const double* dir, double radius, double pitch, double turns,
                         int32_t left);
// Engrosa una cara (o un conjunto de caras) hasta un sólido de espesor `thickness`
// (hacia su normal; negativo = hacia atrás).
/** Macho roscado: núcleo de radio menor + filete ISO (60°) de `length` desde `origin` hacia `dir`;
 *  la cresta pasa por `origin + r·xdir` (la fase de la hélice) */
CadShape* cad_make_thread(const double* origin, const double* dir, const double* xdir, double r_minor, double r_major, double pitch,
                          double length, int32_t left, int32_t chamfer);
CadShape* cad_thicken(const CadShape* faces, double thickness);
/* Superficie que llena un borde cerrado de aristas (con `faces[i]` no nula,
   tangente a esa cara a lo largo de la arista i) y pasa por los puntos. */
CadShape* cad_fill(const CadShape* const* edges, const CadShape* const* faces, int32_t n, const double* points,
                   int32_t n_points);
/* Une superficies por sus bordes; con `solid`, cada concha cerrada se vuelve sólido. */
CadShape* cad_sew(const CadShape* const* shapes, int32_t n, double tolerance, int32_t solid);
/* Parte `s` con las herramientas (caras, superficies o sólidos): todos los pedazos. */
CadShape* cad_split_by(const CadShape* s, const CadShape* const* tools, int32_t n);

// `n` puntos a distancias iguales a lo largo de un alambre (o arista), con la
// tangente en cada uno: out_p y out_t tienen 3n valores.
int32_t cad_wire_sample(const CadShape* wire, int32_t n, double* out_p, double* out_t);

// Líneas de una vista (algoritmo exacto de líneas ocultas): `eye` apunta hacia quien
// mira y `xdir` es la derecha de la hoja. Salida en dobles: cantidad de líneas y, por
// línea, tipo (0 arista visible, 1 contorno visible, 2 arista oculta, 3 contorno oculto,
// 4 arista suave visible), cantidad de puntos y sus (x, y).
int32_t cad_hlr(const CadShape* s, const double* eye, const double* xdir, double deflection, uint8_t** out, size_t* len);

// Las dos caras de cada arista (−1 si falta): out tiene 2 × cad_count_edges.
int32_t cad_edge_face_pairs(const CadShape* s, int32_t* out);

typedef struct {
    double volume;
    double area;
    double center[3];
    double bbox_min[3];
    double bbox_max[3];
    // Momentos principales de inercia respecto al centro de masa, con densidad 1
    // (mm⁵ si las medidas son mm), y sus ejes (3 × 3, uno por fila).
    double inertia[3];
    double axes[9];
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

// ---------------------------------------------------------------------------
// Historia de la última operación del hilo (booleanas, unión múltiple,
// redondeo, chaflán, cáscara, desmolde, transformar, espejar, cortar). Entrada
// i: caras del resultado que salieron de la cara i de las entradas (contadas
// entrada por entrada, en el orden de TopExp) y, en redondeo/chaflán, después
// las caras que generó cada arista elegida. Se vacía al leerla.
// ---------------------------------------------------------------------------
typedef struct {
    int32_t* offsets;  // n + 1
    int32_t* faces;
    int32_t n;
} CadHistory;

int32_t cad_take_history(CadHistory* out);
void cad_history_free(CadHistory* h);

#ifdef __cplusplus
}
#endif
