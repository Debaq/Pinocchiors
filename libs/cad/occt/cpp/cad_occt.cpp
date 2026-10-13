// Implementación del puente con OpenCASCADE. Ver cad_occt.h.

#define _USE_MATH_DEFINES  // M_PI en MSVC
#include "cad_occt.h"

#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <HLRAlgo_Projector.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <gp_Pln.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_CompCurve.hxx>
#include <GCPnts_UniformAbscissa.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom2d_Line.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <Geom_OffsetSurface.hxx>
#include <GCE2d_MakeSegment.hxx>
#include <BRepFeat_SplitShape.hxx>
#include <BRepProj_Projection.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <TopTools_HSequenceOfShape.hxx>
#include <Geom2d_Curve.hxx>
#include <algorithm>
#include <array>
#include <TColgp_Array1OfPnt2d.hxx>
#include <gp_Ax3.hxx>
#include <BRepLib.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <LocOpe_DPrism.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <ShapeFix_Shell.hxx>
#include <ShapeFix_Solid.hxx>
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
#include <BRepBuilderAPI_Copy.hxx>
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
#include <Geom_BSplineSurface.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom_Plane.hxx>
#include <BRepLib_FindSurface.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BOPAlgo_MakerVolume.hxx>
#include <BRepGProp_Face.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Law_Interpol.hxx>
#include <TopTools_MapOfShape.hxx>
#include <IntCurveSurface_HInter.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <GProp_PrincipalProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <GC_MakeArcOfEllipse.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TColgp_Array1OfVec.hxx>
#include <TColStd_Array1OfInteger.hxx>
#include <TColStd_Array1OfReal.hxx>
#include <TColStd_HArray1OfBoolean.hxx>
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
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

#include <cmath>
#include <cstdlib>
#include <cstring>
#include <set>
#include <memory>
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
        case 6: {
            // B-spline por polos: grado, periódica, n polos, k nudos distintos; polos, pesos, nudos y multiplicidades
            if (count < 4) throw Standard_Failure("b-spline: datos incompletos");
            int deg = (int)d[0];
            bool periodic = d[1] != 0;
            int n = (int)d[2], k = (int)d[3];
            if (n < 2 || k < 2 || count != 4 + 4 * n + 2 * k) throw Standard_Failure("b-spline: cantidades que no cuadran");
            TColgp_Array1OfPnt poles(1, n);
            TColStd_Array1OfReal weights(1, n), knots(1, k);
            TColStd_Array1OfInteger mults(1, k);
            const double* q = d + 4;
            for (int i = 0; i < n; i++) poles.SetValue(i + 1, pnt(q + 3 * i));
            q += 3 * n;
            for (int i = 0; i < n; i++) weights.SetValue(i + 1, q[i]);
            q += n;
            for (int i = 0; i < k; i++) knots.SetValue(i + 1, q[i]);
            q += k;
            for (int i = 0; i < k; i++) mults.SetValue(i + 1, (int)std::lround(q[i]));
            Handle(Geom_BSplineCurve) c = new Geom_BSplineCurve(poles, weights, knots, mults, deg, periodic);
            return BRepBuilderAPI_MakeEdge(c).Edge();
        }
        case 7: {
            // Arco de elipse antihorario (alrededor de la normal) de inicio a fin: centro, normal, eje mayor, a, b, inicio, fin y 1 = recorrido así, 0 = al revés
            if (count != 18) throw Standard_Failure("arco de elipse: se esperaban 18 valores");
            double a = d[9], b = d[10];
            if (a <= 0 || b <= 0) throw Standard_Failure("arco de elipse: radio no positivo");
            gp_Dir n = dir(d + 3);
            gp_Dir x = dir(d + 6);
            if (b > a) {
                x = n.Crossed(x);
                std::swap(a, b);
            }
            gp_Elips e(gp_Ax2(pnt(d), n, x), a, b);
            GC_MakeArcOfEllipse arc(e, pnt(d + 11), pnt(d + 14), Standard_True);
            if (!arc.IsDone()) throw Standard_Failure("arco de elipse: no se pudo armar");
            TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(arc.Value()).Edge();
            // Al revés: la misma curva recorrida del fin al inicio
            if (d[17] == 0) edge.Reverse();
            return edge;
        }
        case 8: {
            // Spline por puntos con tangentes en algunos: periódica, n; puntos y tangentes (cero = libre)
            if (count < 2) throw Standard_Failure("spline: datos incompletos");
            bool periodic = d[0] != 0;
            int n = (int)d[1];
            if (n < 2 || count != 2 + 6 * n) throw Standard_Failure("spline: cantidades que no cuadran");
            Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, n);
            for (int i = 0; i < n; i++) pts->SetValue(i + 1, pnt(d + 2 + 3 * i));
            TColgp_Array1OfVec tangents(1, n);
            Handle(TColStd_HArray1OfBoolean) flags = new TColStd_HArray1OfBoolean(1, n);
            const double* t = d + 2 + 3 * n;
            for (int i = 0; i < n; i++) {
                gp_Vec v(t[3 * i], t[3 * i + 1], t[3 * i + 2]);
                bool on = v.Magnitude() > Precision::Confusion();
                flags->SetValue(i + 1, on);
                tangents.SetValue(i + 1, on ? v : gp_Vec(1, 0, 0));
            }
            GeomAPI_Interpolate interp(pts, periodic ? Standard_True : Standard_False, Precision::Confusion());
            // Solo la dirección: el largo lo ajusta OCCT a la parametrización
            interp.Load(tangents, flags, Standard_True);
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
// Cualquier algoritmo con Generated/Modified/IsDeleted (los de BRepBuilderAPI y BRepOffset_MakeOffset)
template <class Mk>
void record(Mk& mk, const std::vector<TopoDS_Shape>& inputs, const TopoDS_Shape& result,
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

// Espina auxiliar de un barrido con torsión: cada punto del camino corrido un poco hacia un
// vector que no gira solo (marco de rotación mínima, doble reflexión) y que se gira `twist`
// radianes en proporción al largo recorrido. MakePipeShell orienta el perfil hacia ella.
static TopoDS_Wire twist_aux(const TopoDS_Wire& w, double twist) {
    BRepAdaptor_CompCurve c(w);
    const double total = GCPnts_AbscissaPoint::Length(c);
    if (total < 1e-9) throw Standard_Failure("el camino no tiene largo");
    const int n = std::max(64, (int)std::ceil(std::fabs(twist) / (M_PI / 32)) * 4);
    std::vector<gp_Pnt> x(n + 1);
    std::vector<gp_Vec> t(n + 1);
    std::vector<double> s(n + 1);
    for (int i = 0; i <= n; i++) {
        double u = c.FirstParameter();
        if (i == n) u = c.LastParameter();
        else if (i > 0) {
            GCPnts_AbscissaPoint ap(c, total * i / n, c.FirstParameter());
            u = ap.IsDone() ? ap.Parameter() : c.FirstParameter() + (c.LastParameter() - c.FirstParameter()) * i / n;
        }
        gp_Vec d;
        c.D1(u, x[i], d);
        if (d.Magnitude() < 1e-12) throw Standard_Failure("el camino tiene un punto sin dirección");
        t[i] = d.Normalized();
        s[i] = total * i / n;
    }
    // Primer vector: perpendicular a la tangente, hacia el eje más de costado
    gp_Vec ref = std::fabs(t[0].X()) < 0.9 ? gp_Vec(1, 0, 0) : gp_Vec(0, 1, 0);
    gp_Vec r = (ref - t[0] * ref.Dot(t[0])).Normalized();
    const double h = 0.01 * total;
    Handle(TColgp_HArray1OfPnt) pts = new TColgp_HArray1OfPnt(1, n + 1);
    for (int i = 0; i <= n; i++) {
        if (i > 0) {
            gp_Vec v1(x[i - 1], x[i]);
            double c1 = v1.Dot(v1);
            if (c1 > 1e-24) {
                gp_Vec rl = r - v1 * (2 / c1 * v1.Dot(r));
                gp_Vec tl = t[i - 1] - v1 * (2 / c1 * v1.Dot(t[i - 1]));
                gp_Vec v2 = t[i] - tl;
                double c2 = v2.Dot(v2);
                r = c2 > 1e-24 ? rl - v2 * (2 / c2 * v2.Dot(rl)) : rl;
            }
            r = (r - t[i] * r.Dot(t[i])).Normalized();
        }
        double a = twist * s[i] / total;
        gp_Vec d = r * std::cos(a) + t[i].Crossed(r) * std::sin(a);
        pts->SetValue(i + 1, x[i].Translated(d * h));
    }
    GeomAPI_Interpolate in(pts, Standard_False, 1e-9);
    in.Perform();
    if (!in.IsDone()) throw Standard_Failure("no se pudo armar la torsión");
    return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(in.Curve()).Edge()).Wire();
}

static CadShape* pipe_impl(const CadShape* profile, const CadShape* spine, int32_t mode, double twist, int32_t solid) {
    return guard("barrer", (CadShape*)nullptr, [&] {
        TopoDS_Wire w;
        if (spine->s.ShapeType() == TopAbs_WIRE) {
            w = TopoDS::Wire(spine->s);
        } else if (spine->s.ShapeType() == TopAbs_EDGE) {
            w = BRepBuilderAPI_MakeWire(TopoDS::Edge(spine->s)).Wire();
        } else {
            throw Standard_Failure("la trayectoria debe ser un alambre");
        }
        // Orientación del perfil: 0 sigue el camino (Frenet corregido), 1 Frenet, 2 fija (no gira),
        // 3 con torsión (hacia la espina auxiliar)
        TopoDS_Wire aux;
        if (mode == 3) aux = twist_aux(w, twist);
        auto setup = [&](BRepOffsetAPI_MakePipeShell& mk) {
            if (mode == 3) {
                // El punto de la auxiliar es el que cae en el plano normal al camino (por largo
                // relativo se desfasa en las curvas y el perfil se inclina)
                mk.SetMode(aux, Standard_False, BRepFill_NoContact);
            } else if (mode == 1) {
                mk.SetMode(Standard_True);
            } else if (mode == 2) {
                BRepAdaptor_CompCurve c(w);
                gp_Pnt p;
                gp_Vec t;
                c.D1(c.FirstParameter(), p, t);
                mk.SetMode(gp_Ax2(p, gp_Dir(t)));
            } else {
                mk.SetMode(Standard_False);
            }
            mk.SetTransitionMode(BRepBuilderAPI_RightCorner);
        };
        auto sweep_wire = [&](const TopoDS_Wire& section, bool make_solid) {
            BRepOffsetAPI_MakePipeShell mk(w);
            setup(mk);
            mk.Add(section, Standard_False, Standard_False);
            mk.Build();
            if (!mk.IsDone() && mode == 0) {
                // Frenet corregido falla en tramos rectos: el triedro discreto sigue el camino igual
                mk.SetDiscreteMode();
                mk.Build();
            }
            if (!mk.IsDone()) throw Standard_Failure("no se pudo barrer el perfil por ese camino");
            if (make_solid && !mk.MakeSolid()) throw Standard_Failure("el barrido no cierra un sólido");
            return mk.Shape();
        };
        TopExp_Explorer fx(profile->s, TopAbs_FACE);
        if (fx.More() && solid != 0) {
            // Caras: el contorno de afuera menos los agujeros, cada cara
            TopoDS_Shape result;
            bool first = true;
            for (; fx.More(); fx.Next()) {
                TopoDS_Face face = TopoDS::Face(fx.Current());
                TopoDS_Wire outer = BRepTools::OuterWire(face);
                TopoDS_Shape r = sweep_wire(outer, true);
                for (TopExp_Explorer wx(face, TopAbs_WIRE); wx.More(); wx.Next()) {
                    TopoDS_Wire inner = TopoDS::Wire(wx.Current());
                    if (inner.IsSame(outer)) continue;
                    r = BRepAlgoAPI_Cut(r, sweep_wire(inner, true)).Shape();
                }
                result = first ? r : BRepAlgoAPI_Fuse(result, r).Shape();
                first = false;
            }
            return wrap_checked(result, "barrido");
        }
        // Alambres (superficie por barrido): cada uno da una superficie
        TopoDS_Compound c;
        BRep_Builder b;
        b.MakeCompound(c);
        int count = 0;
        for (TopExp_Explorer wx(profile->s, TopAbs_WIRE); wx.More(); wx.Next(), count++) b.Add(c, sweep_wire(TopoDS::Wire(wx.Current()), false));
        if (count == 0) {
            for (TopExp_Explorer ex(profile->s, TopAbs_EDGE); ex.More(); ex.Next(), count++)
                b.Add(c, sweep_wire(BRepBuilderAPI_MakeWire(TopoDS::Edge(ex.Current())).Wire(), false));
        }
        if (count == 0) throw Standard_Failure("el perfil no tiene curvas");
        return wrap_checked(c, "barrido");
    });
}

