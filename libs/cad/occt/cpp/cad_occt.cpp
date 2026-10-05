// Implementación del puente con OpenCASCADE. Ver cad_occt.h.

#define _USE_MATH_DEFINES  // M_PI en MSVC
#include "cad_occt.h"

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_PrinterOStream.hxx>
#include <Poly.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
#include <Standard_Failure.hxx>
#include <Standard_Version.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

#include <cmath>
#include <cstdlib>
#include <cstring>
#include <map>
#include <mutex>
#include <sstream>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

struct CadShape {
    TopoDS_Shape s;
};

namespace {

thread_local std::string g_error;
// Historia de la última operación: para cada cara de las entradas (en orden,
// entrada por entrada) y luego cada arista elegida (redondeo/chaflán), las
// caras del resultado que salieron de ella. Ver cad_take_history.
thread_local std::vector<std::vector<int32_t>> g_history;

void set_error(const std::string& msg) { g_error = msg; }

// Ejecuta `f` atrapando cualquier excepción; devuelve `fallback` si falla.
template <typename F, typename R>
R guard(const char* what, R fallback, F f) {
    g_error.clear();
    g_history.clear();
    try {
        return f();
    } catch (const Standard_Failure& e) {
        const char* m = e.GetMessageString();
        set_error(std::string(what) + ": " + e.DynamicType()->Name() +
                  (m && *m ? std::string(" — ") + m : std::string()));
    } catch (const std::exception& e) {
        set_error(std::string(what) + ": " + e.what());
    } catch (...) {
        set_error(std::string(what) + ": excepción desconocida");
    }
    return fallback;
}

// Para operaciones que OCCT puede "completar" con una forma rota (redondeos
// imposibles, booleanas degeneradas): mejor un error claro que un sólido inválido.
CadShape* wrap_checked(const TopoDS_Shape& s, const char* what) {
    if (!s.IsNull() && !BRepCheck_Analyzer(s).IsValid())
        throw Standard_Failure((std::string(what) + ": el resultado no es un sólido válido").c_str());
    return s.IsNull() ? nullptr : new CadShape{s};
}

CadShape* wrap(const TopoDS_Shape& s) {
    if (s.IsNull()) {
        if (g_error.empty()) set_error("resultado vacío");
        return nullptr;
    }
    return new CadShape{s};
}

gp_Pnt pnt(const double* p) { return gp_Pnt(p[0], p[1], p[2]); }
gp_Dir dir(const double* d) { return gp_Dir(d[0], d[1], d[2]); }
void put(double* out, const gp_XYZ& v) {
    out[0] = v.X();
    out[1] = v.Y();
    out[2] = v.Z();
}

gp_Ax2 axis2(const double* ax) {
    gp_Pnt o = pnt(ax);
    gp_Dir z = dir(ax + 3);
    gp_Vec x(ax[6], ax[7], ax[8]);
    if (x.Magnitude() > 1e-12 && !gp_Dir(x).IsParallel(z, 1e-9)) return gp_Ax2(o, z, gp_Dir(x));
    return gp_Ax2(o, z);
}

// Construye la arista de una curva codificada. `d` apunta a sus doubles.
TopoDS_Edge make_edge(int32_t kind, int32_t count, const double* d) {
    switch (kind) {
        case 0: {
            if (count != 6) throw Standard_Failure("línea: se esperaban 6 valores");
            gp_Pnt a = pnt(d), b = pnt(d + 3);
            if (a.Distance(b) < Precision::Confusion()) throw Standard_Failure("línea de largo cero");
            return BRepBuilderAPI_MakeEdge(a, b).Edge();
        }
        case 1: {
            if (count != 9) throw Standard_Failure("arco: se esperaban 9 valores");
            GC_MakeArcOfCircle arc(pnt(d), pnt(d + 3), pnt(d + 6));
            if (!arc.IsDone()) throw Standard_Failure("arco: puntos alineados o repetidos");
            return BRepBuilderAPI_MakeEdge(arc.Value()).Edge();
        }
        case 2: {
            if (count != 7) throw Standard_Failure("círculo: se esperaban 7 valores");
            if (d[6] <= 0) throw Standard_Failure("círculo: radio no positivo");
            gp_Circ c(gp_Ax2(pnt(d), dir(d + 3)), d[6]);
            return BRepBuilderAPI_MakeEdge(c).Edge();
        }
        case 3: {
            if (count < 6 || count % 3 != 0) throw Standard_Failure("spline: puntos incompletos");
            int n = count / 3;
            bool periodic = n > 2 && pnt(d).Distance(pnt(d + 3 * (n - 1))) < Precision::Confusion();
            if (periodic) n -= 1;
            Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, n);
            for (int i = 0; i < n; i++) pts->SetValue(i + 1, pnt(d + 3 * i));
            GeomAPI_Interpolate interp(pts, periodic, Precision::Confusion());
            interp.Perform();
            if (!interp.IsDone()) throw Standard_Failure("spline: no se pudo interpolar");
            return BRepBuilderAPI_MakeEdge(interp.Curve()).Edge();
        }
        default:
            throw Standard_Failure("tipo de curva desconocido");
    }
}

// Arma un alambre con `n` curvas a partir de la curva `first`; avanza `offset`.
TopoDS_Wire make_wire_from(const int32_t* kinds, const int32_t* counts, const double* data,
                           int32_t first, int32_t n, size_t& offset) {
    BRepBuilderAPI_MakeWire mw;
    for (int32_t i = first; i < first + n; i++) {
        TopoDS_Edge e = make_edge(kinds[i], counts[i], data + offset);
        offset += counts[i];
        mw.Add(e);
        if (!mw.IsDone()) throw Standard_Failure("las curvas del lazo no están conectadas");
    }
    return mw.Wire();
}

