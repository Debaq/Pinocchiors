// Lo nuestro contra lo nuevo de OCCT 8 (PLAN_OCCT8 O5): misma tarea, tiempo y calidad.
//   libs/cad/occt/bench/build8.sh t_occt8
#include "bench.h"

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_FastSewing.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_UniformAbscissa.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_PointsToBSplineSurface.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomFill_Gordon.hxx>
#include <Geom_BSplineSurface.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_PolyAlgo.hxx>
#include <HLRBRep_PolyHLRToShape.hxx>
#include <HelixBRep_BuilderHelix.hxx>
#include <Poly_Triangulation.hxx>
#include <TColStd_HArray1OfReal.hxx>
#include <TColgp_Array2OfPnt.hxx>
#include <TColgp_HArray1OfPnt.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS.hxx>

#include <BRepTools.hxx>
#include <chrono>
#include <map>
#include <string>
#include <tuple>

// El puente guarda la forma como primer (único) miembro
struct CadShape { TopoDS_Shape s; };

static double ms_since(std::chrono::steady_clock::time_point t0) {
    return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
}
#define TIMED(var, ...) auto var##_t0 = std::chrono::steady_clock::now(); __VA_ARGS__; double var = ms_since(var##_t0)

// --- 1. Superficie libre: relleno (lo de hoy) vs Gordon vs puntos a B-spline ---
static double skin(double x, double y) { return 6 * std::sin(x / 15) * std::cos(y / 20) + 0.01 * x * y; }
static const double SX = 60, SY = 40;

// Desviación contra la piel (máx y media cuadrática) en una grilla de 40×40 dentro del borde
static void deviation(const Handle(Geom_Surface)& surf, double& maxd, double& rms) {
    maxd = 0; double sq = 0; int n = 0;
    for (int i = 1; i < 40; i++)
        for (int j = 1; j < 40; j++) {
            gp_Pnt p(SX * i / 40, SY * j / 40, 0);
            p.SetZ(skin(p.X(), p.Y()));
            GeomAPI_ProjectPointOnSurf pr(p, surf);
            if (pr.NbPoints() == 0) continue;
            double d = pr.LowerDistance();
            maxd = std::max(maxd, d); sq += d * d; n++;
        }
    rms = n ? std::sqrt(sq / n) : NAN;
}

static Handle(Geom_Curve) interp(const std::vector<gp_Pnt>& pts) {
    Handle(TColgp_HArray1OfPnt) a = new TColgp_HArray1OfPnt(1, (int)pts.size());
    for (size_t i = 0; i < pts.size(); i++) a->SetValue((int)i + 1, pts[i]);
    GeomAPI_Interpolate it(a, false, 1e-7);
    it.Perform();
    return it.Curve();
}

