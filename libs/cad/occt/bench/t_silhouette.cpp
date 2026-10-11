// Línea de partición por silueta (cad_split_silhouette)
#include "bench.h"
int main() {
    const double PI = M_PI;
    // 1. Cilindro de eje z (R 10, alto 20) visto desde x: la silueta son las dos rectas y = ±10
    //    de la pared → con la costura que ya tenía (en x = 10), la pared queda en tres caras
    {
        double ax[9] = {0, 0, 0, 0, 0, 1, 1, 0, 0};
        CadShape* cyl = cad_make_cylinder(ax, 10, 20);
        int wall = face_where(cyl, [](auto& f) { return f.surface == 1; });
        int before = cad_count_faces(cyl);
        double d[3] = {1, 0, 0};
        CadShape* r = cad_split_silhouette(cyl, &wall, 1, d);
        report("cilindro visto de costado", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            int after = cad_count_faces(r);
            std::printf("  caras %d → %d\n", before, after);
            CHECK(after == before + 2, "caras %d → %d", before, after);
            CHECK(near_(mass(r).volume, PI * 100 * 20, 1e-3), "cambió el volumen");
            CHECK(cad_shape_is_valid(r), "inválido");
            int halves = 0;
            for (int i = 0; i < after; i++) {
                CadFaceInfo fi{};
                cad_face_info(r, i, &fi);
                if (fi.surface == 1) halves++;
            }
            CHECK(halves == 3, "pedazos de pared %d", halves);
        }
        // Visto desde arriba (z) la pared está toda de canto: no hay curva que la cruce
        double z[3] = {0, 0, 1};
        CHECK(cad_split_silhouette(cyl, &wall, 1, z) == nullptr, "partió una pared toda de canto");
    }
    // 2. Esfera R 10 vista desde z: la silueta es el ecuador
    {
        double c[3] = {0, 0, 0};
        CadShape* sph = cad_make_sphere(c, 10);
        int f = 0;
        double d[3] = {0, 0, 1};
        CadShape* r = cad_split_silhouette(sph, &f, 1, d);
        report("esfera vista desde arriba", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            CHECK(cad_count_faces(r) == 2, "caras %d", cad_count_faces(r));
            CHECK(near_(mass(r).volume, 4.0 / 3 * PI * 1000, 1e-2), "cambió el volumen");
            CHECK(cad_shape_is_valid(r), "inválido");
            // Las dos mitades: hemisferios de área 2π·100
            for (int i = 0; i < cad_count_faces(r); i++) {
                CadFaceInfo fi{};
                cad_face_info(r, i, &fi);
                std::printf("  cara %d punto (%.2f %.2f %.2f)\n", i, fi.point[0], fi.point[1], fi.point[2]);
            }
        }
    }
    // 3. Esfera vista en diagonal: la silueta es un círculo máximo inclinado
    {
        double c[3] = {0, 0, 0};
        CadShape* sph = cad_make_sphere(c, 10);
        int f = 0;
        double d[3] = {1, 1, 1};
        CadShape* r = cad_split_silhouette(sph, &f, 1, d);
        report("esfera vista en diagonal", r);
        CHECK(r && cad_count_faces(r) == 2 && cad_shape_is_valid(r), "no quedó en dos");
    }
    std::printf(fails ? "%d FALLAS\n" : "todo bien\n", fails);
    return fails ? 1 : 0;
}