double face_area(const TopoDS_Face& f) {
    GProp_GProps p;
    BRepGProp::SurfaceProperties(f, p);
    return p.Mass();
}

TopTools_IndexedMapOfShape map_of(const TopoDS_Shape& s, TopAbs_ShapeEnum t) {
    TopTools_IndexedMapOfShape m;
    TopExp::MapShapes(s, t, m);
    return m;
}

// Anota en g_history qué caras de `inputs` terminaron en cuáles de `result`
// (y, si se pasan, qué caras generó cada arista de `generators`).
void record(BRepBuilderAPI_MakeShape& mk, const std::vector<TopoDS_Shape>& inputs, const TopoDS_Shape& result,
            const std::vector<TopoDS_Shape>& generators = {}) {
    g_history.clear();
    TopTools_IndexedMapOfShape out;
    TopExp::MapShapes(result, TopAbs_FACE, out);
    auto images = [&](const TopoDS_Shape& f, bool generated) {
        std::vector<int32_t> v;
        const TopTools_ListOfShape& list = generated ? mk.Generated(f) : mk.Modified(f);
        for (const TopoDS_Shape& g : list) {
            int i = out.FindIndex(g);
            if (i > 0) v.push_back(i - 1);
        }
        if (!generated && v.empty() && !mk.IsDeleted(f)) {
            int i = out.FindIndex(f);
            if (i > 0) v.push_back(i - 1);
        }
        return v;
    };
    for (const TopoDS_Shape& in : inputs) {
        TopTools_IndexedMapOfShape faces;
        TopExp::MapShapes(in, TopAbs_FACE, faces);
        for (int i = 1; i <= faces.Extent(); i++) g_history.push_back(images(faces(i), false));
    }
    for (const TopoDS_Shape& e : generators) g_history.push_back(images(e, true));
}

TopoDS_Shape boolean_op(const TopoDS_Shape& a, const TopoDS_Shape& b, int32_t op) {
    auto finish = [&](BRepAlgoAPI_BooleanOperation& algo) {
        algo.Build();
        if (algo.HasErrors() || !algo.IsDone()) throw Standard_Failure("la operación booleana falló");
        algo.SimplifyResult();
        TopoDS_Shape r = algo.Shape();
        record(algo, {a, b}, r);
        return r;
    };
    if (op == 0) {
        BRepAlgoAPI_Fuse f(a, b);
        return finish(f);
    }
    if (op == 1) {
        BRepAlgoAPI_Cut c(a, b);
        return finish(c);
    }
    if (op == 2) {
        BRepAlgoAPI_Common c(a, b);
        return finish(c);
    }
    throw Standard_Failure("operación booleana desconocida");
}

// Punto de la cara cercano a su centro de masa y la normal saliente ahí.
void face_point_normal(const TopoDS_Face& face, const gp_Pnt& center, gp_Pnt& p, gp_Vec& n) {
    BRepGProp_Face gf(face);
    Handle(Geom_Surface) surf = BRep_Tool::Surface(face);
    double u = 0, v = 0;
    bool ok = false;
    GeomAPI_ProjectPointOnSurf proj(center, surf);
    if (proj.NbPoints() > 0) {
        proj.LowerDistanceParameters(u, v);
        BRepClass_FaceClassifier fc(face, gp_Pnt2d(u, v), Precision::Confusion());
        ok = fc.State() == TopAbs_IN || fc.State() == TopAbs_ON;
    }
    if (!ok) {
        // El centro cae fuera (anillos, caras en L): buscar en una grilla UV el punto
        // interior más cercano al centro.
        double u0, u1, v0, v1;
        BRepTools::UVBounds(face, u0, u1, v0, v1);
        double best = 1e300;
        const int N = 16;
        for (int i = 0; i <= N; i++) {
            for (int j = 0; j <= N; j++) {
                double uu = u0 + (u1 - u0) * (i + 0.5) / (N + 1);
                double vv = v0 + (v1 - v0) * (j + 0.5) / (N + 1);
                BRepClass_FaceClassifier fc(face, gp_Pnt2d(uu, vv), Precision::Confusion());
                if (fc.State() != TopAbs_IN) continue;
                double dist = surf->Value(uu, vv).Distance(center);
                if (dist < best) {
                    best = dist;
                    u = uu;
                    v = vv;
                    ok = true;
                }
            }
        }
        if (!ok) {
            u = (u0 + u1) / 2;
            v = (v0 + v1) / 2;
        }
    }
    gf.Normal(u, v, p, n);
    if (n.Magnitude() > 1e-12) n.Normalize();
}

}  // namespace