static void superficie_libre() {
    std::printf("\n== Superficie libre (zona del escaneo) ==\n");
    // Lo de hoy: borde de 40 segmentos + puntos de adentro, BRepOffsetAPI_MakeFilling (cad_fill)
    for (int npts : {24, 48, 100}) {
        std::vector<double> border;
        const int nb = 40;
        auto bp = [&](int k) {
            double t = 4.0 * k / nb, x, y;
            if (t < 1) { x = SX * t; y = 0; } else if (t < 2) { x = SX; y = SY * (t - 1); }
            else if (t < 3) { x = SX * (3 - t); y = SY; } else { x = 0; y = SY * (4 - t); }
            return gp_Pnt(x, y, skin(x, y));
        };
        std::vector<int32_t> kinds, counts;
        std::vector<double> data;
        for (int k = 0; k < nb; k++) {
            gp_Pnt a = bp(k), b = bp(k + 1);
            kinds.push_back(0); counts.push_back(6);
            for (double v : {a.X(), a.Y(), a.Z(), b.X(), b.Y(), b.Z()}) data.push_back(v);
        }
        CadShape* wire = cad_make_wire(kinds.data(), counts.data(), data.data(), nb);
        std::vector<double> pts;
        int side = (int)std::ceil(std::sqrt((double)npts));
        for (int i = 0; i < side; i++)
            for (int j = 0; j < side && (int)pts.size() / 3 < npts; j++) {
                double x = SX * (i + 0.5) / side, y = SY * (j + 0.5) / side;
                pts.insert(pts.end(), {x, y, skin(x, y)});
            }
        const CadShape* edges[1] = {wire};
        const CadShape* faces[1] = {nullptr};
        TIMED(t, CadShape* f = cad_fill(edges, faces, 1, pts.data(), (int32_t)pts.size() / 3));
        if (!f) { std::printf("relleno %3d puntos: ERROR %s\n", npts, cad_last_error()); continue; }
        TopoDS_Face face = TopoDS::Face(TopExp_Explorer(f->s, TopAbs_FACE).Current());
        double maxd, rms;
        deviation(BRep_Tool::Surface(face), maxd, rms);
        std::printf("relleno (hoy) %3d puntos     %8.1f ms  máx %.4f mm  rms %.4f mm  válida %d\n", npts, t, maxd, rms,
                    BRepCheck_Analyzer(f->s).IsValid());
    }
    // Gordon: red de n perfiles × n guías interpoladas por puntos de la piel
    for (int n : {3, 5, 8}) {
        TIMED(t, {
            NCollection_Array1<Handle(Geom_Curve)> prof(1, n), guid(1, n);
            for (int i = 0; i < n; i++) {
                std::vector<gp_Pnt> a, b;
                double y = SY * i / (n - 1), x = SX * i / (n - 1);
                // 4 tramos por celda: con más puntos la superficie se infla (k=20 → 397×397 polos)
                const int K = 4 * (n - 1);
                for (int k = 0; k <= K; k++) {
                    double xs = SX * k / K, ys = SY * k / K;
                    a.emplace_back(xs, y, skin(xs, y));
                    b.emplace_back(x, ys, skin(x, ys));
                }
                prof(i + 1) = interp(a);
                guid(i + 1) = interp(b);
            }
            GeomFill_Gordon g;
            g.Init(prof, guid, 1e-4);
            auto tg = std::chrono::steady_clock::now();
            g.Perform();
            std::printf("   Gordon %dx%d Perform: %.1f ms, estado %d\n", n, n, ms_since(tg), (int)g.Status());
            if (!g.IsDone()) {
                g.SetApproximationMode(GeomFill_Gordon::ApproximationMode::AllowApproximateFallback);
                g.Perform();
            }
            if (g.IsDone()) {
                double maxd, rms;
                deviation(g.Surface(), maxd, rms);
                std::printf("Gordon %dx%d curvas", n, n);
                std::printf("              ");
                std::printf("(ver tiempo)  máx %.4f mm  rms %.4f mm  aprox %d\n", maxd, rms, (int)g.IsApproximate());
            } else {
                std::printf("Gordon %dx%d: no se pudo (estado %d)\n", n, n, (int)g.Status());
            }
        });
        std::printf("   tiempo Gordon %dx%d: %.1f ms\n", n, n, t);
    }
    // Grilla de puntos a B-spline (sin borde exacto): referencia
    {
        TIMED(t, {
            TColgp_Array2OfPnt g(1, 20, 1, 20);
            for (int i = 1; i <= 20; i++)
                for (int j = 1; j <= 20; j++) {
                    double x = SX * (i - 1) / 19, y = SY * (j - 1) / 19;
                    g(i, j) = gp_Pnt(x, y, skin(x, y));
                }
            GeomAPI_PointsToBSplineSurface ap(g, 3, 8, GeomAbs_C2, 1e-3);
            double maxd, rms;
            deviation(ap.Surface(), maxd, rms);
            std::printf("puntos→B-spline 20x20            máx %.4f mm  rms %.4f mm\n", maxd, rms);
        });
        std::printf("   tiempo puntos→B-spline: %.1f ms\n", t);
    }
}

// --- 2. Hélice: la nuestra (recta sobre el cilindro) vs TKHelix ---
static double helix_error(const TopoDS_Shape& w, double r, double pitch, double& length) {
    GProp_GProps lp;
    BRepGProp::LinearProperties(w, lp);
    length = lp.Mass();
    double maxd = 0;
    for (TopExp_Explorer ex(w, TopAbs_EDGE); ex.More(); ex.Next()) {
        BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
        for (int k = 0; k <= 200; k++) {
            gp_Pnt p = c.Value(c.FirstParameter() + (c.LastParameter() - c.FirstParameter()) * k / 200);
            double rad = std::hypot(p.X(), p.Y());
            double ang = std::atan2(p.Y(), p.X());
            // Altura que tendría la hélice exacta en ese ángulo (la vuelta más cercana)
            double z0 = pitch * ang / (2 * M_PI);
            double dz = std::remainder(p.Z() - z0, pitch);
            maxd = std::max(maxd, std::hypot(rad - r, dz));
        }
    }
    return maxd;
}