CadShape* cad_pipe_ex(const CadShape* profile, const CadShape* spine, int32_t mode, int32_t solid) {
    return pipe_impl(profile, spine, mode, 0.0, solid);
}

CadShape* cad_pipe_twist(const CadShape* profile, const CadShape* spine, double twist, int32_t solid) {
    return pipe_impl(profile, spine, 3, twist, solid);
}

// El alambre de una sección o un camino: el alambre mismo, una arista, el contorno de una cara
// o el primer alambre de un compuesto
static TopoDS_Wire wire_of(const TopoDS_Shape& s) {
    if (s.ShapeType() == TopAbs_WIRE) return TopoDS::Wire(s);
    if (s.ShapeType() == TopAbs_EDGE) return BRepBuilderAPI_MakeWire(TopoDS::Edge(s)).Wire();
    if (s.ShapeType() == TopAbs_FACE) return BRepTools::OuterWire(TopoDS::Face(s));
    TopExp_Explorer fx(s, TopAbs_FACE);
    if (fx.More()) return BRepTools::OuterWire(TopoDS::Face(fx.Current()));
    TopExp_Explorer wx(s, TopAbs_WIRE);
    if (wx.More()) return TopoDS::Wire(wx.Current());
    throw Standard_Failure("se esperaba una curva o un perfil");
}

CadShape* cad_loft_ex(const CadShape* const* wires, int32_t n, int32_t solid, int32_t ruled, const double* start, const double* end,
                      int32_t closed) {
    return guard("transición", (CadShape*)nullptr, [&] {
        if (n + (start ? 1 : 0) + (end ? 1 : 0) < 2) throw Standard_Failure("se necesitan al menos 2 secciones");
        BRepOffsetAPI_ThruSections mk(solid != 0, ruled != 0);
        if (start) mk.AddVertex(BRepBuilderAPI_MakeVertex(gp_Pnt(start[0], start[1], start[2])).Vertex());
        for (int32_t i = 0; i < n; i++) mk.AddWire(wire_of(wires[i]->s));
        // Cerrada: vuelve a la primera (unión sin tangencia en la costura)
        if (closed && n > 0) mk.AddWire(wire_of(wires[0]->s));
        if (end) mk.AddVertex(BRepBuilderAPI_MakeVertex(gp_Pnt(end[0], end[1], end[2])).Vertex());
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo hacer la transición");
        return wrap_checked(mk.Shape(), "transición");
    });
}

// Escala del perfil a lo largo del camino para que siga tocando la guía: en cada fracción del
// largo del camino (el dominio con que OCCT recorre la ley), la distancia del camino a la guía
// en el plano normal, dividida por la del arranque
static Handle(Law_Function) guide_scale(const TopoDS_Wire& spine, const TopoDS_Wire& guide) {
    BRepAdaptor_CompCurve sc(spine);
    Handle(BRepAdaptor_CompCurve) gh = new BRepAdaptor_CompCurve(guide);
    const double len = GCPnts_AbscissaPoint::Length(sc);
    if (len < Precision::Confusion()) throw Standard_Failure("el camino no tiene largo");
    const int N = 32;
    TColgp_Array1OfPnt2d law(1, N + 1);
    double r0 = 0;
    gp_Pnt last;
    for (int k = 0; k <= N; k++) {
        const double frac = double(k) / N;
        GCPnts_AbscissaPoint ap(sc, frac * len, sc.FirstParameter());
        const double t = ap.IsDone() ? ap.Parameter() : sc.FirstParameter() + frac * (sc.LastParameter() - sc.FirstParameter());
        gp_Pnt p;
        gp_Vec d;
        sc.D1(t, p, d);
        if (d.Magnitude() < 1e-12) throw Standard_Failure("el camino tiene un punto sin dirección");
        // Cruce de la guía con el plano normal; el más cercano al anterior (o al camino al empezar)
        Handle(GeomAdaptor_Surface) pl = new GeomAdaptor_Surface(new Geom_Plane(p, gp_Dir(d)));
        IntCurveSurface_HInter in;
        in.Perform(gh, pl);
        const gp_Pnt ref = k == 0 ? p : last;
        bool found = false;
        gp_Pnt g;
        for (int j = 1; in.IsDone() && j <= in.NbPoints(); j++) {
            gp_Pnt q = in.Point(j).Pnt();
            if (!found || q.Distance(ref) < g.Distance(ref)) g = q;
            found = true;
        }
        if (!found) {
            // La guía no llega a ese plano: su punto más cercano
            BRepExtrema_DistShapeShape ext(BRepBuilderAPI_MakeVertex(p).Vertex(), guide);
            if (!ext.IsDone() || ext.NbSolution() < 1) throw Standard_Failure("la guía no acompaña al camino");
            g = ext.PointOnShape2(1);
        }
        last = g;
        const double r = g.Distance(p);
        if (k == 0) {
            if (r < Precision::Confusion()) throw Standard_Failure("la guía arranca sobre el camino");
            r0 = r;
        }
        law.SetValue(k + 1, gp_Pnt2d(frac, std::max(r / r0, 1e-6)));
    }
    Handle(Law_Interpol) l = new Law_Interpol();
    l->Set(law, Standard_False);
    return l;
}