extern "C" {

const char* cad_last_error(void) { return g_error.c_str(); }
int32_t cad_available(void) { return 1; }
const char* cad_occt_version(void) { return OCC_VERSION_COMPLETE; }

void cad_shape_free(CadShape* s) { delete s; }

CadShape* cad_shape_clone(const CadShape* s) { return s ? new CadShape{s->s} : nullptr; }

int32_t cad_shape_kind(const CadShape* s) {
    if (!s || s->s.IsNull()) return 0;
    return static_cast<int32_t>(s->s.ShapeType()) + 1;
}

int32_t cad_shape_is_valid(const CadShape* s) {
    return guard("validar", 0, [&] { return BRepCheck_Analyzer(s->s).IsValid() ? 1 : 0; });
}

// --- Perfiles ---------------------------------------------------------------

CadShape* cad_make_face(const int32_t* kinds, const int32_t* counts, const double* data,
                        const int32_t* loop_sizes, int32_t n_loops) {
    return guard("cara", (CadShape*)nullptr, [&]() -> CadShape* {
        if (n_loops < 1) throw Standard_Failure("sin lazos");
        size_t offset = 0;
        int32_t first = 0;
        TopoDS_Wire outer = make_wire_from(kinds, counts, data, first, loop_sizes[0], offset);
        first += loop_sizes[0];
        if (!outer.Closed() && !BRep_Tool::IsClosed(outer))
            throw Standard_Failure("el borde exterior no está cerrado");
        BRepBuilderAPI_MakeFace mf(outer, Standard_True);
        if (!mf.IsDone()) throw Standard_Failure("el borde exterior no es plano");
        TopoDS_Face face = mf.Face();
        double area = face_area(face);
        for (int32_t l = 1; l < n_loops; l++) {
            TopoDS_Wire hole = make_wire_from(kinds, counts, data, first, loop_sizes[l], offset);
            first += loop_sizes[l];
            // Los agujeros llegan con cualquier sentido: si el área crece al
            // agregarlo, está al revés.
            TopoDS_Face with = BRepBuilderAPI_MakeFace(face, hole).Face();
            double a = face_area(with);
            if (a > area) {
                with = BRepBuilderAPI_MakeFace(face, TopoDS::Wire(hole.Reversed())).Face();
                a = face_area(with);
            }
            if (a > area || a <= 0) throw Standard_Failure("agujero fuera del borde o superpuesto");
            face = with;
            area = a;
        }
        ShapeFix_Face fix(face);
        fix.Perform();
        return wrap(fix.Face());
    });
}

CadShape* cad_make_wire(const int32_t* kinds, const int32_t* counts, const double* data,
                        int32_t n_curves) {
    return guard("alambre", (CadShape*)nullptr, [&] {
        size_t offset = 0;
        return wrap(make_wire_from(kinds, counts, data, 0, n_curves, offset));
    });
}

// --- Primitivas -------------------------------------------------------------

CadShape* cad_make_box(const double* ax, double dx, double dy, double dz) {
    return guard("caja", (CadShape*)nullptr,
                 [&] { return wrap(BRepPrimAPI_MakeBox(axis2(ax), dx, dy, dz).Shape()); });
}

CadShape* cad_make_cylinder(const double* ax, double radius, double height) {
    return guard("cilindro", (CadShape*)nullptr,
                 [&] { return wrap(BRepPrimAPI_MakeCylinder(axis2(ax), radius, height).Shape()); });
}

CadShape* cad_make_cone(const double* ax, double r1, double r2, double height) {
    return guard("cono", (CadShape*)nullptr,
                 [&] { return wrap(BRepPrimAPI_MakeCone(axis2(ax), r1, r2, height).Shape()); });
}

CadShape* cad_make_sphere(const double* center, double radius) {
    return guard("esfera", (CadShape*)nullptr,
                 [&] { return wrap(BRepPrimAPI_MakeSphere(pnt(center), radius).Shape()); });
}

CadShape* cad_make_torus(const double* ax, double r1, double r2) {
    return guard("toro", (CadShape*)nullptr,
                 [&] { return wrap(BRepPrimAPI_MakeTorus(axis2(ax), r1, r2).Shape()); });
}

// --- Operaciones ------------------------------------------------------------

CadShape* cad_prism(const CadShape* profile, double dx, double dy, double dz) {
    return guard("extruir", (CadShape*)nullptr, [&] {
        gp_Vec v(dx, dy, dz);
        if (v.Magnitude() < Precision::Confusion()) throw Standard_Failure("distancia cero");
        BRepPrimAPI_MakePrism mk(profile->s, v);
        if (!mk.IsDone()) throw Standard_Failure("no se pudo extruir");
        return wrap(mk.Shape());
    });
}

CadShape* cad_revol(const CadShape* profile, const double* axis, double angle) {
    return guard("revolucionar", (CadShape*)nullptr, [&] {
        gp_Ax1 ax(pnt(axis), dir(axis + 3));
        if (std::fabs(angle) >= 2 * M_PI - 1e-9) {
            BRepPrimAPI_MakeRevol mk(profile->s, ax);
            return wrap(mk.Shape());
        }
        BRepPrimAPI_MakeRevol mk(profile->s, ax, angle);
        if (!mk.IsDone()) throw Standard_Failure("no se pudo revolucionar");
        return wrap(mk.Shape());
    });
}

CadShape* cad_pipe(const CadShape* profile, const CadShape* spine) {
    return guard("barrer", (CadShape*)nullptr, [&] {
        TopoDS_Wire w;
        if (spine->s.ShapeType() == TopAbs_WIRE) {
            w = TopoDS::Wire(spine->s);
        } else if (spine->s.ShapeType() == TopAbs_EDGE) {
            w = BRepBuilderAPI_MakeWire(TopoDS::Edge(spine->s)).Wire();
        } else {
            throw Standard_Failure("la trayectoria debe ser un alambre");
        }
        BRepOffsetAPI_MakePipe mk(w, profile->s);
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo barrer");
        return wrap(mk.Shape());
    });
}

CadShape* cad_loft(const CadShape* const* wires, int32_t n, int32_t solid, int32_t ruled) {
    return guard("loft", (CadShape*)nullptr, [&] {
        if (n < 2) throw Standard_Failure("se necesitan al menos 2 perfiles");
        BRepOffsetAPI_ThruSections mk(solid != 0, ruled != 0);
        for (int32_t i = 0; i < n; i++) {
            const TopoDS_Shape& s = wires[i]->s;
            if (s.ShapeType() == TopAbs_WIRE) {
                mk.AddWire(TopoDS::Wire(s));
            } else if (s.ShapeType() == TopAbs_FACE) {
                mk.AddWire(BRepTools::OuterWire(TopoDS::Face(s)));
            } else {
                throw Standard_Failure("cada perfil debe ser un alambre o una cara");
            }
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo hacer el loft");
        return wrap(mk.Shape());
    });
}

CadShape* cad_boolean(const CadShape* a, const CadShape* b, int32_t op) {
    return guard("booleana", (CadShape*)nullptr,
                 [&] { return wrap_checked(boolean_op(a->s, b->s, op), "booleana"); });
}

CadShape* cad_fuse_many(const CadShape* const* shapes, int32_t n) {
    return guard("unir", (CadShape*)nullptr, [&] {
        if (n < 1) throw Standard_Failure("nada que unir");
        if (n == 1) return wrap(shapes[0]->s);
        TopTools_ListOfShape args, tools;
        args.Append(shapes[0]->s);
        for (int32_t i = 1; i < n; i++) tools.Append(shapes[i]->s);
        BRepAlgoAPI_Fuse f;
        f.SetArguments(args);
        f.SetTools(tools);
        f.Build();
        if (f.HasErrors() || !f.IsDone()) throw Standard_Failure("la unión falló");
        f.SimplifyResult();
        std::vector<TopoDS_Shape> inputs;
        for (int32_t i = 0; i < n; i++) inputs.push_back(shapes[i]->s);
        record(f, inputs, f.Shape());
        return wrap(f.Shape());
    });
}

CadShape* cad_compound(const CadShape* const* shapes, int32_t n) {
    return guard("compuesto", (CadShape*)nullptr, [&] {
        BRep_Builder b;
        TopoDS_Compound c;
        b.MakeCompound(c);
        for (int32_t i = 0; i < n; i++) b.Add(c, shapes[i]->s);
        return wrap(c);
    });
}

CadShape* cad_fillet(const CadShape* s, const int32_t* edges, int32_t n, double radius) {
    return guard("redondeo", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        BRepFilletAPI_MakeFillet mk(s->s);
        std::vector<TopoDS_Shape> chosen;
        for (int32_t i = 0; i < n; i++) {
            if (edges[i] < 0 || edges[i] >= m.Extent()) throw Standard_Failure("arista inexistente");
            mk.Add(radius, TopoDS::Edge(m(edges[i] + 1)));
            chosen.push_back(m(edges[i] + 1));
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("radio demasiado grande para esas aristas");
        record(mk, {s->s}, mk.Shape(), chosen);
        return wrap_checked(mk.Shape(), "el radio no entra en esas aristas");
    });
}

CadShape* cad_chamfer(const CadShape* s, const int32_t* edges, int32_t n, double distance) {
    return guard("chaflán", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        BRepFilletAPI_MakeChamfer mk(s->s);
        std::vector<TopoDS_Shape> chosen;
        for (int32_t i = 0; i < n; i++) {
            if (edges[i] < 0 || edges[i] >= m.Extent()) throw Standard_Failure("arista inexistente");
            mk.Add(distance, TopoDS::Edge(m(edges[i] + 1)));
            chosen.push_back(m(edges[i] + 1));
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("distancia demasiado grande para esas aristas");
        record(mk, {s->s}, mk.Shape(), chosen);
        return wrap_checked(mk.Shape(), "la distancia no entra en esas aristas");
    });
}

CadShape* cad_shell(const CadShape* s, const int32_t* faces, int32_t n, double thickness) {
    return guard("cáscara", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        TopTools_ListOfShape remove;
        for (int32_t i = 0; i < n; i++) {
            if (faces[i] < 0 || faces[i] >= m.Extent()) throw Standard_Failure("cara inexistente");
            remove.Append(m(faces[i] + 1));
        }
        if (remove.IsEmpty()) throw Standard_Failure("elegir al menos una cara para abrir");
        BRepOffsetAPI_MakeThickSolid mk;
        mk.MakeThickSolidByJoin(s->s, remove, thickness, 1e-3);
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("grosor incompatible con la forma");
        record(mk, {s->s}, mk.Shape());
        return wrap_checked(mk.Shape(), "cáscara");
    });
}

CadShape* cad_draft(const CadShape* s, const int32_t* faces, int32_t n, const double* d,
                    double angle, const double* neutral_origin, const double* neutral_normal) {
    return guard("desmolde", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        BRepOffsetAPI_DraftAngle mk(s->s);
        gp_Pln neutral(pnt(neutral_origin), dir(neutral_normal));
        for (int32_t i = 0; i < n; i++) {
            if (faces[i] < 0 || faces[i] >= m.Extent()) throw Standard_Failure("cara inexistente");
            mk.Add(TopoDS::Face(m(faces[i] + 1)), dir(d), angle, neutral);
            if (!mk.AddDone()) throw Standard_Failure("la cara no admite desmolde");
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo aplicar el desmolde");
        record(mk, {s->s}, mk.Shape());
        return wrap_checked(mk.Shape(), "desmolde");
    });
}

CadShape* cad_transform(const CadShape* s, const double* m) {
    return guard("transformar", (CadShape*)nullptr, [&] {
        try {
            gp_Trsf t;
            t.SetValues(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8], m[9], m[10], m[11]);
            BRepBuilderAPI_Transform mk(s->s, t, Standard_True);
            record(mk, {s->s}, mk.Shape());
            return wrap(mk.Shape());
        } catch (const Standard_Failure&) {
            // No es semejanza (escala no uniforme): transformación general.
            gp_GTrsf g;
            g.SetVectorialPart(gp_Mat(m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]));
            g.SetTranslationPart(gp_XYZ(m[3], m[7], m[11]));
            BRepBuilderAPI_GTransform mk(s->s, g, Standard_True);
            record(mk, {s->s}, mk.Shape());
            return wrap(mk.Shape());
        }
    });
}

