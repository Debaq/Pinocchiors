// Redondeo de cara (cad_face_blend): volúmenes exactos (Pappus) contra OCCT real
#include "bench.h"
int main() {
    const double PI = M_PI;
    // 1. Canto convexo de una caja 20: cara de arriba con la del frente, r = 3
    {
        CadShape* b = box(0, 0, 0, 20, 20, 20);
        int top = face_where(b, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99; });
        int front = face_where(b, [](auto& f) { return f.surface == 0 && f.normal[1] < -0.99; });
        CadShape* r = cad_face_blend(b, &top, 1, &front, 1, 3.0, 0);
        report("canto convexo", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double exact = 8000 - (1 - PI / 4) * 9 * 20;
            auto m = mass(r);
            CHECK(near_(m.volume, exact, 0.05), "vol %.4f esperado %.4f", m.volume, exact);
            CHECK(cad_count_faces(r) == 7, "caras %d", cad_count_faces(r));
        }
    }
    // 2. Rincón cóncavo de una L: dos cajas unidas; piso z=10 de la de abajo y pared x=10 de la de arriba
    {
        CadShape* a = box(0, 0, 0, 30, 20, 10);
        CadShape* b2 = box(0, 0, 10, 10, 20, 20);
        const CadShape* both[2] = {a, b2};
        CadShape* L = cad_fuse_many(both, 2);
        int floor_ = face_where(L, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99 && f.point[0] > 15; });
        int wall = face_where(L, [](auto& f) { return f.surface == 0 && f.normal[0] > 0.99 && f.point[2] > 15; });
        CadShape* r = cad_face_blend(L, &floor_, 1, &wall, 1, 4.0, 0);
        report("rincón cóncavo", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double before = mass(L).volume;
            double exact = before + (1 - PI / 4) * 16 * 20;
            CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
        }
    }
    // 3. Canto convexo de 60° (cuña): placa con la cara de arriba y una cara inclinada
    {
        CadShape* b = box(0, 0, 0, 30, 20, 10);
        // Corte por un plano inclinado (un prisma triangular restado)
        double ax[9] = {30, 0, 10, 0, 1, 0, 1, 0, 0};
        int32_t kinds[3] = {0, 0, 0}, counts[3] = {6, 6, 6};
        double h = 10.0, w = h / std::tan(60 * PI / 180);  // pared a 60° respecto de la de arriba
        double data[18] = {30 - w, 0, 10, 31, 0, 10, 31, 0, 10, 31, 0, -1, 31, 0, -1, 30 - w, 0, 10};
        // triángulo cerrado: (30-w,0,10) → (31,0,10) → (31,0,-1) → back
        data[6] = 31; data[7] = 0; data[8] = 10; data[9] = 31 + (w + 1) * 0; data[10] = 0; data[11] = 10 - (w + 1) * std::tan(60 * PI / 180);
        data[12] = data[9]; data[13] = 0; data[14] = data[11]; data[15] = 30 - w; data[16] = 0; data[17] = 10;
        int32_t sizes[1] = {3};
        CadShape* tri = cad_make_face(kinds, counts, data, sizes, 1);
        (void)ax;
        CadShape* prism = tri ? cad_prism(tri, 0, 20, 0) : nullptr;
        CadShape* cutb = prism ? cad_boolean(b, prism, 1) : nullptr;
        if (!cutb) { fails++; std::printf("  (sin pieza inclinada: %s)\n", cad_last_error()); }
        else {
            int top = face_where(cutb, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99; });
            int slope = face_where(cutb, [](auto& f) { return f.surface == 0 && f.normal[0] > 0.3 && f.normal[2] > 0.3; });
            CadShape* r = cad_face_blend(cutb, &top, 1, &slope, 1, 2.0, 0);
            report("canto de 120° (pendiente 60°)", r);
            CHECK(r != nullptr, "sin resultado");
            if (r) {
                // Ángulo entre caras (interior) θ = 120°: sección quitada = r²(cot(θ/2)) − r²(π−θ)/2
                double th = 120 * PI / 180;
                double sec = 4 * (1 / std::tan(th / 2)) - 4 * (PI - th) / 2;
                double exact = mass(cutb).volume - sec * 20;
                CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
            }
        }
    }
    // 4. Caras que no se tocan: el canto ya tiene un chaflán de 1; redondear arriba con el frente (r 3) se lo come
    {
        CadShape* b = box(0, 0, 0, 20, 20, 20);
        int32_t kinds[3] = {0, 0, 0}, counts[3] = {6, 6, 6};
        double data[18] = {-1, -1, 21, -1, 1.5, 21, -1, 1.5, 21, -1, -1, 18.5, -1, -1, 18.5, -1, -1, 21};
        int32_t sizes[1] = {3};
        CadShape* tri = cad_make_face(kinds, counts, data, sizes, 1);
        CadShape* prism = tri ? cad_prism(tri, 22, 0, 0) : nullptr;
        CadShape* s = prism ? cad_boolean(b, prism, 1) : nullptr;
        CHECK(s != nullptr, "chaflán: %s", cad_last_error());
        if (s) {
            CHECK(cad_count_faces(s) == 7, "caras con chaflán %d", cad_count_faces(s));
            int top = face_where(s, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99; });
            int front = face_where(s, [](auto& f) { return f.surface == 0 && f.normal[1] < -0.99; });
            CadShape* r = cad_face_blend(s, &top, 1, &front, 1, 3.0, 0);
            report("caras no vecinas (por encima de un chaflán)", r);
            CHECK(r != nullptr, "sin resultado");
            if (r) {
                double exact = 8000 - (1 - PI / 4) * 9 * 20;
                CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
            }
        }
    }
    // Centroide de la sección del redondeo (cuadrado de lado r menos el cuarto de círculo), medido desde el rincón
    auto xbar = [&](double r) {
        double A = (1 - PI / 4) * r * r;
        double q = PI * r * r / 4, cq = r - 4 * r / (3 * PI);
        return (r * r * r / 2 - q * cq) / A;
    };
    // 5. Rincón cóncavo curvo: cilindro R 6 parado sobre una placa (relleno alrededor de la base), r 2
    {
        CadShape* plate = box(-20, -20, 0, 40, 40, 5);
        double ax[9] = {0, 0, 5, 0, 0, 1, 1, 0, 0};
        CadShape* cyl = cad_make_cylinder(ax, 6, 10);
        const CadShape* both[2] = {plate, cyl};
        CadShape* s = cad_fuse_many(both, 2);
        int top = face_where(s, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99 && std::fabs(f.point[2] - 5) < 1e-6; });
        int wall = face_where(s, [](auto& f) { return f.surface == 1; });
        CadShape* r = cad_face_blend(s, &top, 1, &wall, 1, 2.0, 0);
        report("relleno alrededor de un cilindro", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double A = (1 - PI / 4) * 4;
            double exact = mass(s).volume + 2 * PI * (6 + xbar(2)) * A;
            CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
    }
    // 6. Canto convexo curvo: el borde de arriba de un cilindro R 8, r 2
    {
        double ax[9] = {0, 0, 0, 0, 0, 1, 1, 0, 0};
        CadShape* cyl = cad_make_cylinder(ax, 8, 10);
        int top = face_where(cyl, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99; });
        int wall = face_where(cyl, [](auto& f) { return f.surface == 1; });
        CadShape* r = cad_face_blend(cyl, &top, 1, &wall, 1, 2.0, 0);
        report("borde de un cilindro", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double A = (1 - PI / 4) * 4;
            double exact = mass(cyl).volume - 2 * PI * (8 - xbar(2)) * A;
            CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
    }
    // 7. Ángulo que cambia: un cilindro inclinado 30° que atraviesa una placa (relleno en la elipse), r 1.5
    {
        CadShape* plate = box(-30, -30, 0, 60, 60, 5);
        double c = std::cos(30 * PI / 180), sn = std::sin(30 * PI / 180);
        double ax[9] = {-10 * sn, 0, 5 - 10 * c, sn, 0, c, c, 0, -sn};
        CadShape* cyl = cad_make_cylinder(ax, 5, 25);
        const CadShape* both[2] = {plate, cyl};
        CadShape* s = cad_fuse_many(both, 2);
        int top = face_where(s, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99 && std::fabs(f.point[2] - 5) < 1e-6; });
        int wall = face_where(s, [](auto& f) { return f.surface == 1 && f.point[2] > 5; });
        CadShape* r = cad_face_blend(s, &top, 1, &wall, 1, 1.5, 0);
        report("relleno de ángulo variable (cilindro inclinado)", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double added = mass(r).volume - mass(s).volume;
            // Entre lo de un rincón recto alrededor de la elipse (cota baja) y bastante más (lado agudo)
            double low = 2 * PI * 5 * (1 - PI / 4) * 2.25;
            CHECK(added > low * 0.8 && added < low * 4, "agregado %.4f (bajo %.4f)", added, low);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
    }
    // 8. Cadena tangente: óvalo (dos planos y dos medios cilindros) parado sobre una placa, r 1
    {
        CadShape* plate = box(-30, -30, 0, 60, 60, 5);
        CadShape* mid = box(-10, -4, 5, 20, 8, 6);
        double a1[9] = {-10, 0, 5, 0, 0, 1, 1, 0, 0}, a2[9] = {10, 0, 5, 0, 0, 1, 1, 0, 0};
        CadShape* c1 = cad_make_cylinder(a1, 4, 6);
        CadShape* c2 = cad_make_cylinder(a2, 4, 6);
        const CadShape* all[4] = {plate, mid, c1, c2};
        CadShape* s = cad_fuse_many(all, 4);
        int top = face_where(s, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99 && std::fabs(f.point[2] - 5) < 1e-6; });
        std::vector<int> walls;
        for (int i = 0; i < cad_count_faces(s); i++) {
            CadFaceInfo f{};
            cad_face_info(s, i, &f);
            if (f.point[2] > 5 + 1e-6 && f.point[2] < 11 - 1e-6 && std::fabs(f.normal[2]) < 1e-6) walls.push_back(i);
        }
        std::printf("  paredes del óvalo: %zu\n", walls.size());
        CadShape* r = cad_face_blend(s, &top, 1, walls.data(), (int)walls.size(), 1.0, 0);
        report("cadena tangente (óvalo sobre placa)", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double A = (1 - PI / 4) * 1;
            double exact = mass(s).volume + 2 * 20 * A + 2 * PI * (4 + xbar(1)) * A;
            CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
    }
    // 9. Cadena abierta: nervio con un extremo redondo (dos planos y medio cilindro) y el otro
    // plano, sobre una placa; relleno solo por los costados y la punta redonda, r 1
    {
        CadShape* plate = box(-30, -30, 0, 60, 60, 5);
        CadShape* mid = box(-10, -4, 5, 20, 8, 6);
        double a2[9] = {10, 0, 5, 0, 0, 1, 1, 0, 0};
        CadShape* c2 = cad_make_cylinder(a2, 4, 6);
        const CadShape* all[3] = {plate, mid, c2};
        CadShape* s = cad_fuse_many(all, 3);
        int top = face_where(s, [](auto& f) { return f.surface == 0 && f.normal[2] > 0.99 && std::fabs(f.point[2] - 5) < 1e-6; });
        std::vector<int> walls;
        for (int i = 0; i < cad_count_faces(s); i++) {
            CadFaceInfo f{};
            cad_face_info(s, i, &f);
            if (f.point[2] > 5 + 1e-6 && f.point[2] < 11 - 1e-6 && std::fabs(f.normal[2]) < 1e-6 && f.normal[0] > -0.99) walls.push_back(i);
        }
        std::printf("  paredes del nervio: %zu\n", walls.size());
        CadShape* r = cad_face_blend(s, &top, 1, walls.data(), (int)walls.size(), 1.0, 0);
        report("cadena abierta (nervio con punta redonda)", r);
        CHECK(r != nullptr, "sin resultado");
        if (r) {
            double A = (1 - PI / 4) * 1;
            double exact = mass(s).volume + 2 * 20 * A + PI * (4 + xbar(1)) * A;
            CHECK(near_(mass(r).volume, exact, 0.05), "vol %.4f esperado %.4f", mass(r).volume, exact);
            CHECK(cad_shape_is_valid(r), "sólido inválido");
        }
    }
    std::printf(fails ? "%d FALLAS\n" : "todo bien\n", fails);
    return fails ? 1 : 0;
}