CadShape* cad_loft_pipe(const CadShape* const* wires, int32_t n, const CadShape* spine, const CadShape* guide, int32_t solid) {
    return guard("transición por un camino", (CadShape*)nullptr, [&] {
        if (n < 1) throw Standard_Failure("se necesita al menos una sección");
        BRepOffsetAPI_MakePipeShell mk(wire_of(spine->s));
        if (guide && n == 1) {
            // Con curva guía, el perfil la sigue: gira con ella y se escala para seguir tocándola.
            // El contacto automático de OCCT (BRepFill_ContactOnBorder) falla aun en casos simples,
            // así que la guía solo orienta y la escala va como ley aparte: la distancia del camino
            // a la guía en el plano normal, relativa a la del arranque
            mk.SetMode(wire_of(guide->s), Standard_True, BRepFill_NoContact);
            mk.SetLaw(wire_of(wires[0]->s), guide_scale(wire_of(spine->s), wire_of(guide->s)), Standard_False, Standard_False);
        } else {
            if (guide) mk.SetMode(wire_of(guide->s), Standard_True, BRepFill_ContactOnBorder);
            else mk.SetDiscreteMode();
            for (int32_t i = 0; i < n; i++) mk.Add(wire_of(wires[i]->s), Standard_False, Standard_False);
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo pasar por las secciones a lo largo del camino");
        if (solid && !mk.MakeSolid()) throw Standard_Failure("la transición no cierra un sólido");
        return wrap_checked(mk.Shape(), "transición por un camino");
    });
}

namespace hermite {

// Las aristas de un alambre en el orden y el sentido en que se recorre, como curvas
struct Side {
    Handle(Geom_Curve) c;
    double f, l;
    bool reversed;
    gp_Pnt at(double s) const { return c->Value(reversed ? l - (l - f) * s : f + (l - f) * s); }
};

static std::vector<Side> sides_of(const TopoDS_Wire& w) {
    std::vector<Side> out;
    for (BRepTools_WireExplorer x(w); x.More(); x.Next()) {
        double f, l;
        Handle(Geom_Curve) c = BRep_Tool::Curve(x.Current(), f, l);
        if (c.IsNull()) throw Standard_Failure("una arista de la sección no tiene curva");
        out.push_back({c, f, l, x.Current().Orientation() == TopAbs_REVERSED});
    }
    return out;
}

// La sección `b` empezando en la arista `shift` y recorrida al derecho o al revés
static std::vector<Side> rotated(const std::vector<Side>& b, size_t shift, bool backwards) {
    std::vector<Side> out;
    size_t n = b.size();
    for (size_t k = 0; k < n; k++) {
        if (!backwards) out.push_back(b[(shift + k) % n]);
        else {
            Side s = b[(shift + n - k) % n];
            s.reversed = !s.reversed;
            out.push_back(s);
        }
    }
    return out;
}

}  // namespace hermite

// Transición con tangencia en las puntas: la superficie se arma a mano (no hay en OCCT un loft
// con condiciones de borde). Cada lado es un parche B-spline: a lo largo de la arista, la arista
// interpolada en M puntos por fracción de largo (las mismas fracciones en todas las secciones, así
// las curvas son compatibles); entre secciones, Hermite cúbica (Catmull-Rom adentro). En una punta
// con peso > 0 la tangente es la normal de la sección × peso × distancia a la vecina: la
// superficie sale perpendicular a la sección; con peso ≤ 0, la diferencia con la vecina (libre).
// Las secciones tienen que tener la misma cantidad de aristas.
CadShape* cad_loft_tangent(const CadShape* const* wires, int32_t n, double start_w, double end_w, int32_t solid) {
    return guard("transición con tangencia", (CadShape*)nullptr, [&] {
        if (n < 2) throw Standard_Failure("se necesitan al menos 2 secciones");
        std::vector<std::vector<hermite::Side>> secs;
        std::vector<TopoDS_Wire> ws;
        std::vector<gp_Pnt> centers;
        std::vector<gp_Vec> normals;
        for (int32_t i = 0; i < n; i++) {
            TopoDS_Wire w = wire_of(wires[i]->s);
            ws.push_back(w);
            secs.push_back(hermite::sides_of(w));
            GProp_GProps g;
            BRepGProp::LinearProperties(w, g);
            centers.push_back(g.CentreOfMass());
            gp_Vec nrm(0, 0, 0);
            BRepLib_FindSurface fs(w, 1e-6, Standard_True);
            if (fs.Found()) {
                Handle(Geom_Plane) pl = Handle(Geom_Plane)::DownCast(fs.Surface());
                if (!pl.IsNull()) nrm = gp_Vec(pl->Axis().Direction());
            }
            normals.push_back(nrm);
        }
        // Secciones de una sola arista cerrada (un círculo): en dos mitades, si no el lado sería
        // una sola cara pegada a sí misma
        bool all_single = true;
        for (auto& sc : secs) all_single = all_single && sc.size() == 1;
        if (all_single)
            for (auto& sc : secs) {
                hermite::Side sd = sc[0];
                double mid = (sd.f + sd.l) / 2;
                hermite::Side lo{sd.c, sd.f, mid, sd.reversed}, hi{sd.c, mid, sd.l, sd.reversed};
                sc = sd.reversed ? std::vector<hermite::Side>{hi, lo} : std::vector<hermite::Side>{lo, hi};
            }
        const size_t K = secs[0].size();
        for (auto& sc : secs)
            if (sc.size() != K) throw Standard_Failure("para la tangencia las secciones tienen que tener la misma cantidad de lados");
        // Cada sección alineada con la anterior: la arista de arranque y el sentido que dejan sus
        // vértices más cerca (medidos desde el centro de cada una)
        for (int32_t i = 1; i < n; i++) {
            double best = 1e300;
            std::vector<hermite::Side> pick;
            for (int dir = 0; dir < 2; dir++)
                for (size_t sh = 0; sh < K; sh++) {
                    auto cand = hermite::rotated(secs[i], sh, dir == 1);
                    double sum = 0;
                    for (size_t k = 0; k < K; k++) {
                        gp_Vec pa(centers[i - 1], secs[i - 1][k].at(0)), pb(centers[i], cand[k].at(0));
                        sum += (pa - pb).SquareMagnitude();
                        gp_Vec ma(centers[i - 1], secs[i - 1][k].at(0.5)), mb(centers[i], cand[k].at(0.5));
                        sum += (ma - mb).SquareMagnitude();
                    }
                    if (sum < best) {
                        best = sum;
                        pick = cand;
                    }
                }
            secs[i] = pick;
        }
        // Tangentes de las puntas
        auto end_dir = [&](int i, int toward, double w) {
            if (normals[i].Magnitude() < 0.5) throw Standard_Failure("para la tangencia la sección de la punta tiene que ser plana");
            gp_Vec d(centers[i], centers[toward]);
            gp_Vec fwd = toward > i ? d : d.Reversed();
            gp_Vec t = normals[i];
            if (t.Dot(fwd) < 0) t.Reverse();
            return t * (w * d.Magnitude());
        };
        const bool fixed0 = start_w > 0, fixed1 = end_w > 0;
        const gp_Vec T0 = fixed0 ? end_dir(0, 1, start_w) : gp_Vec(), T1 = fixed1 ? end_dir(n - 1, n - 2, end_w) : gp_Vec();

        // Puntos de cada arista por fracción de su largo
        const int M = 25;
        TColStd_Array1OfReal params(1, M);
        for (int m = 0; m < M; m++) params(m + 1) = double(m) / (M - 1);
        auto samples = [&](const hermite::Side& sd) {
            GeomAdaptor_Curve ac(sd.c, sd.f, sd.l);
            double len = GCPnts_AbscissaPoint::Length(ac, sd.f, sd.l);
            std::vector<gp_Pnt> pts(M);
            for (int m = 0; m < M; m++) {
                double s = double(m) / (M - 1);
                double u;
                if (m == 0) u = sd.reversed ? sd.l : sd.f;
                else if (m == M - 1) u = sd.reversed ? sd.f : sd.l;
                else {
                    GCPnts_AbscissaPoint ap(ac, (sd.reversed ? -1 : 1) * len * s, sd.reversed ? sd.l : sd.f);
                    u = ap.IsDone() ? ap.Parameter() : (sd.reversed ? sd.l - (sd.l - sd.f) * s : sd.f + (sd.l - sd.f) * s);
                }
                pts[m] = sd.c->Value(u);
            }
            return pts;
        };
        // Curva por esos puntos con los parámetros fijos: mismos nudos en todas → compatibles
        auto interp = [&](const std::vector<gp_Pnt>& pts) {
            Handle(TColgp_HArray1OfPnt) h = new TColgp_HArray1OfPnt(1, M);
            for (int m = 0; m < M; m++) h->SetValue(m + 1, pts[m]);
            Handle(TColStd_HArray1OfReal) pr = new TColStd_HArray1OfReal(1, M);
            for (int m = 1; m <= M; m++) pr->SetValue(m, params(m));
            GeomAPI_Interpolate in(h, pr, Standard_False, 1e-9);
            in.Perform();
            if (!in.IsDone()) throw Standard_Failure("no se pudo interpolar una arista de la sección");
            return in.Curve();
        };

        BRepBuilderAPI_Sewing sew(1e-6 * std::max(1.0, centers[0].Distance(centers[n - 1])));
        for (size_t k = 0; k < K; k++) {
            std::vector<Handle(Geom_BSplineCurve)> cs;
            for (int32_t i = 0; i < n; i++) cs.push_back(interp(samples(secs[i][k])));
            const int NP = cs[0]->NbPoles();
            auto pole = [&](int i, int p) { return gp_Vec(cs[i]->Pole(p).XYZ()); };
            // Tangente (por polo) en cada sección
            auto tangent = [&](int i, int p) {
                if (i == 0) return fixed0 ? T0 : pole(1, p) - pole(0, p);
                if (i == n - 1) return fixed1 ? T1 : pole(n - 1, p) - pole(n - 2, p);
                return (pole(i + 1, p) - pole(i - 1, p)) * 0.5;
            };
            const int NV = 3 * (n - 1) + 1;
            TColgp_Array2OfPnt poles(1, NP, 1, NV);
            for (int p = 1; p <= NP; p++)
                for (int i = 0; i < n; i++) {
                    poles(p, 3 * i + 1) = gp_Pnt(pole(i, p).XYZ());
                    if (i + 1 < n) {
                        poles(p, 3 * i + 2) = gp_Pnt((pole(i, p) + tangent(i, p) / 3).XYZ());
                        poles(p, 3 * i + 3) = gp_Pnt((pole(i + 1, p) - tangent(i + 1, p) / 3).XYZ());
                    }
                }
            TColStd_Array1OfReal uk(1, cs[0]->NbKnots()), vk(1, n);
            TColStd_Array1OfInteger um(1, cs[0]->NbKnots()), vm(1, n);
            cs[0]->Knots(uk);
            cs[0]->Multiplicities(um);
            for (int i = 1; i <= n; i++) {
                vk(i) = i - 1;
                vm(i) = (i == 1 || i == n) ? 4 : 3;
            }
            Handle(Geom_BSplineSurface) srf = new Geom_BSplineSurface(poles, uk, vk, um, vm, cs[0]->Degree(), 3);
            BRepBuilderAPI_MakeFace mf(srf, 1e-7);
            if (!mf.IsDone()) throw Standard_Failure("no se pudo armar un lado de la transición");
            sew.Add(mf.Face());
        }
        // Tapas: las secciones planas
        if (solid) {
            for (int i : {0, n - 1}) {
                BRepBuilderAPI_MakeFace cap(ws[i], Standard_True);
                if (!cap.IsDone()) throw Standard_Failure("las secciones de las puntas tienen que ser planas para cerrar el sólido");
                sew.Add(cap.Face());
            }
        }
        sew.Perform();
        TopoDS_Shape sewn = sew.SewedShape();
        if (!solid) return wrap(sewn);
        TopExp_Explorer sx(sewn, TopAbs_SHELL);
        if (!sx.More()) throw Standard_Failure("la transición no cierra");
        BRepBuilderAPI_MakeSolid ms(TopoDS::Shell(sx.Current()));
        if (!ms.IsDone()) throw Standard_Failure("la transición no cierra un sólido");
        TopoDS_Solid so = ms.Solid();
        BRepLib::OrientClosedSolid(so);
        return wrap_checked(so, "transición con tangencia");
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

CadShape* cad_shell_faces(const CadShape* s, const int32_t* faces, int32_t n, double thickness, const int32_t* ov,
                          const double* ov_thickness, int32_t n_ov) {
    return guard("cáscara", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        auto face = [&](int32_t i) {
            if (i < 0 || i >= m.Extent()) throw Standard_Failure("cara inexistente");
            return TopoDS::Face(m(i + 1));
        };
        if (n == 0) throw Standard_Failure("elegir al menos una cara para abrir");
        BRepOffset_MakeOffset mk;
        mk.Initialize(s->s, thickness, 1e-3, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Arc, Standard_False);
        for (int32_t i = 0; i < n; i++) mk.AddFace(face(faces[i]));
        // Las caras con otro espesor
        for (int32_t i = 0; i < n_ov; i++) mk.SetOffsetOnFace(face(ov[i]), ov_thickness[i]);
        mk.MakeThickSolid();
        if (!mk.IsDone()) throw Standard_Failure("grosor incompatible con la forma");
        record(mk, {s->s}, mk.Shape());
        return wrap_checked(mk.Shape(), "cáscara");
    });
}

// Las aristas de las formas encadenadas en alambres (uno, o un compuesto si quedan sueltos)
static TopoDS_Shape chain_edges(const std::vector<TopoDS_Shape>& shapes) {
    Handle(TopTools_HSequenceOfShape) edges = new TopTools_HSequenceOfShape();
    for (const TopoDS_Shape& s : shapes)
        for (TopExp_Explorer ex(s, TopAbs_EDGE); ex.More(); ex.Next()) edges->Append(ex.Current());
    if (edges->IsEmpty()) throw Standard_Failure("no hay curvas");
    Handle(TopTools_HSequenceOfShape) wires;
    ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, 1e-4, Standard_False, wires);
    if (wires.IsNull() || wires->IsEmpty()) throw Standard_Failure("las curvas no se pudieron encadenar");
    if (wires->Length() == 1) return wires->Value(1);
    TopoDS_Compound c;
    BRep_Builder b;
    b.MakeCompound(c);
    for (int i = 1; i <= wires->Length(); i++) b.Add(c, wires->Value(i));
    return c;
}

CadShape* cad_chain(const CadShape* const* shapes, int32_t n) {
    return guard("encadenar curvas", (CadShape*)nullptr, [&] {
        std::vector<TopoDS_Shape> v;
        for (int32_t i = 0; i < n; i++) v.push_back(shapes[i]->s);
        return wrap(chain_edges(v));
    });
}

CadShape* cad_project_curves(const CadShape* curves, const CadShape* target, const double* direction) {
    return guard("proyectar curvas", (CadShape*)nullptr, [&] {
        BRepProj_Projection proj(curves->s, target->s, gp_Dir(direction[0], direction[1], direction[2]));
        if (!proj.IsDone()) throw Standard_Failure("las curvas no caen sobre esa cara");
        TopoDS_Shape r = proj.Shape();
        if (!TopExp_Explorer(r, TopAbs_EDGE).More()) throw Standard_Failure("las curvas no caen sobre esa cara");
        return wrap(chain_edges({r}));
    });
}

CadShape* cad_intersect_curves(const CadShape* a, const CadShape* b) {
    return guard("cruce de caras", (CadShape*)nullptr, [&] {
        BRepAlgoAPI_Section sec(a->s, b->s, Standard_False);
        sec.ComputePCurveOn1(Standard_True);
        sec.Approximation(Standard_True);
        sec.Build();
        if (!sec.IsDone()) throw Standard_Failure("no se pudo calcular el cruce");
        if (!TopExp_Explorer(sec.Shape(), TopAbs_EDGE).More()) throw Standard_Failure("las caras no se cruzan");
        return wrap(chain_edges({sec.Shape()}));
    });
}

// ---------------------------------------------------------------------------
// Redondeo de cara: una bola de radio r que rueda tocando los dos grupos de
// caras. Su centro corre por el cruce de las caras desfasadas r (del lado de
// adentro en un canto convexo, de afuera en un rincón cóncavo); la superficie
// del redondeo es el tubo de radio r alrededor de ese camino. Lo que cambia es
// la cuña entre el centro y los puntos de contacto menos el tubo: se resta en
// el convexo y se suma en el cóncavo.
// ---------------------------------------------------------------------------
namespace blend {

// Caras desfasadas `off` (hacia afuera con `off` > 0), extendidas para que el cruce no se corte antes
static TopoDS_Shape offset_faces(const std::vector<TopoDS_Face>& faces, double off, double extend) {
    TopoDS_Compound c;
    BRep_Builder b;
    b.MakeCompound(c);
    for (const TopoDS_Face& f : faces) {
        TopoDS_Face ext;
        if (extend > 0) BRepLib::ExtendFace(f, extend, Standard_True, Standard_True, Standard_True, Standard_True, ext);
        if (ext.IsNull()) ext = f;
        Handle(Geom_Surface) s = BRep_Tool::Surface(ext);
        double u0, u1, v0, v1;
        BRepTools::UVBounds(ext, u0, u1, v0, v1);
        double sign = f.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
        Handle(Geom_OffsetSurface) os = new Geom_OffsetSurface(s, off * sign);
        BRepBuilderAPI_MakeFace mf(os, u0, u1, v0, v1, 1e-7);
        if (mf.IsDone()) b.Add(c, mf.Face());
    }
    return c;
}

static TopoDS_Shape extended(const std::vector<TopoDS_Face>& faces, double extend) {
    return offset_faces(faces, 0.0, extend);
}

// Distancia y punto más cercano de `p` a la forma
static bool nearest(const gp_Pnt& p, const TopoDS_Shape& s, double& d, gp_Pnt& q) {
    BRepExtrema_DistShapeShape ds(BRepBuilderAPI_MakeVertex(p).Vertex(), s);
    if (!ds.IsDone() || ds.NbSolution() < 1) return false;
    d = ds.Value();
    q = ds.PointOnShape2(1);
    return true;
}

// ¿La bola de centro `c` toca la forma (sus caras) con el pie adentro de una cara, a distancia r?
// En el borde de la cara el pie pasa a una arista: ahí deja de tocar (es un corte exacto).
static bool touches(const gp_Pnt& c, const TopoDS_Shape& s, double r, double tol) {
    BRepExtrema_DistShapeShape ds(BRepBuilderAPI_MakeVertex(c).Vertex(), s);
    if (!ds.IsDone() || std::fabs(ds.Value() - r) >= tol) return false;
    for (int i = 1; i <= ds.NbSolution(); i++) {
        if (ds.SupportTypeShape2(i) == BRepExtrema_IsInFace) return true;
        // Sobre una arista que no es borde del grupo: la costura de una cara cerrada
        // (un cilindro) o una arista entre dos caras del grupo
        if (ds.SupportTypeShape2(i) == BRepExtrema_IsOnEdge) {
            TopoDS_Edge e = TopoDS::Edge(ds.SupportOnShape2(i));
            int owners = 0;
            for (TopExp_Explorer fx(s, TopAbs_FACE); fx.More(); fx.Next()) {
                TopoDS_Face f = TopoDS::Face(fx.Current());
                for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
                    if (!ex.Current().IsSame(e)) continue;
                    if (BRep_Tool::IsClosed(e, f)) return true;
                    owners++;
                    break;
                }
            }
            if (owners >= 2) return true;
        }
    }
    return false;
}

// El tramo [ta, tb] (parámetros de la curva compuesta `cc`) del alambre `w`, con sus aristas
static TopoDS_Wire trim(const TopoDS_Wire& w, const BRepAdaptor_CompCurve& cc, double ta, double tb) {
    double gap = 1e-9 * (cc.LastParameter() - cc.FirstParameter());
    if (ta <= cc.FirstParameter() + gap && tb >= cc.LastParameter() - gap) return w;
    TopoDS_Edge ea, eb;
    double ua, ub;
    cc.Edge(ta, ea, ua);
    cc.Edge(tb, eb, ub);
    BRepBuilderAPI_MakeWire mw;
    bool on = false;
    for (BRepTools_WireExplorer x(w); x.More(); x.Next()) {
        TopoDS_Edge e = x.Current();
        bool first = e.IsSame(ea), last = e.IsSame(eb);
        if (first) on = true;
        if (!on) continue;
        double f, l;
        Handle(Geom_Curve) c = BRep_Tool::Curve(e, f, l);
        bool fwd = e.Orientation() != TopAbs_REVERSED;
        double lo = f, hi = l;
        // Recorriendo el alambre la arista va de f a l (o al revés si está dada vuelta)
        if (first) (fwd ? lo : hi) = ua;
        if (last) (fwd ? hi : lo) = ub;
        if (hi - lo > 1e-12) {
            TopoDS_Edge ne = BRepBuilderAPI_MakeEdge(c, lo, hi).Edge();
            ne.Orientation(e.Orientation());
            mw.Add(ne);
        }
        if (last) break;
    }
    if (!mw.IsDone()) throw Standard_Failure("no se pudo recortar el camino del centro");
    return mw.Wire();
}

}  // namespace blend

CadShape* cad_face_blend(const CadShape* s, const int32_t* fa, int32_t na, const int32_t* fb, int32_t nb, double r, int32_t side) {
    return guard("redondeo de cara", (CadShape*)nullptr, [&] {
        if (r <= 0) throw Standard_Failure("el radio tiene que ser mayor que cero");
        auto m = map_of(s->s, TopAbs_FACE);
        auto take = [&](const int32_t* idx, int32_t n) {
            std::vector<TopoDS_Face> v;
            for (int32_t i = 0; i < n; i++) {
                if (idx[i] < 0 || idx[i] >= m.Extent()) throw Standard_Failure("cara inexistente");
                v.push_back(TopoDS::Face(m(idx[i] + 1)));
            }
            if (v.empty()) throw Standard_Failure("elegir caras de los dos lados");
            return v;
        };
        std::vector<TopoDS_Face> A = take(fa, na), B = take(fb, nb);
        TopoDS_Compound ca, cb;
        BRep_Builder bb;
        bb.MakeCompound(ca);
        bb.MakeCompound(cb);
        for (auto& f : A) bb.Add(ca, f);
        for (auto& f : B) bb.Add(cb, f);
        Bnd_Box box;
        BRepBndLib::Add(s->s, box);
        double size = std::sqrt(box.SquareExtent());
        // Una cara sola se extiende bastante (el cruce no se corta antes de tiempo); en un
        // grupo de varias, la extensión de una se metería delante de la vecina (un plano
        // tangente a un cilindro) y el cruce saldría en tramos que no empalman: sin extender
        double extend_wide = 3 * r + 0.05 * size;
        auto extend_of = [&](const std::vector<TopoDS_Face>& g) { return g.size() > 1 ? 0.0 : extend_wide; };
        double tol = std::max(1e-4, r * 1e-3);

        // El camino del centro: el lado (adentro o afuera) cuyo cruce da una bola que toca las dos caras de verdad
        TopoDS_Wire spine;
        int best_valid = 0;
        double best_sign = 0;
        std::vector<double> signs;
        if (side == 1) signs = {-1.0};
        else if (side == 2) signs = {1.0};
        else signs = {-1.0, 1.0};
        const int N = 160;
        std::vector<gp_Pnt> centers;
        std::vector<bool> valid;
        for (double sign : signs) {
            TopoDS_Shape oa = blend::offset_faces(A, sign * r, extend_of(A)), ob = blend::offset_faces(B, sign * r, extend_of(B));
            BRepAlgoAPI_Section sec(oa, ob, Standard_False);
            sec.Approximation(Standard_True);
            sec.Build();
            if (!sec.IsDone() || !TopExp_Explorer(sec.Shape(), TopAbs_EDGE).More()) continue;
            TopoDS_Shape chained = chain_edges({sec.Shape()});
            for (TopExp_Explorer wx(chained, TopAbs_WIRE); wx.More(); wx.Next()) {
                TopoDS_Wire w = TopoDS::Wire(wx.Current());
                BRepAdaptor_CompCurve cc(w);
                std::vector<gp_Pnt> pts;
                std::vector<bool> ok;
                int count = 0;
                for (int k = 0; k <= N; k++) {
                    gp_Pnt c = cc.Value(cc.FirstParameter() + (cc.LastParameter() - cc.FirstParameter()) * k / N);
                    bool good = blend::touches(c, ca, r, tol) && blend::touches(c, cb, r, tol);
                    pts.push_back(c);
                    ok.push_back(good);
                    count += good;
                }
                if (count > best_valid) {
                    best_valid = count;
                    best_sign = sign;
                    spine = w;
                    centers = pts;
                    valid = ok;
                }
            }
            if (best_valid > 0) break;
        }
        if (best_valid < 2) throw Standard_Failure("la bola de ese radio no toca las dos caras a la vez (radio demasiado grande o caras que no se enfrentan)");

        // El tramo donde toca las dos: sus puntas exactas por bisección entre la última muestra
        // que toca y la primera que no
        BRepAdaptor_CompCurve cc(spine);
        double s0 = cc.FirstParameter(), s1 = cc.LastParameter();
        auto par = [&](int k) { return s0 + (s1 - s0) * k / N; };
        auto ok = [&](double t) { gp_Pnt c = cc.Value(t); return blend::touches(c, ca, r, tol) && blend::touches(c, cb, r, tol); };
        int k0 = 0, k1 = N;
        while (k0 < N && !valid[k0]) k0++;
        while (k1 > 0 && !valid[k1]) k1--;
        if (k1 <= k0) throw Standard_Failure("el redondeo quedó demasiado corto");
        auto edge = [&](double in, double out) {
            for (int it = 0; it < 50; it++) {
                double mid = (in + out) / 2;
                (ok(mid) ? in : out) = mid;
            }
            return in;
        };
        double ta = k0 > 0 ? edge(par(k0), par(k0 - 1)) : s0;
        double tb = k1 < N ? edge(par(k1), par(k1 + 1)) : s1;
        // Camino cerrado que toca en toda la vuelta (alrededor de un cilindro): anillo
        bool ring = k0 == 0 && k1 == N && cc.Value(s0).Distance(cc.Value(s1)) < tol;
        if (!ring && k0 <= 1 && k1 >= N - 1 && cc.Value(s0).Distance(cc.Value(s1)) < tol) ring = true;
        if (ring) {
            ta = s0;
            tb = s1;
        } else if (best_sign < 0) {
            // En el canto convexo un poco de más en las puntas (afuera no hay material que
            // cortar): evita caras que coinciden justo en los extremos
            double pad = 0.02 * (tb - ta);
            ta = std::max(s0, ta - pad);
            tb = std::min(s1, tb + pad);
        }
        TopoDS_Shape ea = blend::extended(A, extend_of(A)), eb = blend::extended(B, extend_of(B));

        // Los tramos: entre las juntas del camino donde cambia la curvatura (de recta a arco en
        // una cadena tangente); un loft suave que cruza una junta se abomba. Un anillo de una
        // sola curva va en cuatro tramos: un loft cerrado sobre sí mismo sale retorcido
        std::vector<double> breaks{ta};
        {
            int ni = cc.NbIntervals(GeomAbs_C1);
            TColStd_Array1OfReal iv(1, ni + 1);
            cc.Intervals(iv, GeomAbs_C1);
            double gap = 1e-6 * (s1 - s0);
            for (int i = 1; i <= ni + 1; i++)
                if (iv(i) > ta + gap && iv(i) < tb - gap) breaks.push_back(iv(i));
        }
        breaks.push_back(tb);
        const double total = GCPnts_AbscissaPoint::Length(cc, ta, tb);
        if (ring && breaks.size() == 2) {
            breaks = {ta};
            for (int q = 1; q < 4; q++) {
                GCPnts_AbscissaPoint ap(cc, total * q / 4, ta);
                breaks.push_back(ap.IsDone() ? ap.Parameter() : ta + (tb - ta) * q / 4);
            }
            breaks.push_back(tb);
        }

        // Centro, contactos y la sección de la cuña en cada muestra, repartidas por longitud (no
        // por parámetro: cada tramo de una cadena tiene su propia escala)
        auto section_at = [&](double t) {
            gp_Pnt c = cc.Value(t);
            double da, db;
            gp_Pnt pa, pb;
            if (!blend::nearest(c, ea, da, pa) || !blend::nearest(c, eb, db, pb)) throw Standard_Failure("no se encontró el contacto con las caras");
            gp_Vec ua(c, pa), ub(c, pb);
            if (ua.Magnitude() < 1e-9 || ub.Magnitude() < 1e-9) throw Standard_Failure("contacto degenerado");
            ua.Normalize();
            ub.Normalize();
            double half = ua.Angle(ub) / 2;
            double L = 1.6 * r / std::max(std::cos(half) * std::cos(half), 0.05);
            return std::make_pair(c, BRepBuilderAPI_MakePolygon(c, c.Translated(ua * L), c.Translated(ub * L), Standard_True).Wire());
        };
        const int S = ring ? 96 : 40;
        std::vector<std::vector<TopoDS_Wire>> pieces;
        std::vector<gp_Pnt> centers_path;
        for (size_t j = 0; j + 1 < breaks.size(); j++) {
            double a0 = breaks[j], a1 = breaks[j + 1];
            double len = GCPnts_AbscissaPoint::Length(cc, a0, a1);
            int n = std::max(6, (int)std::ceil(S * len / std::max(total, 1e-12)));
            std::vector<TopoDS_Wire> ws;
            for (int k = 0; k <= n; k++) {
                // La junta con el tramo anterior es la misma sección
                if (k == 0 && j > 0) {
                    ws.push_back(pieces.back().back());
                    continue;
                }
                double t = a0;
                if (k == n) t = a1;
                else if (k > 0) {
                    GCPnts_AbscissaPoint ap(cc, len * k / n, a0);
                    t = ap.IsDone() ? ap.Parameter() : a0 + (a1 - a0) * k / n;
                }
                // Anillo: la última sección es la primera
                if (ring && j + 2 == breaks.size() && k == n) {
                    ws.push_back(pieces.front().front());
                    continue;
                }
                auto [c, w] = section_at(t);
                ws.push_back(w);
                centers_path.push_back(c);
            }
            pieces.push_back(ws);
        }
        Handle(TColgp_HArray1OfPnt) path = new TColgp_HArray1OfPnt(1, (int)centers_path.size());
        for (size_t i = 0; i < centers_path.size(); i++) path->SetValue((int)i + 1, centers_path[i]);

        // El camino del tubo: suavizado por los centros
        GeomAPI_Interpolate interp(path, ring ? Standard_True : Standard_False, 1e-7);
        interp.Perform();
        if (!interp.IsDone()) throw Standard_Failure("no se pudo suavizar el camino del centro");
        TopoDS_Wire spinew = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(interp.Curve()).Edge()).Wire();

        // La cuña: un loft suave por tramo. Con un tramo solo y abierto es el sólido; en un
        // anillo de una curva o un camino abierto de varios tramos, los costados de todos (sin
        // las tapas de las juntas) se cosen en una cáscara cerrada
        TopoDS_Shape wedge;
        auto loft = [&](const std::vector<TopoDS_Wire>& ws) {
            auto piece = std::make_unique<BRepOffsetAPI_ThruSections>(Standard_True, Standard_False);
            for (auto& w : ws) piece->AddWire(w);
            piece->Build();
            if (!piece->IsDone()) throw Standard_Failure("no se pudo armar la cuña del redondeo");
            return piece;
        };
        int spine_edges = 0;
        for (TopExp_Explorer x(spine, TopAbs_EDGE); x.More(); x.Next()) spine_edges++;
        // Camino de varias aristas (una cadena tangente): barrido por el camino exacto con todas
        // las secciones; el loft por secciones alrededor de un arco no es exacto y sus tramos
        // no cierran contra los rectos
        const bool chain = spine_edges > 1;
        TopoDS_Wire exact_spine = chain ? blend::trim(spine, cc, ta, tb) : spine;
        if (!ring && pieces.size() == 1) wedge = loft(pieces[0])->Shape();
        else if (chain) {
            BRepOffsetAPI_MakePipeShell ws(exact_spine);
            ws.SetDiscreteMode();
            for (size_t j = 0; j < pieces.size(); j++)
                for (size_t k = (j == 0 ? 0 : 1); k + (ring && j + 1 == pieces.size() ? 1 : 0) < pieces[j].size(); k++) ws.Add(pieces[j][k], Standard_False, Standard_False);
            ws.Build();
            if (!ws.IsDone() || !ws.MakeSolid()) throw Standard_Failure("no se pudo armar la cuña del redondeo");
            wedge = ws.Shape();
        } else {
            BRepBuilderAPI_Sewing sew(1e-6 * std::max(1.0, size));
            for (size_t j = 0; j < pieces.size(); j++) {
                auto piece = loft(pieces[j]);
                TopTools_IndexedMapOfShape caps;
                if (ring || j > 0) TopExp::MapShapes(piece->FirstShape(), TopAbs_FACE, caps);
                if (ring || j + 1 < pieces.size()) TopExp::MapShapes(piece->LastShape(), TopAbs_FACE, caps);
                for (TopExp_Explorer x(piece->Shape(), TopAbs_FACE); x.More(); x.Next())
                    if (!caps.Contains(x.Current())) sew.Add(x.Current());
            }
            sew.Perform();
            TopExp_Explorer sx(sew.SewedShape(), TopAbs_SHELL);
            if (!sx.More()) throw Standard_Failure("no se pudo cerrar la cuña del redondeo");
            BRepBuilderAPI_MakeSolid ms(TopoDS::Shell(sx.Current()));
            if (!ms.IsDone()) throw Standard_Failure("no se pudo cerrar la cuña del redondeo");
            TopoDS_Solid solid = ms.Solid();
            BRepLib::OrientClosedSolid(solid);
            wedge = solid;
        }

        // El tubo: un círculo de radio r a lo largo del camino del centro
        gp_Pnt c0;
        gp_Vec t0;
        interp.Curve()->D1(interp.Curve()->FirstParameter(), c0, t0);
        gp_Circ circ(gp_Ax2(c0, gp_Dir(t0)), r);
        TopoDS_Wire prof = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circ).Edge()).Wire();
        // En un camino de varias aristas (una cadena tangente: recta y arco) por el camino
        // exacto, el cruce de las caras desfasadas: la interpolación se pasa de largo en las
        // juntas y el tubo deja de ser tangente a las caras. Triedro discreto: el de Frenet
        // corregido falla en caminos rectos
        BRepOffsetAPI_MakePipeShell pipe(chain ? exact_spine : spinew);
        pipe.SetDiscreteMode();
        if (chain) {
            gp_Pnt q;
            gp_Vec tq;
            cc.D1(ta, q, tq);
            prof = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(q, gp_Dir(tq)), r)).Edge()).Wire();
        }
        pipe.Add(prof, Standard_False, Standard_False);
        pipe.Build();
        if (!pipe.IsDone() || !pipe.MakeSolid()) throw Standard_Failure("no se pudo armar el tubo del redondeo");

        // Un poco de holgura en los booleanos: el tubo toca las caras de forma tangente
        const double fuzzy = 1e-5 * r;
        BRepAlgoAPI_Cut tool;
        {
            TopTools_ListOfShape w, t;
            w.Append(wedge);
            t.Append(pipe.Shape());
            tool.SetArguments(w);
            tool.SetTools(t);
            tool.SetFuzzyValue(fuzzy);
            tool.Build();
        }
        if (!tool.IsDone()) throw Standard_Failure("no se pudo armar la herramienta del redondeo");
        TopTools_ListOfShape args, tools;
        args.Append(s->s);
        tools.Append(tool.Shape());
        // Canto convexo: se saca lo de la cuña que queda fuera del tubo; rincón cóncavo: se agrega
        auto run = [&](BRepAlgoAPI_BooleanOperation& op, const char* what) {
            op.SetArguments(args);
            op.SetTools(tools);
            op.SetFuzzyValue(fuzzy);
            op.Build();
            if (!op.IsDone()) throw Standard_Failure(what);
            record(op, {s->s}, op.Shape());
            return wrap_checked(op.Shape(), "redondeo de cara");
        };
        if (best_sign < 0) {
            BRepAlgoAPI_Cut cut;
            return run(cut, "no se pudo recortar el canto");
        }
        BRepAlgoAPI_Fuse fuse;
        return run(fuse, "no se pudo rellenar el rincón");
    });
}