CadShape* cad_mirror(const CadShape* s, const double* origin, const double* normal) {
    return guard("espejar", (CadShape*)nullptr, [&] {
        gp_Trsf t;
        t.SetMirror(gp_Ax2(pnt(origin), dir(normal)));
        BRepBuilderAPI_Transform mk(s->s, t, Standard_True);
        record(mk, {s->s}, mk.Shape());
        return wrap(mk.Shape());
    });
}

CadShape* cad_split_keep(const CadShape* s, const double* origin, const double* normal) {
    return guard("cortar", (CadShape*)nullptr, [&] {
        gp_Pln pl(pnt(origin), dir(normal));
        TopoDS_Face f = BRepBuilderAPI_MakeFace(pl).Face();
        gp_Pnt ref = pnt(origin).Translated(gp_Vec(dir(normal)));
        TopoDS_Shape half = BRepPrimAPI_MakeHalfSpace(f, ref).Solid();
        return wrap(boolean_op(s->s, half, 2));
    });
}

CadShape* cad_from_mesh(const double* verts, int32_t n_verts, const int32_t* tris, int32_t n_tris,
                        double tolerance) {
    return guard("malla a sólido", (CadShape*)nullptr, [&] {
        BRepBuilderAPI_Sewing sew(tolerance);
        for (int32_t i = 0; i < n_tris; i++) {
            int32_t a = tris[3 * i], b = tris[3 * i + 1], c = tris[3 * i + 2];
            if (a < 0 || b < 0 || c < 0 || a >= n_verts || b >= n_verts || c >= n_verts)
                throw Standard_Failure("índice de vértice fuera de rango");
            gp_Pnt pa = pnt(verts + 3 * a), pb = pnt(verts + 3 * b), pc = pnt(verts + 3 * c);
            if (pa.Distance(pb) < tolerance || pb.Distance(pc) < tolerance ||
                pc.Distance(pa) < tolerance)
                continue;  // triángulo degenerado
            BRepBuilderAPI_MakePolygon poly(pa, pb, pc, Standard_True);
            BRepBuilderAPI_MakeFace mf(poly.Wire(), Standard_True);
            if (mf.IsDone()) sew.Add(mf.Face());
        }
        sew.Perform();
        TopoDS_Shape sewed = sew.SewedShape();
        TopoDS_Shape result = sewed;
        if (sewed.ShapeType() == TopAbs_SHELL) {
            BRepBuilderAPI_MakeSolid ms(TopoDS::Shell(sewed));
            if (ms.IsDone()) result = ms.Solid();
        }
        ShapeFix_Shape fix(result);
        fix.Perform();
        return wrap(fix.Shape());
    });
}

