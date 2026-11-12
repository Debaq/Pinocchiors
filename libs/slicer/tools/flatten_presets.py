#!/usr/bin/env python3
"""Aplana los perfiles de Orca (resuelve `inherits`) para las impresoras elegidas.

    python3 -I libs/slicer/tools/flatten_presets.py <orca>/resources/profiles libs/slicer/presets

Escribe presets/{machine,process,filament}/<nombre>.json ya resueltos y presets/index.json
con qué procesos y filamentos sirven para cada impresora.
"""
import json
import pathlib
import re
import sys

# (fabricante, impresora, carretes): con más de un carrete (AD5X con su IFS) el laminador
# declara un filamento por carrete, como Orca, y al enviar se elige cuál usar.
# Todas las boquillas que trae Orca para cada una (la app las ofrece como «Boquilla»).
# La de 0.4 va primero: es la que se elige si no se dice otra.
PRINTERS = [
    ("Creality", "Creality K1 (0.4 nozzle)", 1),
    ("Creality", "Creality K1 (0.6 nozzle)", 1),
    ("Creality", "Creality K1 (0.8 nozzle)", 1),
    ("Creality", "Creality K1 (1.0 nozzle)", 1),
    ("Creality", "Creality K1C 0.4 nozzle", 1),
    ("Creality", "Creality K1C 0.6 nozzle", 1),
    ("Creality", "Creality K1C 0.8 nozzle", 1),
    ("Creality", "Creality K1C 1.0 nozzle", 1),
    ("Creality", "Creality K1 Max (0.4 nozzle)", 1),
    ("Creality", "Creality K1 Max (0.6 nozzle)", 1),
    ("Creality", "Creality K1 Max (0.8 nozzle)", 1),
    ("Creality", "Creality K1 Max (1.0 nozzle)", 1),
    ("Flashforge", "Flashforge AD5X 0.4 nozzle", 4),
    ("Flashforge", "Flashforge AD5X 0.25 nozzle", 4),
    ("Flashforge", "Flashforge AD5X 0.6 nozzle", 4),
    ("Flashforge", "Flashforge AD5X 0.8 nozzle", 4),
    ("Flashforge", "Flashforge AD5X 1.0 nozzle", 4),
    ("Creality", "Creality CR-10 Max 0.4 nozzle", 1),
    ("Creality", "Creality CR-10 Max 0.2 nozzle", 1),
    ("Creality", "Creality CR-10 Max 0.6 nozzle", 1),
    ("Creality", "Creality CR-10 Max 0.8 nozzle", 1),
    ("Creality", "Creality CR-10 Max 1.0 nozzle", 1),
]

# Boquillas que Orca no trae: se sacan de la más parecida de la misma impresora (misma
# máquina con otro diámetro; sus procesos con alturas de capa y anchos de línea a escala; los
# mismos filamentos). (fabricante, nueva, de cuál sale, diámetro)
DERIVED = [
    ("Creality", "Creality CR-10 Max 0.2 nozzle", "Creality CR-10 Max 0.4 nozzle", 0.2),
    ("Creality", "Creality CR-10 Max 0.6 nozzle", "Creality CR-10 Max 0.4 nozzle", 0.6),
    ("Creality", "Creality CR-10 Max 0.8 nozzle", "Creality CR-10 Max 0.4 nozzle", 0.8),
    ("Creality", "Creality CR-10 Max 1.0 nozzle", "Creality CR-10 Max 0.4 nozzle", 1.0),
    ("Creality", "Creality K1 (1.0 nozzle)", "Creality K1 (0.8 nozzle)", 1.0),
    ("Creality", "Creality K1C 1.0 nozzle", "Creality K1C 0.8 nozzle", 1.0),
    ("Creality", "Creality K1 Max (1.0 nozzle)", "Creality K1 Max (0.8 nozzle)", 1.0),
    ("Flashforge", "Flashforge AD5X 1.0 nozzle", "Flashforge AD5X 0.8 nozzle", 1.0),
]


# Variante «Creality CR-10 Max (Klipper)»: la CR-10 Max del taller (Klipper + Moonraker, extrusor
# directo, TMC2208, BLTouch). La «Creality CR-10 Max» de Orca queda tal cual para quien la tenga
# de fábrica (Marlin, Bowden); esta es una copia aparte con lo medido en la máquina real.
KLIPPER_MODEL = "Creality CR-10 Max (Klipper)"
KLIPPER_MACHINE = {
    "gcode_flavor": "klipper",
    "printer_model": KLIPPER_MODEL,
    # Extrusor directo: retracción corta (con 5 mm de Bowden se atasca y raspa el filamento).
    "retraction_length": ["1"],
    # Aceleración probada sin perder pasos; los procesos piden menos (ver KLIPPER_PROCESS).
    "machine_max_acceleration_extruding": ["2000", "2000"],
    "machine_max_acceleration_travel": ["2000", "2000"],
    "machine_max_acceleration_x": ["2000", "2000"],
    "machine_max_acceleration_y": ["2000", "2000"],
    # La rutina de inicio y fin (home, malla de la cama, calentar, purga) vive en printer.cfg.
    "machine_start_gcode": "M83 ; extruder relative mode\nSTART_PRINT BED_TEMP=[bed_temperature_initial_layer_single] EXTRUDER_TEMP=[nozzle_temperature_initial_layer]",
    "machine_end_gcode": "END_PRINT",
    "machine_pause_gcode": "PAUSE",
}
KLIPPER_PROCESS = {
    "default_acceleration": "1500",
    "travel_acceleration": "1500",
    "initial_layer_acceleration": "500",
}