CadShape* cad_face_patch(const CadShape* face, const double* uv, const int32_t* sizes, int32_t n) {
    return guard("parche sobre la cara", (CadShape*)nullptr, [&] {
        TopExp_Explorer fx(face->s, TopAbs_FACE);
        if (!fx.More()) throw Standard_Failure("se esperaba una cara");
        TopoDS_Face f = TopoDS::Face(fx.Current());
        Handle(Geom_Surface) surf = BRep_Tool::Surface(f);
        if (n < 1) throw Standard_Failure("sin contorno");
        // Cada lazo: segmentos rectos en (u, v), que sobre la superficie siguen su forma
        int at = 0;
        std::vector<TopoDS_Wire> loops;
        for (int32_t k = 0; k < n; k++) {
            BRepBuilderAPI_MakeWire mw;
            int m = sizes[k];
            for (int i = 0; i < m; i++) {
                gp_Pnt2d a(uv[2 * (at + i)], uv[2 * (at + i) + 1]);
                gp_Pnt2d b(uv[2 * (at + (i + 1) % m)], uv[2 * (at + (i + 1) % m) + 1]);
                if (a.Distance(b) < 1e-12) continue;
                Handle(Geom2d_TrimmedCurve) seg = GCE2d_MakeSegment(a, b).Value();
                mw.Add(BRepBuilderAPI_MakeEdge(seg, surf).Edge());
            }
            if (!mw.IsDone()) throw Standard_Failure("un contorno no se pudo armar sobre la cara");
            loops.push_back(mw.Wire());
            at += m;
        }
        BRepBuilderAPI_MakeFace mf(surf, loops[0], Standard_True);
        for (size_t k = 1; k < loops.size(); k++) mf.Add(loops[k]);
        if (!mf.IsDone()) throw Standard_Failure("el contorno no forma una cara");
        TopoDS_Face patch = mf.Face();
        BRepLib::BuildCurves3d(patch);
        ShapeFix_Face fix(patch);
        fix.Perform();
        patch = fix.Face();
        // Con la normal hacia afuera, como la cara de la que sale
        if (f.Orientation() == TopAbs_REVERSED) patch.Reverse();
        return wrap(patch);
    });
}

