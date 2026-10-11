#include "cad_occt.h"
#include <cmath>
#include <cstdio>
#include <functional>
#include <vector>
#include <string>
static int fails = 0;
#define CHECK(c, ...) do { if (!(c)) { fails++; std::printf("  FALLA %s:%d: ", __FILE__, __LINE__); std::printf(__VA_ARGS__); std::printf("\n"); } } while (0)
static CadShape* box(double x, double y, double z, double dx, double dy, double dz) {
    double ax[9] = {x, y, z, 0, 0, 1, 1, 0, 0};
    return cad_make_box(ax, dx, dy, dz);
}
static CadMassInfo mass(const CadShape* s) { CadMassInfo m{}; cad_mass_info(s, &m); return m; }
static int face_where(const CadShape* s, std::function<bool(const CadFaceInfo&)> f) {
    int n = cad_count_faces(s);
    for (int i = 0; i < n; i++) { CadFaceInfo fi{}; cad_face_info(s, i, &fi); if (f(fi)) return i; }
    return -1;
}
static bool near_(double a, double b, double tol) { return std::fabs(a - b) <= tol; }
static void report(const char* name, CadShape* r) { std::printf("%s: %s", name, r ? "ok" : cad_last_error()); if (r) { auto m = mass(r); std::printf(" vol=%.4f valid=%d", m.volume, 1); } std::printf("\n"); }
