// pinocchio-slicer: lamina mallas con el motor de OrcaSlicer (libslic3r v2.4.2).
//
//   pinocchio-slicer --config perfil.json --output salida.gcode [--thumbnails dir] [--resources dir] [--calib calib.json] pieza1.stl [pieza2.stl ...]
//
// - `perfil.json`: configuración ya resuelta (máquina + proceso + filamento, sin
//   `inherits` pendientes), en el formato de perfiles de Orca (valores en texto).
// - Cada STL viene en coordenadas de la cama (mm, Z arriba) y apoyado en Z = 0:
//   se lamina donde está, sin centrar ni redistribuir.
//
// - `--thumbnails dir`: miniaturas ya dibujadas, `dir/thumb_<W>x<H>.rgba` (RGBA crudo,
//   la fila de abajo primero, como OpenGL). Orca pide los tamaños del perfil
//   (`thumbnails`) y las escribe en el gcode; las que falten se omiten.
//
// Salida por stdout, una línea JSON por evento:
//   {"event":"progress","percent":N,"text":"..."}
//   {"event":"warning","text":"..."}
//   {"event":"done","gcode":"...","time_s":T,"filament_mm":L,"filament_cm3":V,"filament_g":G,"layers":N,
//    "roles":[{"role":"Outer wall","m":M,"g":G},...]}
//   {"event":"error","text":"..."}   (y código de salida distinto de 0)
// Cancelar = matar el proceso.
//
// - `--calib calib.json`: prueba de calibración (los modos de Orca, `Calib_Params`):
//   {"mode":"temp_tower","start":230,"end":190,"step":5,"print_numbers":true,
//    "freq_start_x":..,"freq_end_x":..,"freq_start_y":..,"freq_end_y":..,"shaper_type":"..",
//    "accelerations":[..],"speeds":[..],
//    "objects":[{"clave":"valor",...},...]}
//   `objects[i]` es la configuración propia de la pieza i (en el orden de los archivos):
//   claves de Orca con valores en texto (p. ej. {"print_flow_ratio":"1.05"}). Dos claves
//   propias cortan la pieza con tapa, como el corte de Orca: "@keep_below":"z" deja lo de
//   abajo de z y "@keep_above":"z" lo de arriba (después se apoya en la cama).

#include "libslic3r/libslic3r.h"
#include "libslic3r/Exception.hpp"
#include "libslic3r/Config.hpp"
#include "libslic3r/Model.hpp"
#include "libslic3r/Print.hpp"
#include "libslic3r/PrintConfig.hpp"
#include "libslic3r/Utils.hpp"
#include "libslic3r/GCode/GCodeProcessor.hpp"
#include "libslic3r/calib.hpp"
#include "libslic3r/TriangleMeshSlicer.hpp"
#include "nlohmann/json.hpp"

#include "libslic3r/ExtrusionEntity.hpp"
#include "libslic3r/GCode/ThumbnailData.hpp"

#include <boost/filesystem.hpp>
#include <boost/nowide/args.hpp>
#include <boost/nowide/fstream.hpp>
#include <fstream>
#include <cstdio>
#include <algorithm>
#include <exception>
#include <iostream>
#include <map>
#include <string>
#include <vector>
#ifdef _WIN32
#include <process.h>
#define getpid _getpid
#else
#include <unistd.h>
#endif

using namespace Slic3r;

namespace {

std::string json_escape(const std::string &s)
{
    std::string out;
    out.reserve(s.size() + 8);
    for (unsigned char c : s) {
        switch (c) {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20) {
                char buf[8];
                std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                out += buf;
            } else
                out += char(c);
        }
    }
    return out;
}

void emit(const std::string &line)
{
    std::cout << line << '\n';
    std::cout.flush();
}

int fail(const std::string &text)
{
    emit("{\"event\":\"error\",\"text\":\"" + json_escape(text) + "\"}");
    return 1;
}

struct Args
{
    std::string              config;
    std::string              output;
    std::string              resources;
    std::string              thumbnails;
    std::string              calib;
    std::vector<std::string> models;
};