CadShape* cad_split_faces(const CadShape* s, const int32_t* faces, int32_t n, const CadShape* tool) {
    return guard("línea de partición", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        TopTools_MapOfShape chosen;
        for (int32_t i = 0; i < n; i++) {
            if (faces[i] < 0 || faces[i] >= m.Extent()) throw Standard_Failure("cara inexistente");
            chosen.Add(m(faces[i] + 1));
        }
        // Lo que la herramienta deja sobre la pieza entera, de una vez: así las aristas de caras
        // vecinas comparten el vértice donde cruzan el borde común (cortando cara por cara cada
        // una tiene el suyo y BRepFeat_SplitShape solo parte la primera)
        BRepAlgoAPI_Section sec(s->s, tool->s, Standard_False);
        sec.ComputePCurveOn1(Standard_True);
        sec.Approximation(Standard_True);
        sec.Build();
        if (!sec.IsDone()) throw Standard_Failure("no se pudo cruzar la herramienta con la pieza");
        BRepFeat_SplitShape split(s->s);
        int added = 0;
        for (TopExp_Explorer ex(sec.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
            TopoDS_Shape f;
            if (!sec.HasAncestorFaceOn1(ex.Current(), f) || !chosen.Contains(f)) continue;
            split.Add(TopoDS::Edge(ex.Current()), TopoDS::Face(f));
            added++;
        }
        // Curvas apoyadas sobre las caras (el cruce no las asigna a ninguna): cara por cara
        const bool on_faces = added == 0;
        for (int32_t i = 0; on_faces && i < n; i++) {
            TopoDS_Face f = TopoDS::Face(m(faces[i] + 1));
            BRepAlgoAPI_Section fs(f, tool->s, Standard_False);
            fs.ComputePCurveOn1(Standard_True);
            fs.Approximation(Standard_True);
            fs.Build();
            if (!fs.IsDone()) continue;
            for (TopExp_Explorer ex(fs.Shape(), TopAbs_EDGE); ex.More(); ex.Next(), added++) split.Add(TopoDS::Edge(ex.Current()), f);
        }
        if (added == 0) throw Standard_Failure("la herramienta no cruza las caras elegidas");
        split.Build();
        if (!split.IsDone()) throw Standard_Failure("no se pudieron partir las caras");
        record(split, {s->s}, split.Shape());
        return wrap_checked(split.Shape(), "línea de partición");
    });
}

namespace silhouette {

// Las curvas de la cara donde la normal es perpendicular a `d`, en (u, v): marching squares sobre
// una grilla, con cada cruce afinado por bisección sobre el lado de la celda
static std::vector<std::vector<gp_Pnt2d>> curves(const TopoDS_Face& f, const gp_Vec& d) {
    BRepAdaptor_Surface sf(f, Standard_False);
    double u0, u1, v0, v1;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    // Un poco más allá del borde (si la superficie no es periódica en esa dirección): la curva
    // tiene que cruzar el borde de la cara para partirla
    double du = (u1 - u0) * 0.02, dv = (v1 - v0) * 0.02;
    if (!sf.IsUPeriodic()) {
        u0 = std::max(u0 - du, sf.FirstUParameter());
        u1 = std::min(u1 + du, sf.LastUParameter());
    }
    if (!sf.IsVPeriodic()) {
        v0 = std::max(v0 - dv, sf.FirstVParameter());
        v1 = std::min(v1 + dv, sf.LastVParameter());
    }
    auto g = [&](double u, double v) {
        gp_Pnt p;
        gp_Vec su, sv;
        sf.D1(u, v, p, su, sv);
        gp_Vec n = su.Crossed(sv);
        double m = n.Magnitude();
        return m < 1e-14 ? 0.0 : n.Dot(d) / m;
    };
    const int N = 64;
    std::vector<double> val((N + 1) * (N + 1));
    auto U = [&](int i) { return u0 + (u1 - u0) * i / N; };
    auto V = [&](int j) { return v0 + (v1 - v0) * j / N; };
    for (int i = 0; i <= N; i++)
        for (int j = 0; j <= N; j++) val[i * (N + 1) + j] = g(U(i), V(j));
    auto at = [&](int i, int j) { return val[i * (N + 1) + j]; };
    // Cruce sobre un lado de la grilla: clave (lado horizontal o vertical, i, j)
    std::map<long, gp_Pnt2d> cross;
    auto key = [&](int dir, int i, int j) { return (long)dir * 1000000 + (long)i * 1000 + j; };
    auto crossing = [&](int dir, int i, int j) -> bool {
        long k = key(dir, i, j);
        if (cross.count(k)) return true;
        double a = at(i, j), b = dir == 0 ? at(i + 1, j) : at(i, j + 1);
        if ((a > 0) == (b > 0)) return false;
        // Bisección sobre el lado
        double lo = 0, hi = 1, ga = a;
        for (int it = 0; it < 50; it++) {
            double mid = (lo + hi) / 2;
            double gm = dir == 0 ? g(U(i) + (U(i + 1) - U(i)) * mid, V(j)) : g(U(i), V(j) + (V(j + 1) - V(j)) * mid);
            if ((gm > 0) == (ga > 0)) lo = mid;
            else hi = mid;
        }
        double t = (lo + hi) / 2;
        cross[k] = dir == 0 ? gp_Pnt2d(U(i) + (U(i + 1) - U(i)) * t, V(j)) : gp_Pnt2d(U(i), V(j) + (V(j + 1) - V(j)) * t);
        return true;
    };
    // Segmentos por celda (entre claves de cruces)
    std::multimap<long, long> links;
    for (int i = 0; i < N; i++)
        for (int j = 0; j < N; j++) {
            std::vector<long> ks;
            if (crossing(0, i, j)) ks.push_back(key(0, i, j));
            if (crossing(1, i + 1, j)) ks.push_back(key(1, i + 1, j));
            if (crossing(0, i, j + 1)) ks.push_back(key(0, i, j + 1));
            if (crossing(1, i, j)) ks.push_back(key(1, i, j));
            if (ks.size() == 2) {
                links.insert({ks[0], ks[1]});
                links.insert({ks[1], ks[0]});
            } else if (ks.size() == 4) {
                // Silla: se emparejan según el signo del centro
                double c = g((U(i) + U(i + 1)) / 2, (V(j) + V(j + 1)) / 2);
                bool same = (c > 0) == (at(i, j) > 0);
                int a0 = same ? 0 : 0, a1 = same ? 1 : 3, b0 = same ? 2 : 1, b1 = same ? 3 : 2;
                links.insert({ks[a0], ks[a1]});
                links.insert({ks[a1], ks[a0]});
                links.insert({ks[b0], ks[b1]});
                links.insert({ks[b1], ks[b0]});
            }
        }
    // Encadenar: arrancar por las puntas (un solo vecino) y después los lazos
    std::vector<std::vector<gp_Pnt2d>> out;
    std::set<std::pair<long, long>> used;
    auto walk = [&](long start) {
        std::vector<gp_Pnt2d> poly{cross[start]};
        long cur = start, prev = -1;
        while (true) {
            long next = -1;
            auto range = links.equal_range(cur);
            for (auto it = range.first; it != range.second; ++it)
                if (it->second != prev && !used.count({cur, it->second})) {
                    next = it->second;
                    break;
                }
            if (next < 0) break;
            used.insert({cur, next});
            used.insert({next, cur});
            poly.push_back(cross[next]);
            prev = cur;
            cur = next;
            if (cur == start) break;
        }
        if (poly.size() >= 3) out.push_back(poly);
    };
    for (auto& [k, _] : cross)
        if (links.count(k) == 1) {
            auto r = links.equal_range(k);
            if (!used.count({k, r.first->second})) walk(k);
        }
    for (auto& [k, _] : cross) {
        auto r = links.equal_range(k);
        for (auto it = r.first; it != r.second; ++it)
            if (!used.count({k, it->second})) walk(k);
    }
    return out;
}

}  // namespace silhouette