// --- Topología --------------------------------------------------------------

int32_t cad_count_faces(const CadShape* s) { return map_of(s->s, TopAbs_FACE).Extent(); }
int32_t cad_count_edges(const CadShape* s) { return map_of(s->s, TopAbs_EDGE).Extent(); }

int32_t cad_face_info(const CadShape* s, int32_t index, CadFaceInfo* out) {
    return guard("cara", 0, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("cara inexistente");
        TopoDS_Face face = TopoDS::Face(m(index + 1));
        std::memset(out, 0, sizeof(CadFaceInfo));

        GProp_GProps props;
        BRepGProp::SurfaceProperties(face, props);
        out->area = props.Mass();
        gp_Pnt c = props.CentreOfMass();
        put(out->center, c.XYZ());

        gp_Pnt p;
        gp_Vec n;
        face_point_normal(face, c, p, n);
        put(out->point, p.XYZ());
        put(out->normal, n.XYZ());

        BRepAdaptor_Surface ad(face);
        switch (ad.GetType()) {
            case GeomAbs_Plane: out->surface = 0; break;
            case GeomAbs_Cylinder: {
                out->surface = 1;
                gp_Cylinder cy = ad.Cylinder();
                put(out->axis_origin, cy.Axis().Location().XYZ());
                put(out->axis_dir, cy.Axis().Direction().XYZ());
                out->radius = cy.Radius();
                break;
            }
            case GeomAbs_Cone: {
                out->surface = 2;
                gp_Cone co = ad.Cone();
                put(out->axis_origin, co.Axis().Location().XYZ());
                put(out->axis_dir, co.Axis().Direction().XYZ());
                out->radius = co.RefRadius();
                break;
            }
            case GeomAbs_Sphere: {
                out->surface = 3;
                gp_Sphere sp = ad.Sphere();
                put(out->axis_origin, sp.Location().XYZ());
                put(out->axis_dir, sp.Position().Direction().XYZ());
                out->radius = sp.Radius();
                break;
            }
            case GeomAbs_Torus: {
                out->surface = 4;
                gp_Torus to = ad.Torus();
                put(out->axis_origin, to.Axis().Location().XYZ());
                put(out->axis_dir, to.Axis().Direction().XYZ());
                out->radius = to.MajorRadius();
                break;
            }
            case GeomAbs_BezierSurface:
            case GeomAbs_BSplineSurface: out->surface = 5; break;
            case GeomAbs_SurfaceOfRevolution: out->surface = 6; break;
            case GeomAbs_SurfaceOfExtrusion: out->surface = 7; break;
            case GeomAbs_OffsetSurface: out->surface = 8; break;
            default: out->surface = 9; break;
        }
        return 1;
    });
}

