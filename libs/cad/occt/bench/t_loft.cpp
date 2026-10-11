// Transición con tangencia en las puntas (cad_loft_tangent): sale y llega perpendicular a las
// secciones
#include "bench.h"
static CadShape* square_wire(double cx, double cy, double z, double a) {
    double h = a / 2;
    double p[4][3] = {{cx - h, cy - h, z}, {cx + h, cy - h, z}, {cx + h, cy + h, z}, {cx - h, cy + h, z}};
    double d[24];
    for (int k = 0; k < 4; k++)
        for (int q = 0; q < 3; q++) {
            d[k * 6 + q] = p[k][q];
            d[k * 6 + 3 + q] = p[(k + 1) % 4][q];
        }
    int32_t kinds[4] = {0, 0, 0, 0}, counts[4] = {6, 6, 6, 6};
    return cad_make_wire(kinds, counts, d, 4);
}
// Centro en x de la rebanada z ∈ [z0, z1]
static double slice_x(const CadShape* s, double z0, double z1) {
    CadShape* slab = box(-50, -50, z0, 100, 100, z1 - z0);
    CadShape* c = cad_boolean(s, slab, 2);
    return c ? mass(c).center[0] : 1e9;
}
int main() {
    // Cuadrado de 4 abajo en (0, 0, 0) y arriba corrido a (10, 0, 20)
    CadShape* a = square_wire(0, 0, 0, 4);
    CadShape* b = square_wire(10, 0, 20, 4);
    const CadShape* secs[2] = {a, b};
    CadShape* plain = cad_loft(secs, 2, 1, 0);
    CadShape* both = cad_loft_tangent(secs, 2, 1.0, 1.0, 1);
    report("transición perpendicular en las dos puntas", both);
    CHECK(both != nullptr, "sin resultado");
    if (both) {
        CHECK(cad_shape_is_valid(both), "sólido inválido");
        // Sin tangencia, a z = 1 ya corrió 0,5; perpendicular, casi nada (sale vertical)
        double xp = slice_x(plain, 0.9, 1.1), xt = slice_x(both, 0.9, 1.1);
        std::printf("  a z = 1: sin tangencia %.4f, con tangencia %.4f\n", xp, xt);
        CHECK(near_(xp, 0.5, 0.02), "loft común a z = 1: %.4f", xp);
        CHECK(std::fabs(xt) < 0.1, "con tangencia corre %.4f", xt);
        // Lo mismo arriba: a z = 19 está casi en x = 10
        double xe = slice_x(both, 18.9, 19.1);
        CHECK(std::fabs(xe - 10) < 0.1, "arriba corre %.4f", xe);
        // Por la mitad pasa por el medio (simetría)
        CHECK(std::fabs(slice_x(both, 9.9, 10.1) - 5) < 0.05, "no pasa por el medio");
        // Las secciones van paralelas (se trasladan): cada corte horizontal es el cuadrado de 4,
        // así que el volumen es 16 × 20 (Cavalieri)
        CHECK(near_(mass(both).volume, 320, 1e-3), "volumen %.4f", mass(both).volume);
    }
    // Solo abajo: arriba llega en recta (a z = 19 todavía no está en 10)
    CadShape* start = cad_loft_tangent(secs, 2, 1.0, 0, 1);
    report("transición perpendicular solo al comienzo", start);
    CHECK(start && std::fabs(slice_x(start, 0.9, 1.1)) < 0.1, "no sale vertical");
    CHECK(start && std::fabs(slice_x(start, 18.9, 19.1) - 10) > 0.2, "arriba también vertical");
    // Cuadrado a círculo no van (distinta cantidad de lados): error claro
    {
        double d[7] = {0, 0, 20, 0, 0, 1, 3};
        int32_t k[1] = {2}, c[1] = {7};
        CadShape* circ = cad_make_wire(k, c, d, 1);
        const CadShape* mix[2] = {a, circ};
        CHECK(cad_loft_tangent(mix, 2, 1.0, 1.0, 1) == nullptr, "aceptó secciones con distinta cantidad de lados");
    }
    // Dos círculos (radio 2 abajo, 3 arriba corrido): sale perpendicular y el sólido es válido
    {
        double d0[7] = {0, 0, 0, 0, 0, 1, 2}, d1[7] = {10, 0, 20, 0, 0, 1, 3};
        int32_t k[1] = {2}, c[1] = {7};
        CadShape* c0 = cad_make_wire(k, c, d0, 1);
        CadShape* c1 = cad_make_wire(k, c, d1, 1);
        const CadShape* cs[2] = {c0, c1};
        CadShape* r = cad_loft_tangent(cs, 2, 1.0, 1.0, 1);
        report("círculo a círculo con tangencia", r);
        CHECK(r && cad_shape_is_valid(r), "inválido");
        if (r) CHECK(std::fabs(slice_x(r, 0.9, 1.1)) < 0.1, "no sale vertical: %.4f", slice_x(r, 0.9, 1.1));
    }
    // Tres secciones (la del medio a mitad de camino): pasa por las tres, sale y llega perpendicular
    {
        CadShape* m = square_wire(5, 0, 10, 4);
        const CadShape* three[3] = {a, m, b};
        CadShape* r = cad_loft_tangent(three, 3, 1.0, 1.0, 1);
        report("tres secciones con tangencia", r);
        CHECK(r && cad_shape_is_valid(r), "inválido");
        if (r) {
            CHECK(near_(mass(r).volume, 320, 0.5), "volumen %.4f", mass(r).volume);
            CHECK(std::fabs(slice_x(r, 9.9, 10.1) - 5) < 0.02, "no pasa por la del medio");
        }
    }
    // Una punta no plana: error
    double arc[9] = {0, 0, 0, 1, 1, 1, 2, 0, 2};
    int32_t k1[1] = {1}, c1[1] = {9};
    CadShape* bent = cad_make_wire(k1, c1, arc, 1);
    const CadShape* bad[2] = {bent, b};
    CHECK(cad_loft_tangent(bad, 2, 1.0, 0, 0) == nullptr, "aceptó una punta no plana");
    std::printf(fails ? "%d FALLAS\n" : "todo bien\n", fails);
    return fails ? 1 : 0;
}