def add_cr10max_klipper(out):
    """Copia las CR-10 Max ya aplanadas en `out` como «CR-10 Max (Klipper)» y las pone en el índice.

    Se puede correr sola: `python3 -I flatten_presets.py --solo-cr10max-klipper presets`
    (parte de los perfiles de fábrica ya escritos; no necesita Orca). Es idempotente."""
    index_path = out / "index.json"
    index = json.loads(index_path.read_text(encoding="utf-8"))
    originals = [p for p in index["printers"] if p["model"] == "Creality CR-10 Max"]
    index["printers"] = [p for p in index["printers"] if p["model"] != KLIPPER_MODEL]
    clones = []
    for p in originals:
        nz = fmt(p["nozzle"])
        new_name = f"Creality CR-10 Max (Klipper) {nz} nozzle"
        rename = lambda n: n.replace("@Creality CR-10 Max", "@Creality CR-10 Max (Klipper)").replace("@Creality CR10Max", f"@Creality CR-10 Max (Klipper) {nz} nozzle")
        machine = json.loads((out / "machine" / file_name(p["name"])).read_text(encoding="utf-8"))
        machine.update(KLIPPER_MACHINE)
        machine["name"] = new_name
        machine["default_print_profile"] = rename(machine.get("default_print_profile", ""))
        (out / "machine" / file_name(new_name)).write_text(json.dumps(machine, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        processes = []
        for pn in p["processes"]:
            proc = json.loads((out / "process" / file_name(pn)).read_text(encoding="utf-8"))
            proc.update(KLIPPER_PROCESS)
            proc["name"] = rename(pn)
            proc["compatible_printers"] = [new_name]
            (out / "process" / file_name(proc["name"])).write_text(json.dumps(proc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
            processes.append(proc["name"])
        clone = dict(p, name=new_name, model=KLIPPER_MODEL, processes=processes,
                     default_process=rename(p["default_process"]))
        clones.append(clone)
    at = max(i for i, q in enumerate(index["printers"]) if q["model"] == "Creality CR-10 Max") + 1
    index["printers"][at:at] = clones
    index_path.write_text(json.dumps(index, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return len(clones)


def fmt(v):
    """Número como lo escribe Orca: «0.3», «0.12», «1.0»."""
    s = f"{round(v, 3):g}"
    return s if "." in s else s + ".0"


def derive(tables, name, base, nozzle):
    """Agrega a `tables` la máquina `name` y sus procesos, a escala desde `base` (0.4)."""
    m = resolve(tables["machine"], base)
    k = nozzle / float(m.get("nozzle_diameter", ["0.4"])[0])
    m = clean(m)
    m.update({
        "name": name,
        "instantiation": "true",
        "nozzle_diameter": [fmt(nozzle)],
        "printer_variant": fmt(nozzle),
        "max_layer_height": [fmt(0.75 * nozzle)],
        "min_layer_height": [fmt(float(m.get("min_layer_height", ["0.08"])[0]) * k)],
    })
    tables["machine"][name] = m
    for pname in list(tables["process"]):
        data = tables["process"][pname]
        if str(data.get("instantiation", "true")).lower() != "true":
            continue
        p = resolve(tables["process"], pname)
        if base not in p.get("compatible_printers", []):
            continue
        p = clean(p)
        for key, v in list(p.items()):
            if (key == "line_width" or key.endswith("_line_width")) and not str(v).endswith("%"):
                p[key] = fmt(float(v) * k)
        layer = float(p.get("layer_height", "0.2")) * k
        p["layer_height"] = fmt(layer)
        p["initial_layer_print_height"] = fmt(float(p.get("initial_layer_print_height", "0.2")) * k)
        label = re.sub(r"^[0-9.]+mm\s*", "", pname.split("@")[0]).strip()
        new = f"{layer:.2f}mm {label} @{name}"
        p.update({"name": new, "instantiation": "true", "compatible_printers": [name], "compatible_printers_condition": ""})
        tables["process"][new] = p


def bed_size(machine):
    """Cama (ancho, fondo, alto en mm) de `printable_area` y `printable_height`."""
    pts = []
    for p in machine.get("printable_area", []):
        try:
            x, y = (float(v) for v in str(p).split("x"))
            pts.append((x, y))
        except ValueError:
            pass
    if len(pts) < 3:
        return None
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    try:
        h = float(machine.get("printable_height", 0))
    except (TypeError, ValueError):
        h = 0.0
    return [round(max(xs) - min(xs), 2), round(max(ys) - min(ys), 2), round(h, 2)]
# Claves que solo sirven al gestor de perfiles de Orca.
DROP = {"inherits", "setting_id", "filament_id", "instantiation", "renamed_from", "is_custom_defined", "version"}


def load_dir(root: pathlib.Path):
    by_name = {}
    for kind in ("machine", "process", "filament"):
        for f in sorted((root / kind).rglob("*.json")):
            try:
                data = json.loads(f.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                continue
            name = data.get("name")
            if name:
                by_name.setdefault(kind, {})[name] = data
    return by_name


def resolve(table, name, seen=()):
    data = table[name]
    parent = data.get("inherits")
    base = {}
    if parent:
        if parent in seen:
            raise SystemExit(f"herencia circular en {name}")
        base = resolve(table, parent, seen + (name,))
    merged = dict(base)
    merged.update(data)
    return merged


def clean(data):
    return {k: v for k, v in data.items() if k not in DROP}


def file_name(name):
    return re.sub(r"[^A-Za-z0-9._@ ()+-]", "_", name) + ".json"


def main():
    if sys.argv[1] == "--solo-cr10max-klipper":
        print(add_cr10max_klipper(pathlib.Path(sys.argv[2])), "impresoras Klipper")
        return
    profiles = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    # Filamentos de base comunes (OrcaFilamentLibrary) por si un perfil hereda de ahí.
    lib = load_dir(profiles / "OrcaFilamentLibrary")
    by_vendor = {}
    for vendor in sorted({v for v, _, _ in PRINTERS}):
        tables = load_dir(profiles / vendor)
        for kind, t in lib.items():
            for n, d in t.items():
                tables.setdefault(kind, {}).setdefault(n, d)
        by_vendor[vendor] = tables
    for vendor, name, base, nozzle in DERIVED:
        derive(by_vendor[vendor], name, base, nozzle)
    # Los filamentos de una boquilla derivada son los de su 0.4
    alias = {name: base for _, name, base, _ in DERIVED}

    index = {"printers": []}
    written = {"machine": set(), "process": set(), "filament": set()}

    def write(kind, name, tables):
        if name in written[kind]:
            return
        data = clean(resolve(tables[kind], name))
        d = out / kind
        d.mkdir(parents=True, exist_ok=True)
        (d / file_name(name)).write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        written[kind].add(name)

    for vendor, printer, tools in PRINTERS:
        tables = by_vendor[vendor]
        machine = resolve(tables["machine"], printer)
        write("machine", printer, tables)
        processes, filaments = [], []
        for kind, bucket in (("process", processes), ("filament", filaments)):
            for name, data in sorted(tables[kind].items()):
                if str(data.get("instantiation", "true")).lower() != "true":
                    continue
                compat = resolve(tables[kind], name).get("compatible_printers", [])
                if printer in compat or (kind == "filament" and alias.get(printer) in compat):
                    write(kind, name, tables)
                    bucket.append(name)
        # El perfil por defecto puede venir heredado de otra impresora (la AD5X hereda el de
        # la AD5M Pro): si no está entre los compatibles se usa uno que sí.
        default_process = machine.get("default_print_profile", "")
        if default_process not in processes:
            # La de capa más cercana a la mitad de la boquilla (0.4 → 0.20 mm); a igual capa, «Standard»
            nozzle = float(machine.get("nozzle_diameter", ["0.4"])[0])

            def score(name):
                m = re.match(r"([0-9.]+)mm", name)
                layer = float(m.group(1)) if m else 99.0
                return (round(abs(layer - nozzle / 2), 3), "Standard" not in name)

            default_process = min(processes, key=score) if processes else ""
        default_filament = (machine.get("default_filament_profile") or [""])[0]
        if default_filament not in filaments:
            default_filament = next((f for f in filaments if "PLA" in f), filaments[0] if filaments else "")
        index["printers"].append({
            "name": printer,
            "vendor": vendor,
            "tools": tools,
            "bed": bed_size(machine),
            "model": machine.get("printer_model", printer),
            "nozzle": float(machine.get("nozzle_diameter", ["0.4"])[0]),
            "default_process": default_process,
            "default_filament": default_filament,
            "processes": processes,
            "filaments": filaments,
        })
    (out / "index.json").write_text(json.dumps(index, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    add_cr10max_klipper(out)
    for p in json.loads((out / "index.json").read_text(encoding="utf-8"))["printers"]:
        print(f"{p['name']}: {len(p['processes'])} procesos, {len(p['filaments'])} filamentos")


if __name__ == "__main__":
    main()