// Línea de partición por silueta: parte las caras donde, vistas desde `dir`, la normal pasa a
// ser perpendicular a la mirada (el contorno aparente; en un molde, donde se separan las dos
// mitades). Cada curva se lleva a una cortina reglada que la cruza a lo largo de la normal y la
// cara se parte por su cruce con la cortina.
CadShape* cad_split_silhouette(const CadShape* s, const int32_t* faces, int32_t n, const double* dir) {
    return guard("línea de partición por silueta", (CadShape*)nullptr, [&] {
        gp_Vec d(dir[0], dir[1], dir[2]);
        if (d.Magnitude() < 1e-12) throw Standard_Failure("falta la dirección");
        d.Normalize();
        auto m = map_of(s->s, TopAbs_FACE);
        BRepFeat_SplitShape split(s->s);
        int added = 0;
        for (int32_t i = 0; i < n; i++) {
            if (faces[i] < 0 || faces[i] >= m.Extent()) throw Standard_Failure("cara inexistente");
            TopoDS_Face f = TopoDS::Face(m(faces[i] + 1));
            Bnd_Box fb;
            BRepBndLib::Add(f, fb);
            const double h = 0.05 * std::sqrt(fb.SquareExtent()) + 1e-3;
            BRepAdaptor_Surface sf(f, Standard_False);
            for (auto& poly : silhouette::curves(f, d)) {
                // Los dos bordes de la cortina: la curva corrida ± h a lo largo de la normal
                bool closed = poly.front().Distance(poly.back()) < 1e-12;
                // Puntos en 3D con su normal (en un polo la derivada se anula: se toma la de al lado)
                std::vector<gp_Pnt> lo_p, hi_p;
                const double vmid = (sf.FirstVParameter() + sf.LastVParameter()) / 2;
                for (size_t k = 0; k + (closed ? 1 : 0) < poly.size(); k++) {
                    gp_Pnt p, q;
                    gp_Vec su, sv;
                    sf.D1(poly[k].X(), poly[k].Y(), p, su, sv);
                    gp_Vec nn = su.Crossed(sv);
                    for (double e = 1e-7; nn.Magnitude() < 1e-12 && e < 1e-2; e *= 10) {
                        double v = poly[k].Y() + (poly[k].Y() < vmid ? e : -e);
                        sf.D1(poly[k].X(), v, q, su, sv);
                        nn = su.Crossed(sv);
                    }
                    if (nn.Magnitude() < 1e-12) continue;
                    nn.Normalize();
                    gp_Pnt a = p.Translated(nn * -h);
                    if (!lo_p.empty() && a.Distance(lo_p.back()) < 1e-7 * h) continue;
                    lo_p.push_back(a);
                    hi_p.push_back(p.Translated(nn * h));
                }
                if (closed && lo_p.size() > 2 && lo_p.front().Distance(lo_p.back()) < 1e-7 * h) {
                    lo_p.pop_back();
                    hi_p.pop_back();
                }
                const int count = (int)lo_p.size();
                if (count < 2) continue;
                Handle(TColgp_HArray1OfPnt) lo = new TColgp_HArray1OfPnt(1, count), hi = new TColgp_HArray1OfPnt(1, count);
                for (int k = 0; k < count; k++) {
                    lo->SetValue(k + 1, lo_p[k]);
                    hi->SetValue(k + 1, hi_p[k]);
                }
                auto curve = [&](const Handle(TColgp_HArray1OfPnt)& pts) {
                    GeomAPI_Interpolate in(pts, closed ? Standard_True : Standard_False, 1e-9);
                    in.Perform();
                    if (!in.IsDone()) throw Standard_Failure("no se pudo seguir la silueta");
                    return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(in.Curve()).Edge()).Wire();
                };
                BRepOffsetAPI_ThruSections curtain(Standard_False, Standard_True);
                curtain.AddWire(curve(lo));
                curtain.AddWire(curve(hi));
                curtain.Build();
                if (!curtain.IsDone()) continue;
                BRepAlgoAPI_Section sec(f, curtain.Shape(), Standard_False);
                sec.ComputePCurveOn1(Standard_True);
                sec.Approximation(Standard_True);
                sec.Build();
                if (!sec.IsDone()) continue;
                for (TopExp_Explorer ex(sec.Shape(), TopAbs_EDGE); ex.More(); ex.Next(), added++) split.Add(TopoDS::Edge(ex.Current()), f);
            }
        }
        if (added == 0) throw Standard_Failure("ninguna de las caras elegidas tiene silueta en esa dirección");
        split.Build();
        if (!split.IsDone()) throw Standard_Failure("no se pudieron partir las caras por la silueta");
        record(split, {s->s}, split.Shape());
        return wrap_checked(split.Shape(), "línea de partición por silueta");
    });
}

CadShape* cad_extend_face(const CadShape* face, double distance) {
    return guard("extender la cara", (CadShape*)nullptr, [&] {
        TopExp_Explorer ex(face->s, TopAbs_FACE);
        if (!ex.More()) throw Standard_Failure("se esperaba una cara");
        TopoDS_Face ext;
        BRepLib::ExtendFace(TopoDS::Face(ex.Current()), distance, Standard_True, Standard_True, Standard_True, Standard_True, ext);
        if (ext.IsNull()) throw Standard_Failure("no se pudo extender la cara");
        return wrap(ext);
    });
}

CadShape* cad_extend_surface(const CadShape* s, const int32_t* edges, int32_t n, double distance) {
    return guard("extender superficie", (CadShape*)nullptr, [&] {
        if (distance <= 0) throw Standard_Failure("la distancia tiene que ser mayor que cero");
        auto em = map_of(s->s, TopAbs_EDGE);
        TopTools_IndexedDataMapOfShapeListOfShape anc;
        TopExp::MapShapesAndAncestors(s->s, TopAbs_EDGE, TopAbs_FACE, anc);
        // Por cara, qué lados de su parámetro se extienden (u mín, u máx, v mín, v máx)
        std::vector<std::pair<TopoDS_Face, std::array<bool, 4>>> todo;
        for (int32_t i = 0; i < n; i++) {
            if (edges[i] < 0 || edges[i] >= em.Extent()) throw Standard_Failure("arista inexistente");
            TopoDS_Edge e = TopoDS::Edge(em(edges[i] + 1));
            const TopTools_ListOfShape& fl = anc.FindFromKey(e);
            if (fl.Extent() != 1) throw Standard_Failure("solo se extienden bordes libres de una superficie");
            TopoDS_Face f = TopoDS::Face(fl.First());
            double u0, u1, v0, v1, a, b;
            BRepTools::UVBounds(f, u0, u1, v0, v1);
            Handle(Geom2d_Curve) pc = BRep_Tool::CurveOnSurface(e, f, a, b);
            if (pc.IsNull()) throw Standard_Failure("el borde no está sobre la superficie");
            gp_Pnt2d m = pc->Value((a + b) / 2);
            double d[4] = {std::fabs(m.X() - u0), std::fabs(m.X() - u1), std::fabs(m.Y() - v0), std::fabs(m.Y() - v1)};
            int side = int(std::min_element(d, d + 4) - d);
            auto it = std::find_if(todo.begin(), todo.end(), [&](auto& t) { return t.first.IsSame(f); });
            if (it == todo.end()) {
                todo.push_back({f, {false, false, false, false}});
                it = todo.end() - 1;
            }
            it->second[side] = true;
        }
        // Las caras extendidas en su lugar; las demás como estaban
        TopoDS_Compound c;
        BRep_Builder bld;
        bld.MakeCompound(c);
        for (TopExp_Explorer fx(s->s, TopAbs_FACE); fx.More(); fx.Next()) {
            TopoDS_Face f = TopoDS::Face(fx.Current());
            auto it = std::find_if(todo.begin(), todo.end(), [&](auto& t) { return t.first.IsSame(f); });
            if (it == todo.end()) {
                bld.Add(c, f);
                continue;
            }
            TopoDS_Face ext;
            BRepLib::ExtendFace(f, distance, it->second[0], it->second[1], it->second[2], it->second[3], ext);
            if (ext.IsNull()) throw Standard_Failure("no se pudo extender la superficie");
            bld.Add(c, ext);
        }
        return wrap_checked(c, "superficie extendida");
    });
}

CadShape* cad_remove_faces(const CadShape* s, const int32_t* faces, int32_t n) {
    return guard("eliminar cara", (CadShape*)nullptr, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        if (n == 0) throw Standard_Failure("elegir al menos una cara");
        BRepAlgoAPI_Defeaturing df;
        df.SetShape(s->s);
        for (int32_t i = 0; i < n; i++) {
            if (faces[i] < 0 || faces[i] >= m.Extent()) throw Standard_Failure("cara inexistente");
            df.AddFaceToRemove(m(faces[i] + 1));
        }
        df.SetRunParallel(Standard_False);
        df.Build();
        if (!df.IsDone() || df.HasErrors()) throw Standard_Failure("las caras vecinas no alcanzan a cerrar el hueco");
        record(df, {s->s}, df.Shape());
        return wrap_checked(df.Shape(), "eliminar cara");
    });
}

CadShape* cad_fillet_points(const CadShape* s, const int32_t* edges, int32_t n, const double* law, int32_t np) {
    return guard("redondeo", (CadShape*)nullptr, [&] {
        // `law`: `np` pares (fracción del largo de 0 a 1, radio), en orden
        if (np < 2) throw Standard_Failure("hacen falta al menos dos radios");
        auto m = map_of(s->s, TopAbs_EDGE);
        BRepFilletAPI_MakeFillet mk(s->s);
        std::vector<TopoDS_Shape> chosen;
        for (int32_t i = 0; i < n; i++) {
            if (edges[i] < 0 || edges[i] >= m.Extent()) throw Standard_Failure("arista inexistente");
            TopoDS_Edge e = TopoDS::Edge(m(edges[i] + 1));
            double first, last;
            BRep_Tool::Range(e, first, last);
            TColgp_Array1OfPnt2d ur(1, np);
            for (int32_t k = 0; k < np; k++) ur.SetValue(k + 1, gp_Pnt2d(first + law[2 * k] * (last - first), law[2 * k + 1]));
            mk.Add(ur, e);
            chosen.push_back(e);
        }
        mk.Build();
        if (!mk.IsDone()) throw Standard_Failure("radio demasiado grande para esas aristas");
        record(mk, {s->s}, mk.Shape(), chosen);
        return wrap_checked(mk.Shape(), "el radio no entra en esas aristas");
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
        // Superficies (sin sólidos): la intersección con el semiespacio infinito
        // sale vacía; un bloque grande y finito del lado que se conserva sí sirve
        if (!TopExp_Explorer(s->s, TopAbs_SOLID).More()) {
            Bnd_Box box;
            BRepBndLib::Add(s->s, box);
            if (box.IsVoid()) throw Standard_Failure("nada que cortar");
            double size = 4.0 * std::sqrt(box.SquareExtent()) + 1.0;
            gp_Pnt c = box.CornerMin().Translated(gp_Vec(box.CornerMin(), box.CornerMax()) * 0.5);
            gp_Vec nv(dir(normal));
            gp_Pnt on = c.Translated(nv * -gp_Vec(pnt(origin), c).Dot(nv));
            gp_Ax3 ax(on, pl.Axis().Direction());
            gp_Pln at(ax);
            TopoDS_Face sq = BRepBuilderAPI_MakeFace(at, -size, size, -size, size).Face();
            TopoDS_Shape block = BRepPrimAPI_MakePrism(sq, gp_Vec(dir(normal)).Normalized() * (2.0 * size)).Shape();
            return wrap(boolean_op(s->s, block, 2));
        }
        TopoDS_Face f = BRepBuilderAPI_MakeFace(pl).Face();
        gp_Pnt ref = pnt(origin).Translated(gp_Vec(dir(normal)));
        TopoDS_Shape half = BRepPrimAPI_MakeHalfSpace(f, ref).Solid();
        return wrap(boolean_op(s->s, half, 2));
    });
}