int32_t cad_edge_info(const CadShape* s, int32_t index, CadEdgeInfo* out) {
    return guard("arista", 0, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("arista inexistente");
        TopoDS_Edge edge = TopoDS::Edge(m(index + 1));
        std::memset(out, 0, sizeof(CadEdgeInfo));
        if (BRep_Tool::Degenerated(edge)) {
            out->curve = 4;
            return 1;
        }
        BRepAdaptor_Curve c(edge);
        double t0 = c.FirstParameter(), t1 = c.LastParameter();
        out->length = GCPnts_AbscissaPoint::Length(c);
        put(out->start, c.Value(t0).XYZ());
        put(out->end, c.Value(t1).XYZ());
        gp_Pnt mid;
        gp_Vec tan;
        c.D1((t0 + t1) / 2, mid, tan);
        put(out->mid, mid.XYZ());
        if (tan.Magnitude() > 1e-12) tan.Normalize();
        put(out->tangent, tan.XYZ());
        out->closed = BRep_Tool::IsClosed(edge) || c.Value(t0).Distance(c.Value(t1)) < Precision::Confusion();
        switch (c.GetType()) {
            case GeomAbs_Line: out->curve = 0; break;
            case GeomAbs_Circle: {
                out->curve = 1;
                gp_Circ ci = c.Circle();
                put(out->center, ci.Location().XYZ());
                put(out->axis, ci.Axis().Direction().XYZ());
                out->radius = ci.Radius();
                break;
            }
            case GeomAbs_Ellipse: out->curve = 2; break;
            case GeomAbs_BezierCurve:
            case GeomAbs_BSplineCurve: out->curve = 3; break;
            default: out->curve = 4; break;
        }
        return 1;
    });
}

int32_t cad_edge_faces(const CadShape* s, int32_t edge, int32_t* out2) {
    return guard("arista", 0, [&] {
        auto edges = map_of(s->s, TopAbs_EDGE);
        auto faces = map_of(s->s, TopAbs_FACE);
        if (edge < 0 || edge >= edges.Extent()) throw Standard_Failure("arista inexistente");
        TopTools_IndexedDataMapOfShapeListOfShape anc;
        TopExp::MapShapesAndUniqueAncestors(s->s, TopAbs_EDGE, TopAbs_FACE, anc);
        const TopTools_ListOfShape& list = anc.FindFromKey(edges(edge + 1));
        int32_t k = 0;
        for (const TopoDS_Shape& f : list) {
            if (k >= 2) break;
            out2[k++] = faces.FindIndex(f) - 1;
        }
        return k;
    });
}

int32_t cad_closest_face(const CadShape* s, const double* point, const double* normal,
                         double min_cos, double* dist) {
    return guard("buscar cara", (int32_t)-1, [&]() -> int32_t {
        auto m = map_of(s->s, TopAbs_FACE);
        TopoDS_Vertex v = BRepBuilderAPI_MakeVertex(pnt(point)).Vertex();
        int32_t best = -1;
        double best_d = 1e300;
        for (int i = 1; i <= m.Extent(); i++) {
            TopoDS_Face face = TopoDS::Face(m(i));
            BRepExtrema_DistShapeShape ext(v, face);
            if (!ext.IsDone() || ext.NbSolution() < 1) continue;
            double d = ext.Value();
            if (d >= best_d) continue;
            if (normal) {
                double u = 0, w = 0;
                gp_Pnt p;
                gp_Vec n;
                if (ext.SupportTypeShape2(1) == BRepExtrema_IsInFace) {
                    ext.ParOnFaceS2(1, u, w);
                } else {
                    // Punto más cercano en un borde/vértice: proyectar para tener (u, v)
                    GeomAPI_ProjectPointOnSurf proj(ext.PointOnShape2(1), BRep_Tool::Surface(face));
                    if (proj.NbPoints() < 1) continue;
                    proj.LowerDistanceParameters(u, w);
                }
                BRepGProp_Face(face).Normal(u, w, p, n);
                if (n.Magnitude() < 1e-12) continue;
                n.Normalize();
                if (n.Dot(gp_Vec(normal[0], normal[1], normal[2]).Normalized()) < min_cos) continue;
            }
            best_d = d;
            best = i - 1;
        }
        *dist = best_d;
        return best;
    });
}

int32_t cad_closest_edge(const CadShape* s, const double* point, const double* d,
                         double min_cos, double* dist) {
    return guard("buscar arista", (int32_t)-1, [&]() -> int32_t {
        auto m = map_of(s->s, TopAbs_EDGE);
        TopoDS_Vertex v = BRepBuilderAPI_MakeVertex(pnt(point)).Vertex();
        int32_t best = -1;
        double best_d = 1e300;
        for (int i = 1; i <= m.Extent(); i++) {
            TopoDS_Edge edge = TopoDS::Edge(m(i));
            if (BRep_Tool::Degenerated(edge)) continue;
            BRepExtrema_DistShapeShape ext(v, edge);
            if (!ext.IsDone() || ext.NbSolution() < 1) continue;
            double dd = ext.Value();
            if (dd >= best_d) continue;
            if (d) {
                BRepAdaptor_Curve c(edge);
                double t = (c.FirstParameter() + c.LastParameter()) / 2;
                if (ext.SupportTypeShape2(1) == BRepExtrema_IsOnEdge) ext.ParOnEdgeS2(1, t);
                gp_Pnt p;
                gp_Vec tan;
                c.D1(t, p, tan);
                if (tan.Magnitude() < 1e-12) continue;
                tan.Normalize();
                if (std::fabs(tan.Dot(gp_Vec(d[0], d[1], d[2]).Normalized())) < min_cos) continue;
            }
            best_d = dd;
            best = i - 1;
        }
        *dist = best_d;
        return best;
    });
}