bool parse_args(int argc, char **argv, Args &args, std::string &err)
{
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        auto value = [&](std::string &dst) {
            if (i + 1 >= argc) {
                err = "falta el valor de " + a;
                return false;
            }
            dst = argv[++i];
            return true;
        };
        if (a == "--config") {
            if (!value(args.config)) return false;
        } else if (a == "--output") {
            if (!value(args.output)) return false;
        } else if (a == "--thumbnails") {
            if (!value(args.thumbnails)) return false;
        } else if (a == "--resources") {
            if (!value(args.resources)) return false;
        } else if (a == "--calib") {
            if (!value(args.calib)) return false;
        } else if (a == "--version") {
            emit("{\"event\":\"version\",\"engine\":\"OrcaSlicer libslic3r\",\"version\":\"" SLIC3R_VERSION "\"}");
            std::exit(0);
        } else if (!a.empty() && a[0] == '-') {
            err = "opción desconocida: " + a;
            return false;
        } else
            args.models.push_back(a);
    }
    if (args.config.empty()) err = "falta --config";
    else if (args.output.empty()) err = "falta --output";
    else if (args.models.empty()) err = "no hay piezas que laminar";
    return err.empty();
}

// Lee las miniaturas pedidas de `dir` (ver arriba).
ThumbnailsList load_thumbnails(const std::string &dir, const ThumbnailsParams &params)
{
    ThumbnailsList out;
    for (const Vec2d &size : params.sizes) {
        const unsigned w = unsigned(size.x()), h = unsigned(size.y());
        boost::filesystem::path file = boost::filesystem::path(dir) / ("thumb_" + std::to_string(w) + "x" + std::to_string(h) + ".rgba");
        boost::nowide::ifstream in(file.string(), std::ios::binary);
        if (!in)
            continue;
        ThumbnailData data;
        data.set(w, h);
        in.read(reinterpret_cast<char *>(data.pixels.data()), std::streamsize(data.pixels.size()));
        if (in.gcount() == std::streamsize(data.pixels.size()))
            out.push_back(std::move(data));
    }
    return out;
}

// Modo de calibración por nombre
bool calib_mode(const std::string &name, CalibMode &mode)
{
    static const std::map<std::string, CalibMode> modes = {
        {"none", CalibMode::Calib_None},
        {"pa_line", CalibMode::Calib_PA_Line},
        {"pa_pattern", CalibMode::Calib_PA_Pattern},
        {"pa_tower", CalibMode::Calib_PA_Tower},
        {"flow_rate", CalibMode::Calib_Flow_Rate},
        {"temp_tower", CalibMode::Calib_Temp_Tower},
        {"vol_speed_tower", CalibMode::Calib_Vol_speed_Tower},
        {"vfa_tower", CalibMode::Calib_VFA_Tower},
        {"retraction_tower", CalibMode::Calib_Retraction_tower},
        {"input_shaping_freq", CalibMode::Calib_Input_shaping_freq},
        {"input_shaping_damp", CalibMode::Calib_Input_shaping_damp},
        {"cornering", CalibMode::Calib_Cornering},
    };
    auto it = modes.find(name);
    if (it == modes.end())
        return false;
    mode = it->second;
    return true;
}

// Lee `--calib`: los parámetros de la prueba y la configuración de cada pieza
bool load_calib(const std::string &file, Calib_Params &params, std::vector<std::map<std::string, std::string>> &objects, std::string &err)
{
    boost::nowide::ifstream in(file);
    if (!in) {
        err = "no se pudo abrir " + file;
        return false;
    }
    nlohmann::json j;
    try {
        in >> j;
    } catch (const std::exception &e) {
        err = std::string("calibración mal escrita: ") + e.what();
        return false;
    }
    if (!calib_mode(j.value("mode", std::string("none")), params.mode)) {
        err = "modo de calibración desconocido: " + j.value("mode", std::string());
        return false;
    }
    params.start         = j.value("start", 0.0);
    params.end           = j.value("end", 0.0);
    params.step          = j.value("step", 0.0);
    params.print_numbers = j.value("print_numbers", true);
    params.freqStartX    = j.value("freq_start_x", 0.0);
    params.freqEndX      = j.value("freq_end_x", 0.0);
    params.freqStartY    = j.value("freq_start_y", 0.0);
    params.freqEndY      = j.value("freq_end_y", 0.0);
    params.test_model    = j.value("test_model", 0);
    params.shaper_type   = j.value("shaper_type", std::string());
    if (j.contains("accelerations"))
        params.accelerations = j["accelerations"].get<std::vector<double>>();
    if (j.contains("speeds"))
        params.speeds = j["speeds"].get<std::vector<double>>();
    if (j.contains("objects"))
        for (const auto &obj : j["objects"]) {
            std::map<std::string, std::string> kv;
            for (auto it = obj.begin(); it != obj.end(); ++it)
                kv[it.key()] = it.value().is_string() ? it.value().get<std::string>() : it.value().dump();
            objects.push_back(std::move(kv));
        }
    return true;
}

} // namespace