CadShape* cad_split_by(const CadShape* s, const CadShape* const* tools, int32_t n) {
    return guard("partir", (CadShape*)nullptr, [&] {
        if (n < 1) throw Standard_Failure("falta con qué partir");
        TopTools_ListOfShape args, ts;
        args.Append(s->s);
        std::vector<TopoDS_Shape> inputs{s->s};
        for (int32_t i = 0; i < n; i++) {
            ts.Append(tools[i]->s);
            inputs.push_back(tools[i]->s);
        }
        BRepAlgoAPI_Splitter sp;
        sp.SetArguments(args);
        sp.SetTools(ts);
        sp.Build();
        if (sp.HasErrors() || !sp.IsDone()) throw Standard_Failure("no se pudo partir");
        record(sp, inputs, sp.Shape());
        return wrap(sp.Shape());
    });
}

// Malla cerrada, manifold y orientada (cada arista dirigida una sola vez, con
// su opuesta en otra cara): las caras se arman compartiendo vértices y aristas,
// sin coser. Con 20 k triángulos baja de ~10 s a menos de 1 s. Si la malla no
// cumple, devuelve una forma nula y se cose. Cada pieza conexa es una cáscara:
// las de volumen positivo, sólidos; las de volumen negativo (pieles que miran
// a un hueco), huecos del sólido que las contiene.
static TopoDS_Shape mesh_shell_direct(const double* verts, int32_t n_verts, const int32_t* tris,
                                      int32_t n_tris, double tolerance) {
    std::map<std::pair<int32_t, int32_t>, int32_t> directed;
    for (int32_t i = 0; i < n_tris; i++) {
        const int32_t* t = tris + 3 * i;
        for (int k = 0; k < 3; k++) {
            int32_t a = t[k], b = t[(k + 1) % 3];
            if (a < 0 || b < 0 || a >= n_verts || b >= n_verts || a == b) return TopoDS_Shape();
            if (!directed.emplace(std::make_pair(a, b), i).second) return TopoDS_Shape();
        }
    }
    for (const auto& [e, f] : directed)
        if (!directed.count({e.second, e.first})) return TopoDS_Shape();
    // Piezas conexas por aristas compartidas
    std::vector<int32_t> parent(n_tris);
    for (int32_t i = 0; i < n_tris; i++) parent[i] = i;
    auto root = [&](int32_t x) {
        while (parent[x] != x) x = parent[x] = parent[parent[x]];
        return x;
    };
    for (const auto& [e, f] : directed) parent[root(f)] = root(directed.at({e.second, e.first}));
    std::map<int32_t, TopoDS_Shell> shells;
    std::map<int32_t, double> volume;
    std::map<int32_t, int32_t> sample;

    BRep_Builder builder;
    std::vector<TopoDS_Vertex> vertex(n_verts);
    std::map<std::pair<int32_t, int32_t>, TopoDS_Edge> edges;
    auto vertex_of = [&](int32_t i) -> const TopoDS_Vertex& {
        if (vertex[i].IsNull()) builder.MakeVertex(vertex[i], pnt(verts + 3 * i), tolerance);
        return vertex[i];
    };
    for (int32_t i = 0; i < n_tris; i++) {
        const int32_t* t = tris + 3 * i;
        gp_Pnt pa = pnt(verts + 3 * t[0]), pb = pnt(verts + 3 * t[1]), pc = pnt(verts + 3 * t[2]);
        gp_Vec n = gp_Vec(pa, pb).Crossed(gp_Vec(pa, pc));
        if (n.Magnitude() < tolerance * tolerance) return TopoDS_Shape();
        TopoDS_Wire wire;
        builder.MakeWire(wire);
        for (int k = 0; k < 3; k++) {
            int32_t a = t[k], b = t[(k + 1) % 3];
            auto key = std::make_pair(std::min(a, b), std::max(a, b));
            auto it = edges.find(key);
            if (it == edges.end()) {
                BRepBuilderAPI_MakeEdge me(vertex_of(key.first), vertex_of(key.second));
                if (!me.IsDone()) return TopoDS_Shape();
                it = edges.emplace(key, me.Edge()).first;
            }
            builder.Add(wire, a < b ? it->second : TopoDS::Edge(it->second.Reversed()));
        }
        wire.Closed(Standard_True);
        BRepBuilderAPI_MakeFace mf(gp_Pln(pa, gp_Dir(n)), wire, Standard_True);
        if (!mf.IsDone()) return TopoDS_Shape();
        auto it = shells.find(root(i));
        if (it == shells.end()) {
            TopoDS_Shell shell;
            builder.MakeShell(shell);
            it = shells.emplace(root(i), shell).first;
            sample[root(i)] = t[0];
        }
        builder.Add(it->second, mf.Face());
        volume[root(i)] += gp_Vec(pa.XYZ()).Dot(gp_Vec(pb.XYZ()).Crossed(gp_Vec(pc.XYZ()))) / 6.0;
    }
    if (shells.size() == 1) {
        // Orientada por el volumen con signo: el clasificador de
        // OrientClosedSolid a veces se equivoca con mallas grandes (dejaba
        // el sólido al revés y las booleanas daban vacío)
        TopoDS_Shell shell = shells.begin()->second;
        shell.Closed(Standard_True);
        if (volume.begin()->second < 0.0) shell.Reverse();
        TopoDS_Solid solid;
        builder.MakeSolid(solid);
        builder.Add(solid, shell);
        return solid;
    }
    std::vector<std::pair<TopoDS_Solid, TopoDS_Solid>> outer;  // (con huecos, sola)
    std::vector<int32_t> voids;
    for (auto& [r, shell] : shells) {
        shell.Closed(Standard_True);
        if (volume[r] < 0.0) {
            voids.push_back(r);
            continue;
        }
        TopoDS_Solid solid, alone;
        builder.MakeSolid(solid);
        builder.Add(solid, shell);
        builder.MakeSolid(alone);
        builder.Add(alone, shell);
        outer.emplace_back(solid, alone);
    }
    if (outer.empty()) return TopoDS_Shape();
    for (int32_t r : voids) {
        gp_Pnt p = pnt(verts + 3 * sample[r]);
        bool placed = false;
        for (auto& [solid, alone] : outer) {
            BRepClass3d_SolidClassifier in(alone, p, tolerance);
            if (in.State() == TopAbs_IN) {
                builder.Add(solid, shells[r]);
                placed = true;
                break;
            }
        }
        if (!placed) return TopoDS_Shape();
    }
    if (outer.size() == 1) return outer[0].first;
    TopoDS_Compound all;
    builder.MakeCompound(all);
    for (auto& [solid, alone] : outer) builder.Add(all, solid);
    return all;
}

