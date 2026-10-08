// Implementación del puente con OpenCASCADE. Ver cad_occt.h.

#define _USE_MATH_DEFINES  // M_PI en MSVC
#include "cad_occt.h"

#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <HLRAlgo_Projector.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_CompCurve.hxx>
#include <GCPnts_UniformAbscissa.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom2d_Line.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <gp_Ax3.hxx>
#include <BRepLib.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <LocOpe_DPrism.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <gp_Lin.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <TDocStd_Document.hxx>
#include <XCAFApp_Application.hxx>
#include <XCAFDoc_ColorTool.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <TDataStd_Name.hxx>
#include <Quantity_Color.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
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
#include <GProp_PrincipalProps.hxx>
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
#include <gp_Elips.hxx>
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
        case 5: {
            // Puntos y al final las tangentes de salida y de llegada
            if (count < 12 || count % 3 != 0) throw Standard_Failure("spline: puntos incompletos");
            int n = count / 3 - 2;
            Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, n);
            for (int i = 0; i < n; i++) pts->SetValue(i + 1, pnt(d + 3 * i));
            gp_Vec t0(d[3 * n], d[3 * n + 1], d[3 * n + 2]);
            gp_Vec t1(d[3 * n + 3], d[3 * n + 4], d[3 * n + 5]);
            if (t0.Magnitude() < Precision::Confusion() || t1.Magnitude() < Precision::Confusion())
                throw Standard_Failure("spline: manija sobre su punto");
            GeomAPI_Interpolate interp(pts, Standard_False, Precision::Confusion());
            // Solo la dirección: el largo lo ajusta OCCT a la parametrización
            interp.Load(t0, t1, Standard_True);
            interp.Perform();
            if (!interp.IsDone()) throw Standard_Failure("spline: no se pudo interpolar");
            return BRepBuilderAPI_MakeEdge(interp.Curve()).Edge();
        }
        case 4: {
            if (count != 11) throw Standard_Failure("elipse: se esperaban 11 valores");
            double a = d[9], b = d[10];
            if (a <= 0 || b <= 0) throw Standard_Failure("elipse: radio no positivo");
            gp_Dir n = dir(d + 3);
            gp_Dir x = dir(d + 6);
            // OCCT pide el radio mayor primero: si b es más largo, el eje mayor va a 90°
            if (b > a) {
                x = n.Crossed(x);
                std::swap(a, b);
            }
            gp_Elips e(gp_Ax2(pnt(d), n, x), a, b);
            return BRepBuilderAPI_MakeEdge(e).Edge();
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
    // Armada vacía: el constructor con las dos formas ya calcula, y el Build
    // de abajo la volvía a calcular entera (cada booleana costaba el doble)
    auto finish = [&](BRepAlgoAPI_BooleanOperation& algo) {
        TopTools_ListOfShape args, tools;
        args.Append(a);
        tools.Append(b);
        algo.SetArguments(args);
        algo.SetTools(tools);
        // Sin cajas orientadas: con un medio espacio (cortar por plano) es
        // infinito y la caja lo dejaba fuera (el resultado salía vacío)
        algo.SetRunParallel(Standard_True);
        algo.Build();
        if (algo.HasErrors() || !algo.IsDone()) throw Standard_Failure("la operación booleana falló");
        algo.SimplifyResult();
        TopoDS_Shape r = algo.Shape();
        record(algo, {a, b}, r);
        return r;
    };
    if (op == 0) {
        BRepAlgoAPI_Fuse f;
        return finish(f);
    }
    if (op == 1) {
        BRepAlgoAPI_Cut c;
        return finish(c);
    }
    if (op == 2) {
        BRepAlgoAPI_Common c;
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
        // Con una cara: cada lazo con MakePipeShell (esquinas a inglete, como
        // Onshape; MakePipe deja sólidos inválidos en esquinas vivas) y los
        // agujeros restados. Sin cara (alambre o arista), el barrido de siempre.
        TopExp_Explorer fx(profile->s, TopAbs_FACE);
        if (!fx.More()) {
            BRepOffsetAPI_MakePipe mk(w, profile->s);
            mk.Build();
            if (!mk.IsDone()) throw Standard_Failure("no se pudo barrer");
            return wrap(mk.Shape());
        }
        auto sweep_wire = [&](const TopoDS_Wire& section) {
            BRepOffsetAPI_MakePipeShell mk(w);
            mk.SetMode(Standard_False);
            mk.SetTransitionMode(BRepBuilderAPI_RightCorner);
            mk.Add(section, Standard_False, Standard_False);
            mk.Build();
            if (!mk.IsDone()) throw Standard_Failure("no se pudo barrer el perfil por ese camino");
            if (!mk.MakeSolid()) throw Standard_Failure("el barrido no cierra un sólido");
            return mk.Shape();
        };
        TopoDS_Face face = TopoDS::Face(fx.Current());
        TopoDS_Wire outer = BRepTools::OuterWire(face);
        TopoDS_Shape result = sweep_wire(outer);
        for (TopExp_Explorer wx(face, TopAbs_WIRE); wx.More(); wx.Next()) {
            TopoDS_Wire inner = TopoDS::Wire(wx.Current());
            if (inner.IsSame(outer)) continue;
            result = BRepAlgoAPI_Cut(result, sweep_wire(inner)).Shape();
        }
        return wrap_checked(result, "barrido");
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
        f.SetRunParallel(Standard_True);
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

CadShape* cad_fillet(const CadShape* s, const int32_t* edges, int32_t n, double radius, double radius2) {
    return guard("redondeo", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        BRepFilletAPI_MakeFillet mk(s->s);
        std::vector<TopoDS_Shape> chosen;
        for (int32_t i = 0; i < n; i++) {
            if (edges[i] < 0 || edges[i] >= m.Extent()) throw Standard_Failure("arista inexistente");
            // Radio variable: de `radius` al comienzo de la arista a `radius2` al final
            if (std::fabs(radius2 - radius) > 1e-12) mk.Add(radius, radius2, TopoDS::Edge(m(edges[i] + 1)));
            else mk.Add(radius, TopoDS::Edge(m(edges[i] + 1)));
            chosen.push_back(m(edges[i] + 1));
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("radio demasiado grande para esas aristas");
        record(mk, {s->s}, mk.Shape(), chosen);
        return wrap_checked(mk.Shape(), "el radio no entra en esas aristas");
    });
}

CadShape* cad_chamfer(const CadShape* s, const int32_t* edges, int32_t n, double distance, double second,
                      int32_t mode, int32_t flip) {
    return guard("chaflán", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        TopTools_IndexedDataMapOfShapeListOfShape anc;
        TopExp::MapShapesAndAncestors(s->s, TopAbs_EDGE, TopAbs_FACE, anc);
        BRepFilletAPI_MakeChamfer mk(s->s);
        std::vector<TopoDS_Shape> chosen;
        for (int32_t i = 0; i < n; i++) {
            if (edges[i] < 0 || edges[i] >= m.Extent()) throw Standard_Failure("arista inexistente");
            TopoDS_Edge e = TopoDS::Edge(m(edges[i] + 1));
            if (mode == 0) {
                mk.Add(distance, e);
            } else {
                // La primera distancia se mide sobre una de las dos caras (`flip`: la otra)
                const TopTools_ListOfShape& faces = anc.FindFromKey(e);
                if (faces.Extent() < 2) throw Standard_Failure("la arista no separa dos caras");
                TopoDS_Face f = TopoDS::Face(flip ? faces.Last() : faces.First());
                if (mode == 1) mk.Add(distance, second, e, f);
                else mk.AddDA(distance, second * M_PI / 180.0, e, f);
            }
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
        // ¿Semejanza? Columnas ortogonales y del mismo largo; si no (escala no
        // uniforme), la transformación general. gp_Trsf::SetValues no siempre
        // avisa: con una matriz que no lo es se queda solo con la traslación.
        gp_XYZ c0(m[0], m[4], m[8]), c1(m[1], m[5], m[9]), c2(m[2], m[6], m[10]);
        double l0 = c0.Modulus(), l1 = c1.Modulus(), l2 = c2.Modulus();
        double tol = 1e-9 * std::max({l0, l1, l2, 1.0});
        bool similar = std::fabs(l0 - l1) < tol && std::fabs(l1 - l2) < tol && std::fabs(c0.Dot(c1)) < tol * l0 &&
                       std::fabs(c1.Dot(c2)) < tol * l1 && std::fabs(c0.Dot(c2)) < tol * l0;
        try {
            if (!similar) throw Standard_Failure("no es semejanza");
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

CadShape* cad_sub_shape(const CadShape* s, int32_t kind, int32_t index) {
    return guard("elemento", (CadShape*)nullptr, [&] {
        TopAbs_ShapeEnum t = kind == 0 ? TopAbs_FACE : kind == 1 ? TopAbs_EDGE : kind == 2 ? TopAbs_VERTEX : TopAbs_SOLID;
        auto m = map_of(s->s, t);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("elemento inexistente");
        return new CadShape{m(index + 1)};
    });
}

int32_t cad_count_solids(const CadShape* s) {
    return guard("sólidos", 0, [&] { return (int32_t)map_of(s->s, TopAbs_SOLID).Extent(); });
}

int32_t cad_face_indices_in(const CadShape* parent, const CadShape* child, int32_t* out) {
    return guard("caras", 0, [&] {
        auto pm = map_of(parent->s, TopAbs_FACE);
        auto cm = map_of(child->s, TopAbs_FACE);
        for (int i = 1; i <= cm.Extent(); i++) out[i - 1] = pm.FindIndex(cm(i)) - 1;
        return 1;
    });
}

int32_t cad_count_vertices(const CadShape* s) {
    return guard("vértices", 0, [&] { return (int32_t)map_of(s->s, TopAbs_VERTEX).Extent(); });
}

int32_t cad_vertex_point(const CadShape* s, int32_t index, double* out) {
    return guard("vértice", 0, [&] {
        auto m = map_of(s->s, TopAbs_VERTEX);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("vértice inexistente");
        gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(m(index + 1)));
        out[0] = p.X();
        out[1] = p.Y();
        out[2] = p.Z();
        return 1;
    });
}

CadShape* cad_make_vertex(const double* p) {
    return guard("vértice", (CadShape*)nullptr, [&] { return new CadShape{BRepBuilderAPI_MakeVertex(pnt(p)).Vertex()}; });
}

double cad_min_distance(const CadShape* a, const CadShape* b, double* pa, double* pb) {
    return guard("distancia", -1.0, [&] {
        BRepExtrema_DistShapeShape ext(a->s, b->s);
        if (!ext.IsDone() || ext.NbSolution() == 0) return -1.0;
        gp_Pnt p = ext.PointOnShape1(1), q = ext.PointOnShape2(1);
        pa[0] = p.X(); pa[1] = p.Y(); pa[2] = p.Z();
        pb[0] = q.X(); pb[1] = q.Y(); pb[2] = q.Z();
        return ext.Value();
    });
}

CadShape* cad_make_helix(const double* origin, const double* dir, double radius, double pitch, double turns,
                         int32_t left) {
    return guard("hélice", (CadShape*)nullptr, [&] {
        if (radius <= 0 || pitch <= 0 || turns <= 0) throw Standard_Failure("radio, paso y vueltas tienen que ser positivos");
        gp_Ax3 ax(pnt(origin), gp_Dir(dir[0], dir[1], dir[2]));
        Handle(Geom_CylindricalSurface) cyl = new Geom_CylindricalSurface(ax, radius);
        // En (u, v) del cilindro la hélice es una recta: u = ángulo, v = altura
        double du = left ? -2 * M_PI : 2 * M_PI;
        gp_Dir2d d(du, pitch);
        Handle(Geom2d_Line) line = new Geom2d_Line(gp_Pnt2d(0, 0), d);
        double len = turns * std::sqrt(du * du + pitch * pitch);
        TopoDS_Edge e = BRepBuilderAPI_MakeEdge(line, cyl, 0.0, len).Edge();
        BRepLib::BuildCurves3d(e);
        return new CadShape{BRepBuilderAPI_MakeWire(e).Wire()};
    });
}

CadShape* cad_make_thread(const double* origin, const double* dir, const double* xdir, double r_minor, double r_major,
                          double pitch, double length, int32_t left, int32_t chamfer) {
    return guard("rosca", (CadShape*)nullptr, [&] {
        if (!(r_minor > 0 && r_major > r_minor && pitch > 0 && length > 0))
            throw Standard_Failure("la rosca necesita radio menor < mayor, paso y largo positivos");
        gp_Dir z(dir[0], dir[1], dir[2]);
        // X fija la fase: el filete arranca en origin + r·X (roscas que calzan
        // comparten la misma hélice)
        gp_Vec xv(xdir[0], xdir[1], xdir[2]);
        xv -= gp_Vec(z) * xv.Dot(gp_Vec(z));
        if (xv.Magnitude() < 1e-9) throw Standard_Failure("la X de la rosca no puede ser paralela al eje");
        gp_Ax3 base(pnt(origin), z, gp_Dir(xv));
        // El filete arranca una vuelta antes y termina una después: al final se
        // recorta al largo pedido y las puntas quedan planas
        gp_Ax3 ax(base.Location().Translated(gp_Vec(z) * -pitch), z, base.XDirection());
        // La hélice va apenas adentro del núcleo, para que la unión no quede rozando
        double eps = std::min(0.02 * (r_major - r_minor), 0.01);
        double rh = r_minor - eps;
        double turns = length / pitch + 2.0;
        Handle(Geom_CylindricalSurface) cyl = new Geom_CylindricalSurface(ax, rh);
        double du = left ? -2 * M_PI : 2 * M_PI;
        Handle(Geom2d_Line) line = new Geom2d_Line(gp_Pnt2d(0, 0), gp_Dir2d(du, pitch));
        // Un tramo por vuelta: el barrido sale en caras chicas (una sola cara para
        // toda la hélice es una superficie enorme y unirla crecía más que lineal:
        // 40 mm de M6 tardaba 8 s)
        double per_turn = std::sqrt(du * du + pitch * pitch);
        BRepBuilderAPI_MakeWire wire;
        int pieces = (int)std::ceil(turns - 1e-9);
        for (int k = 0; k < pieces; k++) {
            double a = k * per_turn, b = std::min(turns, k + 1.0) * per_turn;
            TopoDS_Edge e = BRepBuilderAPI_MakeEdge(line, cyl, a, b).Edge();
            BRepLib::BuildCurves3d(e);
            wire.Add(e);
        }
        TopoDS_Wire spine = wire.Wire();
        // Perfil ISO básico en el plano que contiene al eje: flancos de 60°,
        // cresta de P/8 en el radio mayor y fondo de P/4 entre filetes
        double t30 = std::tan(M_PI / 6);
        double top = pitch / 8, bottom = top + 2 * (r_major - rh) * t30;
        gp_Pnt c0 = ax.Location();
        gp_Vec x(ax.XDirection()), zv(z);
        auto at = [&](double r, double a) { return c0.Translated(x * r + zv * a); };
        BRepBuilderAPI_MakePolygon poly;
        poly.Add(at(rh, -bottom / 2));
        poly.Add(at(r_major, -top / 2));
        poly.Add(at(r_major, top / 2));
        poly.Add(at(rh, bottom / 2));
        poly.Close();
        BRepOffsetAPI_MakePipeShell mk(spine);
        // Binormal fija en el eje: el perfil no se tuerce a lo largo de la hélice
        mk.SetMode(z);
        mk.Add(poly.Wire(), Standard_False, Standard_False);
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo barrer el filete");
        if (!mk.MakeSolid()) throw Standard_Failure("el filete no cierra un sólido");
        gp_Ax2 core_ax(ax.Location(), z, ax.XDirection());
        TopoDS_Shape core = BRepPrimAPI_MakeCylinder(core_ax, r_minor, length + 2 * pitch).Shape();
        // La unión hélice-núcleo es lo caro (cada flanco corta al cilindro en
        // una hélice): en paralelo y con cajas orientadas, un 30 % menos
        BRepAlgoAPI_Fuse fu;
        {
            TopTools_ListOfShape args, tools;
            args.Append(core);
            tools.Append(mk.Shape());
            fu.SetArguments(args);
            fu.SetTools(tools);
            fu.SetRunParallel(Standard_True);
            fu.SetUseOBB(Standard_True);
            fu.Build();
        }
        if (!fu.IsDone()) throw Standard_Failure("no se pudo unir el filete al núcleo");
        TopoDS_Shape rod = fu.Shape();
        // Recorte al largo; con `chamfer`, la punta del comienzo achaflanada a
        // 45° desde el diámetro menor (como la punta de un tornillo), en la
        // misma booleana
        TopoDS_Shape clip;
        double big = r_major * 1.5;
        if (chamfer) {
            double r0 = std::max(r_minor - eps, 0.1 * r_minor);
            double rise = std::min(big - r0, length);
            gp_Pnt o = base.Location();
            gp_Vec bx(base.XDirection());
            auto q = [&](double r, double a) { return o.Translated(bx * r + zv * a); };
            BRepBuilderAPI_MakePolygon prof;
            prof.Add(q(0, 0));
            prof.Add(q(r0, 0));
            prof.Add(q(r0 + rise, rise));
            if (rise < length) prof.Add(q(r0 + rise, length));
            prof.Add(q(0, length));
            prof.Close();
            TopoDS_Face pf = BRepBuilderAPI_MakeFace(prof.Wire(), Standard_True).Face();
            clip = BRepPrimAPI_MakeRevol(pf, gp_Ax1(o, z), 2 * M_PI).Shape();
        } else {
            gp_Ax2 clip_ax(base.Location(), z, base.XDirection());
            clip = BRepPrimAPI_MakeCylinder(clip_ax, big, length).Shape();
        }
        return wrap_checked(BRepAlgoAPI_Common(rod, clip).Shape(), "rosca");
    });
}

CadShape* cad_thicken(const CadShape* faces, double thickness) {
    return guard("engrosar", (CadShape*)nullptr, [&] {
        if (std::fabs(thickness) < 1e-9) throw Standard_Failure("espesor cero");
        BRepOffset_MakeOffset mk;
        mk.Initialize(faces->s, thickness, 1e-6, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Intersection,
                      Standard_True);
        mk.MakeOffsetShape();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo engrosar");
        return wrap_checked(mk.Shape(), "engrosar");
    });
}

int32_t cad_wire_sample(const CadShape* wire, int32_t n, double* out_p, double* out_t) {
    return guard("muestrear curva", 0, [&] {
        if (n < 2) throw Standard_Failure("hacen falta al menos dos puntos");
        TopoDS_Wire w;
        if (wire->s.ShapeType() == TopAbs_WIRE) w = TopoDS::Wire(wire->s);
        else if (wire->s.ShapeType() == TopAbs_EDGE) w = BRepBuilderAPI_MakeWire(TopoDS::Edge(wire->s)).Wire();
        else throw Standard_Failure("se esperaba un alambre");
        BRepAdaptor_CompCurve c(w);
        GCPnts_UniformAbscissa ua(c, (int)n);
        if (!ua.IsDone() || ua.NbPoints() != n) throw Standard_Failure("no se pudo repartir la curva");
        for (int i = 1; i <= n; i++) {
            gp_Pnt p;
            gp_Vec t;
            c.D1(ua.Parameter(i), p, t);
            if (t.Magnitude() > 1e-12) t.Normalize();
            put(out_p + 3 * (i - 1), p.XYZ());
            put(out_t + 3 * (i - 1), t.XYZ());
        }
        return 1;
    });
}

double cad_ray_hit(const CadShape* s, const double* origin, const double* dir) {
    return guard("rayo", -1.0, [&] {
        IntCurvesFace_ShapeIntersector inter;
        inter.Load(s->s, 1e-7);
        gp_Lin line(pnt(origin), gp_Dir(dir[0], dir[1], dir[2]));
        inter.Perform(line, 1e-9, RealLast());
        if (!inter.IsDone() || inter.NbPnt() == 0) return -1.0;
        double best = -1.0;
        for (int i = 1; i <= inter.NbPnt(); i++) {
            double t = inter.WParameter(i);
            if (t > 1e-9 && (best < 0 || t < best)) best = t;
        }
        return best;
    });
}

CadShape* cad_draft_prism(const CadShape* face, double height, double angle) {
    return guard("prisma con desmolde", (CadShape*)nullptr, [&] {
        TopExp_Explorer ex(face->s, TopAbs_FACE);
        if (!ex.More()) throw Standard_Failure("se esperaba una cara");
        // LocOpe_DPrism mide la altura a lo largo de la pared inclinada: la que
        // se pide es la vertical
        if (std::fabs(std::cos(angle)) < 1e-6) throw Standard_Failure("ángulo de desmolde inválido");
        // Hacia el otro lado el ángulo también cambia de signo (para que siga angostándose)
        LocOpe_DPrism dp(TopoDS::Face(ex.Current()), height / std::cos(angle), height < 0 ? -angle : angle);
        TopoDS_Shape r = dp.Shape();
        return wrap_checked(r, "prisma con desmolde");
    });
}

CadShape* cad_offset_face(const CadShape* face, double distance) {
    return guard("desplazar perfil", (CadShape*)nullptr, [&] {
        TopExp_Explorer ex(face->s, TopAbs_FACE);
        if (!ex.More()) throw Standard_Failure("se esperaba una cara");
        TopoDS_Face f = TopoDS::Face(ex.Current());
        BRepOffsetAPI_MakeOffset mk(f, GeomAbs_Arc);
        mk.Perform(distance);
        if (!mk.IsDone()) throw Standard_Failure("no se pudo desplazar el perfil");
        TopoDS_Shape w = mk.Shape();
        // Los lazos desplazados forman la cara nueva (el exterior y sus agujeros)
        TopExp_Explorer wx(w, TopAbs_WIRE);
        if (!wx.More()) throw Standard_Failure("el desplazamiento no dejó contorno");
        BRepBuilderAPI_MakeFace mf(TopoDS::Wire(wx.Current()), Standard_True);
        for (wx.Next(); wx.More(); wx.Next()) mf.Add(TopoDS::Wire(wx.Current()));
        if (!mf.IsDone()) throw Standard_Failure("el contorno desplazado no forma una cara");
        BRepLib::BuildCurves3d(mf.Face());
        ShapeFix_Face fix(mf.Face());
        fix.Perform();
        return new CadShape{fix.Face()};
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
        if (std::fabs(vol.Mass()) > 1e-12) {
            GProp_PrincipalProps pp = vol.PrincipalProperties();
            pp.Moments(out->inertia[0], out->inertia[1], out->inertia[2]);
            put(out->axes, pp.FirstAxisOfInertia().XYZ());
            put(out->axes + 3, pp.SecondAxisOfInertia().XYZ());
            put(out->axes + 6, pp.ThirdAxisOfInertia().XYZ());
        }
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

int32_t cad_write_step_parts(const CadShape* const* shapes, const char* const* names, const double* colors,
                             int32_t n, uint8_t** out, size_t* len) {
    quiet_messages();
    return guard("escribir STEP", 0, [&] {
        Handle(XCAFApp_Application) app = XCAFApp_Application::GetApplication();
        Handle(TDocStd_Document) doc;
        app->NewDocument("MDTV-XCAF", doc);
        Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
        Handle(XCAFDoc_ColorTool) ct = XCAFDoc_DocumentTool::ColorTool(doc->Main());
        for (int i = 0; i < n; i++) {
            TDF_Label l = st->AddShape(shapes[i]->s, Standard_False);
            TDataStd_Name::Set(l, TCollection_ExtendedString(names[i], Standard_True));
            const double* c = colors + 3 * i;
            if (c[0] >= 0) ct->SetColor(l, Quantity_Color(c[0], c[1], c[2], Quantity_TOC_sRGB), XCAFDoc_ColorGen);
        }
        STEPCAFControl_Writer w;
        w.SetNameMode(Standard_True);
        w.SetColorMode(Standard_True);
        if (!w.Transfer(doc, STEPControl_AsIs)) throw Standard_Failure("no se pudieron traducir las piezas a STEP");
        std::ostringstream os;
        if (w.WriteStream(os) != IFSelect_RetDone) throw Standard_Failure("no se pudo escribir STEP");
        app->Close(doc);
        return give_bytes(os.str(), out, len);
    });
}

int32_t cad_hlr(const CadShape* s, const double* eye, const double* xdir, double deflection, uint8_t** out, size_t* len) {
    return guard("proyectar vista", 0, [&] {
        gp_Ax2 cs(gp::Origin(), gp_Dir(eye[0], eye[1], eye[2]), gp_Dir(xdir[0], xdir[1], xdir[2]));
        HLRAlgo_Projector projector(cs);
        Handle(HLRBRep_Algo) algo = new HLRBRep_Algo();
        algo->Add(s->s);
        algo->Projector(projector);
        algo->Update();
        algo->Hide();
        HLRBRep_HLRToShape hts(algo);
        std::vector<double> data{0.0};
        double lines = 0;
        auto put_kind = [&](const TopoDS_Shape& comp, int kind) {
            if (comp.IsNull()) return;
            for (TopExp_Explorer ex(comp, TopAbs_EDGE); ex.More(); ex.Next()) {
                BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
                GCPnts_TangentialDeflection td(c, 0.1, deflection, 2);
                if (td.NbPoints() < 2) continue;
                data.push_back(kind);
                data.push_back(td.NbPoints());
                for (int i = 1; i <= td.NbPoints(); i++) {
                    gp_Pnt p = td.Value(i);
                    data.push_back(p.X());
                    data.push_back(p.Y());
                }
                lines += 1;
            }
        };
        put_kind(hts.VCompound(), 0);
        put_kind(hts.OutLineVCompound(), 1);
        put_kind(hts.HCompound(), 2);
        put_kind(hts.OutLineHCompound(), 3);
        put_kind(hts.Rg1LineVCompound(), 4);
        data[0] = lines;
        std::string bytes(reinterpret_cast<const char*>(data.data()), data.size() * sizeof(double));
        return give_bytes(bytes, out, len);
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