static void helice() {
    std::printf("\n== Hélice r=10 paso=2.5 10 vueltas ==\n");
    const double r = 10, pitch = 2.5, turns = 10;
    double exact = turns * std::hypot(2 * M_PI * r, pitch);
    double o[3] = {0, 0, 0}, d[3] = {0, 0, 1};
    TIMED(t1, CadShape* ours = cad_make_helix(o, d, r, pitch, turns, 0));
    double len;
    double err = helix_error(ours->s, r, pitch, len);
    std::printf("nuestra   %8.3f ms  error forma %.2e mm  largo %.4f (exacto %.4f)\n", t1, err, len, exact);
    TIMED(t2, {
        HelixBRep_BuilderHelix b;
        NCollection_Array1<double> pitches(1, 1), nturns(1, 1);
        pitches(1) = pitch; nturns(1) = turns;
        b.SetParameters(gp_Ax3(gp::Origin(), gp::DZ(), gp::DX()), 2 * r, pitches, nturns);
        b.SetApproxParameters(1e-6, 8, GeomAbs_C2);
        b.Perform();
        if (b.ErrorStatus() == 0) {
            err = helix_error(b.Shape(), r, pitch, len);
            std::printf("TKHelix   ");
        }
    });
    std::printf("%8.3f ms  error forma %.2e mm  largo %.4f\n", t2, err, len);
    // Lo que la nuestra no hace: paso variable y espiral cónica
    {
        HelixBRep_BuilderHelix b;
        // Dos tramos: diámetros en los 3 bordes de tramo, paso y vueltas por tramo
        NCollection_Array1<double> diams(1, 3), pitches(1, 2), nturns(1, 2);
        diams(1) = 20; diams(2) = 14; diams(3) = 8; pitches(1) = 2.5; pitches(2) = 5; nturns(1) = 4; nturns(2) = 3;
        b.SetParameters(gp_Ax3(gp::Origin(), gp::DZ(), gp::DX()), diams, pitches, nturns);
        b.Perform();
        GProp_GProps lp;
        if (b.ErrorStatus() == 0) BRepGProp::LinearProperties(b.Shape(), lp);
        std::printf("TKHelix cónica + paso variable: %s, largo %.2f\n", b.ErrorStatus() == 0 ? "ok" : "falla", lp.Mass());
    }
}

// --- 3. Plano 2D: HLR exacto (lo de hoy) vs HLR por malla ---
static TopoDS_Shape pieza_hlr() {
    TopoDS_Shape s = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 80, 50, 12).Shape();
    s = BRepAlgoAPI_Fuse(s, BRepPrimAPI_MakeBox(gp_Pnt(30, 0, 12), 20, 50, 25).Shape()).Shape();
    for (int i = 0; i < 6; i++)
        for (int j = 0; j < 3; j++)
            s = BRepAlgoAPI_Cut(s, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(6 + i * 4.5 + (i > 2 ? 40 : 0), 10 + j * 15, -1), gp::DZ()), 1.5, 14).Shape()).Shape();
    s = BRepAlgoAPI_Cut(s, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(25, 25, 28), gp::DX()), 6, 30).Shape()).Shape();
    BRepFilletAPI_MakeFillet f(s);
    TopTools_IndexedMapOfShape em;
    TopExp::MapShapes(s, TopAbs_EDGE, em);
    for (int i = 1; i <= em.Extent(); i++) {
        BRepAdaptor_Curve c(TopoDS::Edge(em(i)));
        if (c.GetType() == GeomAbs_Line && std::fabs(c.Line().Direction().Z()) > 0.99 && c.Value(c.FirstParameter()).Z() < 12.5)
            f.Add(2.0, TopoDS::Edge(em(i)));
    }
    f.Build();
    return f.IsDone() ? f.Shape() : s;
}

static int count_edges(const TopoDS_Shape& s) {
    int n = 0;
    for (TopExp_Explorer ex(s, TopAbs_EDGE); ex.More(); ex.Next()) n++;
    return n;
}