int main(int argc, char **argv)
{
    // En Windows argv viene en la página de códigos local: rutas con tildes o ñ (la
    // carpeta del usuario) se perderían. nowide lo reemplaza por UTF-8, como espera libslic3r.
    boost::nowide::args utf8_args(argc, argv);
    Args        args;
    std::string err;
    if (!parse_args(argc, argv, args, err))
        return fail(err);

    try {
        // Solo errores en el log de libslic3r (stderr); el progreso va por stdout.
        set_logging_level(1);
        if (!args.resources.empty())
            set_resources_dir(args.resources);
        // Una carpeta por proceso: dos laminados a la vez (pruebas, conector MCP) no se pisan los temporales
        boost::filesystem::path tmp = boost::filesystem::temp_directory_path() / ("pinocchio-slicer-" + std::to_string(::getpid()));
        boost::filesystem::create_directories(tmp);
        set_temporary_dir(tmp.string());
        set_data_dir(tmp.string());

        // Configuración: valores por defecto + perfil resuelto. El perfil se lee en una
        // configuración vacía (así cada opción nace de su definición, con su tabla de
        // enums) y después se aplica sobre la completa, igual que el CLI de Orca.
        DynamicPrintConfig config = DynamicPrintConfig::full_print_config();
        {
            DynamicPrintConfig                 loaded;
            ConfigSubstitutionContext          ctx(ForwardCompatibilitySubstitutionRule::EnableSilent);
            std::map<std::string, std::string> key_values;
            std::string                        reason;
            if (loaded.load_from_json(args.config, ctx, true, key_values, reason) != 0)
                return fail("no se pudo leer el perfil: " + reason);
            config.apply(loaded, true);
        }
        config.normalize_fdm();

        // Una boquilla, un filamento (como el CLI de Orca con una sola extrusora).
        {
            std::vector<int> &maps = config.option<ConfigOptionInts>("filament_map", true)->values;
            size_t filaments = std::max<size_t>(1, config.option<ConfigOptionStrings>("filament_type", true)->values.size());
            maps.assign(filaments, 1);
            if (!config.has("nozzle_volume_type")) {
                auto *opt = config.option<ConfigOptionEnumsGeneric>("nozzle_volume_type", true);
                opt->values.assign(1, nvtStandard);
            }
        }

        // Prueba de calibración: parámetros y configuración propia de cada pieza
        Calib_Params                                    calib;
        std::vector<std::map<std::string, std::string>> per_object;
        if (!args.calib.empty() && !load_calib(args.calib, calib, per_object, err))
            return fail(err);

        // Piezas en coordenadas de la cama.
        Model model;
        // Las claves de cada pieza las escribe Pinocchio: un valor mal escrito es un error, no se reemplaza callado
        ConfigSubstitutionContext ctx(ForwardCompatibilitySubstitutionRule::Disable);
        for (size_t f = 0; f < args.models.size(); ++f) {
            Model part = Model::read_from_file(args.models[f], nullptr, nullptr, LoadStrategy::LoadModel);
            std::map<std::string, std::string> own = f < per_object.size() ? per_object[f] : std::map<std::string, std::string>{};
            for (ModelObject *src : part.objects) {
                ModelObject *obj = model.add_object(*src);
                obj->clear_instances();
                obj->add_instance();
                for (const auto &[key, value] : own) {
                    if (key == "@keep_below" || key == "@keep_above") {
                        // Corte horizontal con tapa (en las coordenadas del archivo)
                        const float z = std::stof(value);
                        for (ModelVolume *vol : obj->volumes) {
                            indexed_triangle_set its = vol->mesh().its;
                            its_transform(its, vol->get_matrix());
                            indexed_triangle_set upper, lower;
                            cut_mesh(its, z, &upper, &lower, true);
                            vol->set_mesh(key == "@keep_below" ? std::move(lower) : std::move(upper));
                            vol->set_transformation(Geometry::Transformation());
                            vol->calculate_convex_hull();
                        }
                        obj->invalidate_bounding_box();
                        continue;
                    }
                    try {
                        obj->config.set_deserialize(key, value, ctx);
                    } catch (const std::exception &e) {
                        return fail("clave de pieza «" + key + "»: " + e.what());
                    }
                }
                obj->ensure_on_bed();
            }
        }
        if (model.objects.empty())
            return fail("las piezas no tienen triángulos");

        Print print;
        print.is_BBL_printer() = false;
        // Origen de la placa: Print no lo inicializa (Vec3d de Eigen) y la GUI de Orca
        // siempre lo fija. Con basura, el brim se desplaza a coordenadas absurdas y
        // Clipper falla al azar con «Coordinate outside allowed range».
        print.set_plate_origin(Vec3d::Zero());
        print.set_status_callback([](const PrintBase::SlicingStatus &s) {
            if (s.percent >= 0)
                emit("{\"event\":\"progress\",\"percent\":" + std::to_string(s.percent) + ",\"text\":\"" + json_escape(s.text) + "\"}");
            else if (!s.text.empty())
                emit("{\"event\":\"warning\",\"text\":\"" + json_escape(s.text) + "\"}");
        });
        if (calib.mode != CalibMode::Calib_None)
            print.set_calib_params(calib);
        print.apply(model, config);

        StringObjectException warning;
        StringObjectException error = print.validate(&warning);
        if (!error.string.empty())
            return fail(error.string);
        if (!warning.string.empty())
            emit("{\"event\":\"warning\",\"text\":\"" + json_escape(warning.string) + "\"}");
        if (print.empty())
            return fail("ninguna pieza cabe en la cama o todas quedaron fuera");

        Model::setExtruderParams(config, 1);
        Model::setPrintSpeedTable(config, print.config());

        print.process();
        std::string conflict = print.get_conflict_string();
        if (!conflict.empty())
            emit("{\"event\":\"warning\",\"text\":\"" + json_escape(conflict) + "\"}");

        GCodeProcessorResult result;
        ThumbnailsGeneratorCallback thumbs = nullptr;
        if (!args.thumbnails.empty()) {
            const std::string dir = args.thumbnails;
            thumbs = [dir](const ThumbnailsParams &params) { return load_thumbnails(dir, params); };
        }
        std::string          out = print.export_gcode(args.output, &result, thumbs);
        if (result.gcode_check_result.error_code != 0)
            emit("{\"event\":\"warning\",\"text\":\"" + json_escape(std::string("trayectorias fuera del área imprimible")) + "\"}");

        const auto  &stats   = print.print_statistics();
        const float  time_s  = result.print_statistics.modes[static_cast<size_t>(PrintEstimatedStatistics::ETimeMode::Normal)].time;
        size_t       layers  = 0;
        for (const PrintObject *obj : print.objects())
            layers = std::max(layers, obj->layer_count());
        char         buf[512];
        std::snprintf(buf, sizeof(buf),
                      "\"time_s\":%.1f,\"filament_mm\":%.1f,\"filament_cm3\":%.3f,\"filament_g\":%.2f,\"layers\":%zu",
                      double(time_s), stats.total_used_filament, stats.total_extruded_volume / 1000.0, stats.total_weight, layers);
        std::string roles;
        for (const auto &[role, use] : result.print_statistics.used_filaments_per_role) {
            char r[256];
            std::snprintf(r, sizeof(r), "{\"role\":\"%s\",\"m\":%.4f,\"g\":%.3f}", json_escape(ExtrusionEntity::role_to_string(role)).c_str(), use.first, use.second);
            if (!roles.empty()) roles += ",";
            roles += r;
        }
        emit("{\"event\":\"done\",\"gcode\":\"" + json_escape(out) + "\"," + buf + ",\"roles\":[" + roles + "]}");
        return 0;
    } catch (const SlicingErrors &e) {
        // what() es solo «Errors»: el motivo de cada pieza va adentro
        std::string text;
        for (const SlicingError &one : e.errors_)
            if (text.find(one.what()) == std::string::npos)
                text += (text.empty() ? "" : "\n") + std::string(one.what());
        return fail(text.empty() ? std::string(e.what()) : text);
    } catch (const std::exception &e) {
        return fail(e.what());
    } catch (...) {
        return fail("error desconocido en el motor");
    }
}