double cad_face_distance(const CadShape* s, int32_t index, const double* point) {
    return guard("distancia", -1.0, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("cara inexistente");
        BRepExtrema_DistShapeShape ext(BRepBuilderAPI_MakeVertex(pnt(point)).Vertex(), m(index + 1));
        return ext.IsDone() && ext.NbSolution() > 0 ? ext.Value() : -1.0;
    });
}

double cad_edge_distance(const CadShape* s, int32_t index, const double* point) {
    return guard("distancia", -1.0, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("arista inexistente");
        BRepExtrema_DistShapeShape ext(BRepBuilderAPI_MakeVertex(pnt(point)).Vertex(), m(index + 1));
        return ext.IsDone() && ext.NbSolution() > 0 ? ext.Value() : -1.0;
    });
}

int32_t cad_edge_face_pairs(const CadShape* s, int32_t* out) {
    return guard("aristas", 0, [&] {
        auto edges = map_of(s->s, TopAbs_EDGE);
        auto faces = map_of(s->s, TopAbs_FACE);
        TopTools_IndexedDataMapOfShapeListOfShape anc;
        TopExp::MapShapesAndUniqueAncestors(s->s, TopAbs_EDGE, TopAbs_FACE, anc);
        for (int e = 1; e <= edges.Extent(); e++) {
            out[2 * (e - 1)] = -1;
            out[2 * (e - 1) + 1] = -1;
            const TopTools_ListOfShape& list = anc.FindFromKey(edges(e));
            int k = 0;
            for (const TopoDS_Shape& f : list) {
                if (k >= 2) break;
                out[2 * (e - 1) + k++] = faces.FindIndex(f) - 1;
            }
        }
        return 1;
    });
}

int32_t cad_mass_info(const CadShape* s, CadMassInfo* out) {
    return guard("medidas", 0, [&] {
        std::memset(out, 0, sizeof(CadMassInfo));
        GProp_GProps vol, surf;
        BRepGProp::VolumeProperties(s->s, vol);
        BRepGProp::SurfaceProperties(s->s, surf);
        out->volume = vol.Mass();
        out->area = surf.Mass();
        gp_Pnt c = std::fabs(vol.Mass()) > 1e-12 ? vol.CentreOfMass() : surf.CentreOfMass();
        put(out->center, c.XYZ());
        Bnd_Box box;
        BRepBndLib::AddOptimal(s->s, box, Standard_False, Standard_False);
        if (!box.IsVoid()) {
            double x0, y0, z0, x1, y1, z1;
            box.Get(x0, y0, z0, x1, y1, z1);
            out->bbox_min[0] = x0;
            out->bbox_min[1] = y0;
            out->bbox_min[2] = z0;
            out->bbox_max[0] = x1;
            out->bbox_max[1] = y1;
            out->bbox_max[2] = z1;
        }
        return 1;
    });
}

// --- Teselado ---------------------------------------------------------------

int32_t cad_tessellate(const CadShape* s, double linear, double angular, CadMesh* out) {
    std::memset(out, 0, sizeof(CadMesh));
    return guard("teselar", 0, [&] {
        BRepMesh_IncrementalMesh mesher(s->s, linear, Standard_False, angular, Standard_True);
        std::vector<double> pos, nor, epts;
        std::vector<uint32_t> tri;
        std::vector<int32_t> tface;
        std::vector<size_t> eoff;

        auto faces = map_of(s->s, TopAbs_FACE);
        for (int fi = 1; fi <= faces.Extent(); fi++) {
            TopoDS_Face face = TopoDS::Face(faces(fi));
            TopLoc_Location loc;
            Handle(Poly_Triangulation) t = BRep_Tool::Triangulation(face, loc);
            if (t.IsNull()) continue;
            const gp_Trsf& tr = loc.Transformation();
            bool reversed = face.Orientation() == TopAbs_REVERSED;
            uint32_t base = static_cast<uint32_t>(pos.size() / 3);
            BRepGProp_Face gf(face);
            if (!t->HasUVNodes() && !t->HasNormals()) Poly::ComputeNormals(t);
            for (int i = 1; i <= t->NbNodes(); i++) {
                gp_Pnt p = t->Node(i).Transformed(tr);
                pos.insert(pos.end(), {p.X(), p.Y(), p.Z()});
                gp_Vec n;
                if (t->HasUVNodes()) {
                    gp_Pnt2d uv = t->UVNode(i);
                    gp_Pnt q;
                    gf.Normal(uv.X(), uv.Y(), q, n);
                } else {
                    gp_Dir d = t->Normal(i);
                    n = gp_Vec(d);
                    if (reversed) n.Reverse();
                }
                n.Transform(tr);
                double len = n.Magnitude();
                if (len > 1e-12) n /= len;
                nor.insert(nor.end(), {n.X(), n.Y(), n.Z()});
            }
            for (int i = 1; i <= t->NbTriangles(); i++) {
                int a, b, c;
                t->Triangle(i).Get(a, b, c);
                if (reversed) std::swap(b, c);
                tri.insert(tri.end(), {base + a - 1, base + b - 1, base + c - 1});
                tface.push_back(fi - 1);
            }
        }

        auto edges = map_of(s->s, TopAbs_EDGE);
        eoff.push_back(0);
        for (int ei = 1; ei <= edges.Extent(); ei++) {
            TopoDS_Edge e = TopoDS::Edge(edges(ei));
            if (!BRep_Tool::Degenerated(e)) {
                BRepAdaptor_Curve c(e);
                GCPnts_TangentialDeflection d(c, angular, linear);
                for (int i = 1; i <= d.NbPoints(); i++) {
                    gp_Pnt p = d.Value(i);
                    epts.insert(epts.end(), {p.X(), p.Y(), p.Z()});
                }
            }
            eoff.push_back(epts.size() / 3);
        }

        auto dup = [](const auto& v) {
            using T = typename std::decay_t<decltype(v)>::value_type;
            T* p = static_cast<T*>(std::malloc(sizeof(T) * (v.empty() ? 1 : v.size())));
            if (!v.empty()) std::memcpy(p, v.data(), sizeof(T) * v.size());
            return p;
        };
        out->positions = dup(pos);
        out->normals = dup(nor);
        out->triangles = dup(tri);
        out->triangle_face = dup(tface);
        out->n_vertices = pos.size() / 3;
        out->n_triangles = tface.size();
        out->edge_points = dup(epts);
        out->edge_offsets = dup(eoff);
        out->n_edges = eoff.size() - 1;
        return 1;
    });
}