static void hlr() {
    std::printf("\n== Plano 2D: 4 vistas de una pieza con 18 agujeros y redondeos ==\n");
    CadShape p{pieza_hlr()};
    double eyes[4][3] = {{0, -1, 0}, {0, 0, 1}, {1, 0, 0}, {1, -1, 1}};
    double xs[4][3] = {{1, 0, 0}, {1, 0, 0}, {0, 1, 0}, {1, 1, 0}};
    size_t total = 0;
    TIMED(t1, for (int v = 0; v < 4; v++) {
        uint8_t* out; size_t len;
        if (cad_hlr(&p, eyes[v], xs[v], 0.05, &out, &len)) { total += len; cad_bytes_free(out); }
    });
    std::printf("exacto (hoy, HLRBRep_Algo)  %8.1f ms  %zu bytes de líneas\n", t1, total);
    for (double defl : {0.05, 0.01}) {
        int lines = 0;
        TIMED(t2, {
            BRepMesh_IncrementalMesh(p.s, defl, false, 0.2, true);
            for (int v = 0; v < 4; v++) {
                gp_Dir eye(eyes[v][0], eyes[v][1], eyes[v][2]), x(xs[v][0], xs[v][1], xs[v][2]);
                Handle(HLRBRep_PolyAlgo) algo = new HLRBRep_PolyAlgo();
                algo->Load(p.s);
                algo->Projector(HLRAlgo_Projector(gp_Ax2(gp::Origin(), eye, x)));
                algo->Update();
                HLRBRep_PolyHLRToShape h;
                h.Update(algo);
                lines += count_edges(h.VCompound()) + count_edges(h.OutLineVCompound()) + count_edges(h.HCompound());
            }
        });
        std::printf("por malla (PolyAlgo) %.2f mm %8.1f ms  %d aristas\n", defl, t2, lines);
        BRepTools::Clean(p.s);
    }
}

static std::string huella_c(const CadShape* s) {
    auto m = mass(s);
    char b[160];
    std::snprintf(b, sizeof b, "vol=%.2f caras=%d válido=%d", m.volume, cad_count_faces(s), (int)BRepCheck_Analyzer(s->s).IsValid());
    return b;
}

// --- 4. Malla a sólido: lo de hoy (cad_from_mesh) vs Sewing vs FastSewing ---
static void malla_a_solido() {
    std::printf("\n== Malla a sólido (esfera teselada) ==\n");
    for (double defl : {0.5, 0.1}) {
        CadShape* sph = nullptr;
        { double c[3] = {0, 0, 0}; sph = cad_make_sphere(c, 30); }
        CadMesh m{};
        cad_tessellate(sph, defl, 0.3, &m);
        std::vector<double> v(m.positions, m.positions + m.n_vertices * 3);
        std::vector<int32_t> tr(m.triangles, m.triangles + m.n_triangles * 3);
        // La teselación separa vértices por cara: soldar por posición
        std::vector<double> wv; std::vector<int32_t> map(m.n_vertices);
        std::map<std::tuple<long, long, long>, int> seen;
        for (size_t i = 0; i < m.n_vertices; i++) {
            auto key = std::make_tuple(std::lround(v[3 * i] * 1e4), std::lround(v[3 * i + 1] * 1e4), std::lround(v[3 * i + 2] * 1e4));
            auto it = seen.find(key);
            if (it == seen.end()) { it = seen.emplace(key, (int)wv.size() / 3).first; wv.insert(wv.end(), {v[3 * i], v[3 * i + 1], v[3 * i + 2]}); }
            map[i] = it->second;
        }
        for (auto& k : tr) k = map[k];
        int nt = (int)tr.size() / 3;
        cad_mesh_free(&m);
        TIMED(t1, CadShape* a = cad_from_mesh(wv.data(), (int)wv.size() / 3, tr.data(), nt, 1e-4));
        std::printf("%6d triángulos: hoy (cad_from_mesh) %8.1f ms  %s\n", nt, t1, a ? huella_c(a).c_str() : cad_last_error());
        auto face = [&](int i) {
            gp_Pnt pa(wv[3 * tr[3 * i]], wv[3 * tr[3 * i] + 1], wv[3 * tr[3 * i] + 2]);
            gp_Pnt pb(wv[3 * tr[3 * i + 1]], wv[3 * tr[3 * i + 1] + 1], wv[3 * tr[3 * i + 1] + 2]);
            gp_Pnt pc(wv[3 * tr[3 * i + 2]], wv[3 * tr[3 * i + 2] + 1], wv[3 * tr[3 * i + 2] + 2]);
            return BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakePolygon(pa, pb, pc, true).Wire(), true).Face();
        };
        TIMED(t2, {
            BRepBuilderAPI_Sewing sew(1e-4);
            for (int i = 0; i < nt; i++) sew.Add(face(i));
            sew.Perform();
        });
        std::printf("%6d triángulos: Sewing                %8.1f ms\n", nt, t2);
        TIMED(t3, {
            BRepBuilderAPI_FastSewing fs(1e-4);
            for (int i = 0; i < nt; i++) fs.Add(face(i));
            fs.Perform();
            TopoDS_Shape r = fs.GetResult();
            // FastSewing solo une caras de bordes naturales (parches de 4 lados), no triángulos
            std::string info = "no arma concha con triángulos";
            for (TopExp_Explorer ex(r, TopAbs_SHELL); ex.More(); ex.Next()) {
                BRepBuilderAPI_MakeSolid ms(TopoDS::Shell(ex.Current()));
                if (ms.IsDone()) { CadShape c{ms.Solid()}; info = huella_c(&c); }
                break;
            }
            std::printf("%6d triángulos: FastSewing → %s\n", nt, info.c_str());
        });
        std::printf("%6d triángulos: FastSewing            %8.1f ms (con sólido y chequeo)\n", nt, t3);
    }
}