CadShape* cad_from_mesh(const double* verts, int32_t n_verts, const int32_t* tris, int32_t n_tris,
                        double tolerance) {
    return guard("malla a sólido", (CadShape*)nullptr, [&] {
        TopoDS_Shape direct = mesh_shell_direct(verts, n_verts, tris, n_tris, tolerance);
        if (!direct.IsNull()) return wrap(direct);
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

int32_t cad_face_classes(const CadShape* s, CadFaceClass* out, int32_t n) {
    return guard("clases de cara", 0, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        if (n < m.Extent()) throw Standard_Failure("faltan lugares para las caras");
        for (int i = 1; i <= m.Extent(); i++) {
            TopoDS_Face face = TopoDS::Face(m(i));
            CadFaceClass& o = out[i - 1];
            std::memset(&o, 0, sizeof(CadFaceClass));
            o.reversed = face.Orientation() == TopAbs_REVERSED ? 1 : 0;
            BRepAdaptor_Surface ad(face);
            switch (ad.GetType()) {
                case GeomAbs_Plane: {
                    o.surface = 0;
                    gp_Pln pl = ad.Plane();
                    put(o.origin, pl.Location().XYZ());
                    gp_Dir d = pl.Axis().Direction();
                    if (o.reversed) d.Reverse();
                    put(o.dir, d.XYZ());
                    break;
                }
                case GeomAbs_Cylinder: {
                    o.surface = 1;
                    gp_Cylinder cy = ad.Cylinder();
                    put(o.origin, cy.Axis().Location().XYZ());
                    put(o.dir, cy.Axis().Direction().XYZ());
                    o.radius = cy.Radius();
                    break;
                }
                case GeomAbs_Cone: {
                    o.surface = 2;
                    gp_Cone co = ad.Cone();
                    put(o.origin, co.Axis().Location().XYZ());
                    put(o.dir, co.Axis().Direction().XYZ());
                    o.radius = co.RefRadius();
                    o.angle = co.SemiAngle();
                    break;
                }
                case GeomAbs_Sphere: o.surface = 3; break;
                case GeomAbs_Torus: {
                    o.surface = 4;
                    gp_Torus to = ad.Torus();
                    put(o.origin, to.Axis().Location().XYZ());
                    put(o.dir, to.Axis().Direction().XYZ());
                    o.radius = to.MajorRadius();
                    break;
                }
                case GeomAbs_BezierSurface:
                case GeomAbs_BSplineSurface: o.surface = 5; break;
                case GeomAbs_SurfaceOfRevolution: o.surface = 6; break;
                case GeomAbs_SurfaceOfExtrusion: o.surface = 7; break;
                case GeomAbs_OffsetSurface: o.surface = 8; break;
                default: o.surface = 9; break;
            }
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
                // En un polo (esfera, cono) la normal se anula: se toma la de al lado, hacia el medio de la cara
                if (n.Magnitude() < 1e-12) {
                    double u0, u1, w0, w1;
                    BRepTools::UVBounds(face, u0, u1, w0, w1);
                    for (double e = 1e-6; n.Magnitude() < 1e-12 && e < 0.1; e *= 10)
                        BRepGProp_Face(face).Normal(u + (0.5 * (u0 + u1) - u) * e, w + (0.5 * (w0 + w1) - w) * e, p, n);
                }
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

// Filete de perfil trapecial (semiancho `hw_in` en r_minor y `hw_out` en r_major, medidos a lo largo
// del eje), de `starts` entradas con avance `lead` por vuelta (el paso entre filetes es lead/starts)
static CadShape* thread_impl(const double* origin, const double* dir, const double* xdir, double r_minor, double r_major,
                             double lead, int32_t starts, double hw_in, double hw_out, double length, int32_t left,
                             int32_t chamfer) {
    const double pitch = lead;
    return guard("rosca", (CadShape*)nullptr, [&] {
        if (starts < 1 || starts > 8) throw Standard_Failure("las entradas van de 1 a 8");
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
        double slope = (hw_in - hw_out) / (r_major - r_minor);
        double hw_rh = hw_in + slope * (r_minor - rh);
        gp_Pnt c0 = ax.Location();
        gp_Vec x(ax.XDirection()), zv(z);
        // Cada entrada es la misma hélice corrida un paso entre filetes a lo largo del eje
        std::vector<TopoDS_Shape> starts_shapes;
        for (int32_t j = 0; j < starts; j++) {
            gp_Vec shift = zv * (j * lead / starts);
            TopoDS_Wire sp = spine;
            if (j > 0) {
                gp_Trsf t;
                t.SetTranslation(shift);
                sp = TopoDS::Wire(BRepBuilderAPI_Transform(spine, t, Standard_True).Shape());
            }
            auto at = [&](double r, double a) { return c0.Translated(x * r + zv * a + shift); };
            BRepBuilderAPI_MakePolygon poly;
            poly.Add(at(rh, -hw_rh));
            poly.Add(at(r_major, -hw_out));
            poly.Add(at(r_major, hw_out));
            poly.Add(at(rh, hw_rh));
            poly.Close();
            BRepOffsetAPI_MakePipeShell mk(sp);
            // Binormal fija en el eje: el perfil no se tuerce a lo largo de la hélice
            mk.SetMode(z);
            mk.Add(poly.Wire(), Standard_False, Standard_False);
            mk.Build();
            if (!mk.IsDone()) throw Standard_Failure("no se pudo barrer el filete");
            if (!mk.MakeSolid()) throw Standard_Failure("el filete no cierra un sólido");
            starts_shapes.push_back(mk.Shape());
        }
        gp_Ax2 core_ax(ax.Location(), z, ax.XDirection());
        TopoDS_Shape core = BRepPrimAPI_MakeCylinder(core_ax, r_minor, length + 2 * pitch).Shape();
        // La unión hélice-núcleo es lo caro (cada flanco corta al cilindro en
        // una hélice): en paralelo y con cajas orientadas, un 30 % menos
        BRepAlgoAPI_Fuse fu;
        {
            TopTools_ListOfShape args, tools;
            args.Append(core);
            for (auto& s : starts_shapes) tools.Append(s);
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

CadShape* cad_make_thread(const double* origin, const double* dir, const double* xdir, double r_minor, double r_major,
                          double pitch, double length, int32_t left, int32_t chamfer) {
    // Perfil ISO básico: flancos de 60°, cresta de P/8 y fondo de P/4
    double top = pitch / 8;
    return thread_impl(origin, dir, xdir, r_minor, r_major, pitch, 1, top / 2 + (r_major - r_minor) * std::tan(M_PI / 6), top / 2, length,
                       left, chamfer);
}

CadShape* cad_make_thread_trapezoid(const double* origin, const double* dir, const double* xdir, double r_minor, double r_major,
                                    double lead, int32_t starts, double hw_in, double hw_out, double length, int32_t left) {
    return thread_impl(origin, dir, xdir, r_minor, r_major, lead, starts, hw_in, hw_out, length, left, 0);
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

CadShape* cad_offset_shape(const CadShape* s, double distance) {
    return guard("desfasar superficie", (CadShape*)nullptr, [&] {
        if (std::fabs(distance) < 1e-9) throw Standard_Failure("distancia cero");
        BRepOffset_MakeOffset mk;
        mk.Initialize(s->s, distance, 1e-6, BRepOffset_Skin, Standard_False, Standard_False, GeomAbs_Arc, Standard_False);
        mk.MakeOffsetShape();
        if (!mk.IsDone()) throw Standard_Failure("no se pudo desfasar");
        // De un sólido queda su piel desfasada: las cáscaras, sin volumen
        TopoDS_Shape r = mk.Shape();
        if (TopExp_Explorer(r, TopAbs_SOLID).More()) {
            TopoDS_Compound c;
            BRep_Builder b;
            b.MakeCompound(c);
            for (TopExp_Explorer ex(r, TopAbs_SHELL); ex.More(); ex.Next()) b.Add(c, ex.Current());
            r = c;
        }
        return wrap_checked(r, "superficie desfasada");
    });
}

CadShape* cad_fill(const CadShape* const* edges, const CadShape* const* faces, int32_t n, const double* points,
                   int32_t n_points) {
    return guard("relleno", (CadShape*)nullptr, [&] {
        if (n < 1) throw Standard_Failure("hacen falta aristas del borde");
        // Borde plano, sin tangencias ni puntos: la cara plana que encierra
        bool tangent = false;
        for (int32_t i = 0; faces && i < n; i++) tangent = tangent || faces[i];
        if (!tangent && n_points == 0) {
            TopTools_ListOfShape list;
            for (int32_t i = 0; i < n; i++)
                for (TopExp_Explorer ex(edges[i]->s, TopAbs_EDGE); ex.More(); ex.Next()) list.Append(ex.Current());
            BRepBuilderAPI_MakeWire mw;
            mw.Add(list);
            if (mw.IsDone() && BRep_Tool::IsClosed(mw.Wire())) {
                BRepBuilderAPI_MakeFace mf(mw.Wire(), Standard_True);
                if (mf.IsDone()) return wrap(mf.Face());
            }
        }
        BRepOffsetAPI_MakeFilling fill;
        for (int32_t i = 0; i < n; i++) {
            TopExp_Explorer ex(edges[i]->s, TopAbs_EDGE);
            for (; ex.More(); ex.Next()) {
                const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
                // Con la cara vecina: tangente a ella; sin ella, solo pasa por el borde
                if (faces && faces[i]) {
                    TopExp_Explorer fx(faces[i]->s, TopAbs_FACE);
                    if (fx.More()) {
                        fill.Add(e, TopoDS::Face(fx.Current()), GeomAbs_G1, Standard_True);
                        continue;
                    }
                }
                fill.Add(e, GeomAbs_C0, Standard_True);
            }
        }
        for (int32_t i = 0; i < n_points; i++) fill.Add(pnt(points + 3 * i));
        fill.Build();
        if (!fill.IsDone()) throw Standard_Failure("el borde no se pudo rellenar (¿está cerrado?)");
        return wrap(fill.Shape());
    });
}

CadShape* cad_sew(const CadShape* const* shapes, int32_t n, double tolerance, int32_t solid) {
    return guard("coser", (CadShape*)nullptr, [&] {
        if (n < 1) throw Standard_Failure("nada que coser");
        BRepBuilderAPI_Sewing sew(tolerance);
        for (int32_t i = 0; i < n; i++) sew.Add(shapes[i]->s);
        sew.Perform();
        TopoDS_Shape sewed = sew.SewedShape();
        if (sewed.IsNull()) throw Standard_Failure("no quedó nada cosido");
        if (!solid) return wrap(sewed);
        // Cada concha cerrada, un sólido (orientado hacia afuera)
        BRep_Builder b;
        TopoDS_Compound out;
        b.MakeCompound(out);
        int count = 0;
        std::vector<TopoDS_Shell> shells;
        for (TopExp_Explorer ex(sewed, TopAbs_SHELL); ex.More(); ex.Next()) shells.push_back(TopoDS::Shell(ex.Current()));
        // Caras sueltas (una esfera es una sola cara): cada una en su concha
        for (TopExp_Explorer ex(sewed, TopAbs_FACE, TopAbs_SHELL); ex.More(); ex.Next()) {
            TopoDS_Shell sh;
            b.MakeShell(sh);
            b.Add(sh, ex.Current());
            sh.Closed(BRep_Tool::IsClosed(sh));
            shells.push_back(sh);
        }
        for (const TopoDS_Shell& shell : shells) {
            if (!BRep_Tool::IsClosed(shell)) throw Standard_Failure("las superficies no cierran un volumen");
            ShapeFix_Solid fix;
            TopoDS_Solid sol = fix.SolidFromShell(shell);
            if (sol.IsNull()) throw Standard_Failure("no se pudo hacer el sólido");
            b.Add(out, sol);
            count++;
        }
        if (count == 0) throw Standard_Failure("las superficies no cierran un volumen");
        if (count == 1) {
            TopExp_Explorer ex(out, TopAbs_SOLID);
            return wrap_checked(ex.Current(), "coser");
        }
        return wrap_checked(out, "coser");
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

int32_t cad_lines_hit(const CadShape* s, const double* origins, int32_t n, const double* dir, uint8_t* out) {
    return guard("rayos", 0, [&] {
        IntCurvesFace_ShapeIntersector inter;
        inter.Load(s->s, 1e-7);
        gp_Dir d(dir[0], dir[1], dir[2]);
        for (int32_t i = 0; i < n; i++) {
            inter.Perform(gp_Lin(pnt(origins + 3 * i), d), RealFirst(), RealLast());
            out[i] = inter.IsDone() && inter.NbPnt() > 0 ? 1 : 0;
        }
        return 1;
    });
}

int32_t cad_edge_points(const CadShape* s, int32_t index, const double* fractions, int32_t n, double* out) {
    return guard("puntos de arista", 0, [&] {
        auto m = map_of(s->s, TopAbs_EDGE);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("arista inexistente");
        BRepAdaptor_Curve c(TopoDS::Edge(m(index + 1)));
        double t0 = c.FirstParameter(), t1 = c.LastParameter();
        for (int32_t i = 0; i < n; i++) put(out + 3 * i, c.Value(t0 + fractions[i] * (t1 - t0)).XYZ());
        return 1;
    });
}

int32_t cad_face_eval(const CadShape* s, int32_t index, const double* uv, int32_t n, double* bounds, double* out) {
    return guard("superficie de la cara", 0, [&] {
        auto m = map_of(s->s, TopAbs_FACE);
        if (index < 0 || index >= m.Extent()) throw Standard_Failure("cara inexistente");
        TopoDS_Face face = TopoDS::Face(m(index + 1));
        BRepAdaptor_Surface sf(face);
        bounds[0] = sf.FirstUParameter();
        bounds[1] = sf.LastUParameter();
        bounds[2] = sf.FirstVParameter();
        bounds[3] = sf.LastVParameter();
        bool reversed = face.Orientation() == TopAbs_REVERSED;
        for (int32_t i = 0; i < n; i++) {
            gp_Pnt p;
            gp_Vec du, dv;
            sf.D1(uv[2 * i], uv[2 * i + 1], p, du, dv);
            gp_Vec nrm = du.Crossed(dv);
            if (nrm.Magnitude() > 1e-15) nrm.Normalize();
            if (reversed) nrm.Reverse();
            double* o = out + 12 * i;
            put(o, p.XYZ());
            put(o + 3, du.XYZ());
            put(o + 6, dv.XYZ());
            put(o + 9, nrm.XYZ());
        }
        return 1;
    });
}

int32_t cad_plane_hits(const CadShape* s, const double* origin, const double* normal, double* out, int32_t max) {
    return guard("cruces con el plano", -1, [&] {
        BRepAlgoAPI_Section sec(s->s, gp_Pln(pnt(origin), dir(normal)), Standard_False);
        sec.Build();
        if (!sec.IsDone()) throw Standard_Failure("no se pudo cortar");
        int32_t k = 0;
        for (TopExp_Explorer ex(sec.Shape(), TopAbs_VERTEX); ex.More() && k < max; ex.Next()) {
            put(out + 3 * k, BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())).XYZ());
            k++;
        }
        return k;
    });
}

CadShape* cad_section(const CadShape* s, const double* origin, const double* normal) {
    return guard("intersección con el plano", (CadShape*)nullptr, [&] {
        BRepAlgoAPI_Section sec(s->s, gp_Pln(pnt(origin), dir(normal)), Standard_False);
        sec.Approximation(Standard_True);
        sec.Build();
        if (!sec.IsDone()) throw Standard_Failure("no se pudo cortar");
        TopoDS_Compound out;
        BRep_Builder b;
        b.MakeCompound(out);
        for (TopExp_Explorer ex(sec.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) b.Add(out, ex.Current());
        return new CadShape{out};
    });
}

CadShape* cad_outline(const CadShape* s, const double* origin, const double* normal, const double* xdir) {
    return guard("silueta", (CadShape*)nullptr, [&] {
        gp_Ax2 cs(pnt(origin), dir(normal), dir(xdir));
        Handle(HLRBRep_Algo) algo = new HLRBRep_Algo();
        algo->Add(s->s);
        algo->Projector(HLRAlgo_Projector(cs));
        algo->Update();
        algo->Hide();
        HLRBRep_HLRToShape hts(algo);
        // Las líneas salen en el plano XY del sistema de la vista: se llevan al plano
        gp_Trsf back;
        back.SetTransformation(gp_Ax3(cs));
        back.Invert();
        TopoDS_Compound out;
        BRep_Builder b;
        b.MakeCompound(out);
        for (const TopoDS_Shape& comp : {hts.VCompound(), hts.OutLineVCompound(), hts.Rg1LineVCompound()}) {
            if (comp.IsNull()) continue;
            BRepLib::BuildCurves3d(comp);
            TopoDS_Shape moved = BRepBuilderAPI_Transform(comp, back, Standard_True).Shape();
            for (TopExp_Explorer ex(moved, TopAbs_EDGE); ex.More(); ex.Next()) b.Add(out, ex.Current());
        }
        return new CadShape{out};
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

// Solo la caja (sin volumen ni inercia: en superficies libres esas integrales tardan segundos)
int32_t cad_bounds(const CadShape* s, double* out6) {
    return guard("caja", 0, [&] {
        Bnd_Box box;
        BRepBndLib::AddOptimal(s->s, box, Standard_False, Standard_False);
        if (box.IsVoid()) return 0;
        box.Get(out6[0], out6[1], out6[2], out6[3], out6[4], out6[5]);
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
