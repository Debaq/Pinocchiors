// Barrido con torsión (cad_pipe_twist): volumen = área del perfil × largo del camino, y el perfil
// girado al final
#include "bench.h"
static CadShape* rect(double w, double h) {
    double d[24] = {-w / 2, -h / 2, 0, w / 2, -h / 2, 0, w / 2, -h / 2, 0, w / 2, h / 2, 0, w / 2, h / 2, 0, -w / 2, h / 2, 0, -w / 2, h / 2, 0, -w / 2, -h / 2, 0};
    int32_t kinds[4] = {0, 0, 0, 0}, counts[4] = {6, 6, 6, 6}, loops[1] = {4};
    return cad_make_face(kinds, counts, d, loops, 1);
}
int main() {
    const double PI = M_PI;
    CadShape* prof = rect(10, 4);
    // 1. Camino recto de 50, un cuarto de vuelta
    {
        double d[6] = {0, 0, 0, 0, 0, 50};
        int32_t k[1] = {0}, c[1] = {6};
        CadShape* spine = cad_make_wire(k, c, d, 1);
        CadShape* r = cad_pipe_twist(prof, spine, PI / 2, 1);
        report("recto con cuarto de vuelta", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            auto m = mass(r);
            CHECK(near_(m.volume, 2000, 0.5), "vol %.4f", m.volume);
            // La esquina (a √29 del eje) pasa por el eje x al girar: la caja llega a √29
            CHECK(near_(m.bbox_max[0], std::sqrt(29.0), 0.02) && near_(m.bbox_max[1], std::sqrt(29.0), 0.02), "caja %.4f %.4f", m.bbox_max[0], m.bbox_max[1]);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
        // Sin torsión: la caja no pasa de 5 × 2
        CadShape* r0 = cad_pipe_twist(prof, spine, 0, 1);
        CHECK(r0 && near_(mass(r0).bbox_max[0], 5, 1e-3) && near_(mass(r0).bbox_max[1], 2, 1e-3), "sin torsión cambia la caja");
    }
    // 2. Cuarto de circunferencia de radio 30, media vuelta
    {
        double s2 = std::sqrt(0.5);
        double d[9] = {0, 0, 0, 30 - 30 * s2, 0, 30 * s2, 30, 0, 30};
        int32_t k[1] = {1}, c[1] = {9};
        CadShape* spine = cad_make_wire(k, c, d, 1);
        // Con cualquier torsión el perfil sigue perpendicular al camino: el volumen no cambia
        for (double tw : {0.0, 0.5, 2.0}) {
            CadShape* q = cad_pipe_twist(prof, spine, tw, 1);
            CHECK(q && near_(mass(q).volume, 40 * PI / 2 * 30, 0.05), "torsión %.1f: vol %.4f", tw, q ? mass(q).volume : -1);
        }
        CadShape* r = cad_pipe_twist(prof, spine, PI, 1);
        report("arco con media vuelta", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double exact = 40 * PI / 2 * 30;
            CHECK(near_(mass(r).volume, exact, exact * 1e-3), "vol %.4f esperado %.4f", mass(r).volume, exact);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
    }
    std::printf(fails ? "%d FALLAS\n" : "todo bien\n", fails);
    return fails ? 1 : 0;
}