void cad_mesh_free(CadMesh* m) {
    if (!m) return;
    std::free(m->positions);
    std::free(m->normals);
    std::free(m->triangles);
    std::free(m->triangle_face);
    std::free(m->edge_points);
    std::free(m->edge_offsets);
    std::memset(m, 0, sizeof(CadMesh));
}

// --- Archivos ---------------------------------------------------------------

// Los traductores de STEP imprimen estadísticas por stdout: silenciarlas.
static void quiet_messages() {
    static std::once_flag once;
    std::call_once(once, [] {
        Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_PrinterOStream));
    });
}

static int32_t give_bytes(const std::string& s, uint8_t** out, size_t* len) {
    *out = static_cast<uint8_t*>(std::malloc(s.size() ? s.size() : 1));
    std::memcpy(*out, s.data(), s.size());
    *len = s.size();
    return 1;
}

int32_t cad_write_step(const CadShape* s, uint8_t** out, size_t* len) {
    quiet_messages();
    return guard("escribir STEP", 0, [&] {
        STEPControl_Writer w;
        if (w.Transfer(s->s, STEPControl_AsIs) != IFSelect_RetDone)
            throw Standard_Failure("no se pudo traducir la forma a STEP");
        std::ostringstream os;
        if (w.WriteStream(os) != IFSelect_RetDone) throw Standard_Failure("no se pudo escribir STEP");
        return give_bytes(os.str(), out, len);
    });
}

CadShape* cad_read_step(const uint8_t* data, size_t len) {
    quiet_messages();
    return guard("leer STEP", (CadShape*)nullptr, [&] {
        std::istringstream is(std::string(reinterpret_cast<const char*>(data), len));
        STEPControl_Reader r;
        if (r.ReadStream("modelo.step", is) != IFSelect_RetDone)
            throw Standard_Failure("archivo STEP ilegible");
        r.TransferRoots();
        TopoDS_Shape s = r.OneShape();
        if (s.IsNull()) throw Standard_Failure("el archivo STEP no tiene formas");
        return wrap(s);
    });
}

int32_t cad_write_brep(const CadShape* s, uint8_t** out, size_t* len) {
    return guard("escribir BREP", 0, [&] {
        std::ostringstream os;
        BRepTools::Write(s->s, os);
        return give_bytes(os.str(), out, len);
    });
}

CadShape* cad_read_brep(const uint8_t* data, size_t len) {
    return guard("leer BREP", (CadShape*)nullptr, [&] {
        std::istringstream is(std::string(reinterpret_cast<const char*>(data), len));
        TopoDS_Shape s;
        BRep_Builder b;
        BRepTools::Read(s, is, b);
        return wrap(s);
    });
}

void cad_bytes_free(uint8_t* p) { std::free(p); }

int32_t cad_take_history(CadHistory* out) {
    std::memset(out, 0, sizeof(CadHistory));
    std::vector<int32_t> offsets{0}, faces;
    for (const auto& v : g_history) {
        faces.insert(faces.end(), v.begin(), v.end());
        offsets.push_back(static_cast<int32_t>(faces.size()));
    }
    out->n = static_cast<int32_t>(g_history.size());
    out->offsets = static_cast<int32_t*>(std::malloc(sizeof(int32_t) * offsets.size()));
    std::memcpy(out->offsets, offsets.data(), sizeof(int32_t) * offsets.size());
    out->faces = static_cast<int32_t*>(std::malloc(sizeof(int32_t) * (faces.empty() ? 1 : faces.size())));
    if (!faces.empty()) std::memcpy(out->faces, faces.data(), sizeof(int32_t) * faces.size());
    g_history.clear();
    return out->n;
}

void cad_history_free(CadHistory* h) {
    std::free(h->offsets);
    std::free(h->faces);
    std::memset(h, 0, sizeof(CadHistory));
}

}  // extern "C"