// --- 5. Superficie por red (cad_grid_face) sobre un casquete de esfera ---
static void red() {
    std::printf("\n== Superficie por grilla: casquete r=20, grilla en [-14,14]², borde círculo r=13 ==\n");
    auto zs = [](double x, double y) { return std::sqrt(400 - x * x - y * y); };
    std::vector<double> border;
    for (int k = 0; k < 40; k++) {
        double a = 2 * M_PI * k / 40, x = 13 * std::cos(a), y = 13 * std::sin(a);
        border.insert(border.end(), {x, y, zs(x, y)});
    }
    for (int curves : {4, 6, 8}) {
        int n = (curves - 1) * 4 + 1;
        std::vector<double> g;
        for (int r = 0; r < n; r++)
            for (int c = 0; c < n; c++) {
                double x = -14 + 28.0 * c / (n - 1), y = -14 + 28.0 * r / (n - 1);
                g.insert(g.end(), {x, y, zs(x, y)});
            }
        TIMED(t, CadShape* f = cad_grid_face(g.data(), n, n, 0, border.data(), 40));
        if (!f) { std::printf("red %d: ERROR %s\n", curves, cad_last_error()); continue; }
        TopoDS_Face face = TopoDS::Face(TopExp_Explorer(f->s, TopAbs_FACE).Current());
        Handle(Geom_Surface) S = BRep_Tool::Surface(face);
        double mx = 0;
        for (int i = 0; i <= 20; i++)
            for (int j = 0; j <= 20; j++) {
                double x = -9 + 18.0 * i / 20, y = -9 + 18.0 * j / 20;
                GeomAPI_ProjectPointOnSurf pr(gp_Pnt(x, y, zs(x, y)), S);
                if (pr.NbPoints()) mx = std::max(mx, pr.LowerDistance());
            }
        GProp_GProps gp;
        BRepGProp::SurfaceProperties(f->s, gp);
        double h = 20 - std::sqrt(400 - 169.0);
        std::printf("grilla %d (%dx%d): %8.1f ms  desvío máx %.5f mm  área %.2f (exacta %.2f)  válida %d\n", curves, n, n, t, mx,
                    gp.Mass(), 2 * M_PI * 20 * h, BRepCheck_Analyzer(f->s).IsValid());
    }
}

int main(int argc, char** argv) {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    std::printf("OpenCASCADE %s\n", cad_occt_version());
    std::string only = argc > 1 ? argv[1] : "";
    if (only.empty() || only == "superficie") superficie_libre();
    if (only.empty() || only == "helice") helice();
    if (only.empty() || only == "hlr") hlr();
    if (only.empty() || only == "malla") malla_a_solido();
    if (only == "red") red();
    return fails ? 1 : 0;
}
