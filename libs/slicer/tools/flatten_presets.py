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

VENDOR = "Creality"
PRINTERS = [
    "Creality K1 (0.4 nozzle)",
    "Creality K1C 0.4 nozzle",
    "Creality K1 Max (0.4 nozzle)",
]
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
    profiles = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    tables = load_dir(profiles / VENDOR)
    # Filamentos de base comunes (OrcaFilamentLibrary) por si un Creality hereda de ahí.
    lib = load_dir(profiles / "OrcaFilamentLibrary")
    for kind, t in lib.items():
        for n, d in t.items():
            tables.setdefault(kind, {}).setdefault(n, d)

    index = {"printers": []}
    written = {"machine": set(), "process": set(), "filament": set()}

    def write(kind, name):
        if name in written[kind]:
            return
        data = clean(resolve(tables[kind], name))
        d = out / kind
        d.mkdir(parents=True, exist_ok=True)
        (d / file_name(name)).write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        written[kind].add(name)

    for printer in PRINTERS:
        machine = resolve(tables["machine"], printer)
        write("machine", printer)
        processes, filaments = [], []
        for kind, bucket in (("process", processes), ("filament", filaments)):
            for name, data in sorted(tables[kind].items()):
                if str(data.get("instantiation", "true")).lower() != "true":
                    continue
                compat = resolve(tables[kind], name).get("compatible_printers", [])
                if printer in compat:
                    write(kind, name)
                    bucket.append(name)
        index["printers"].append({
            "name": printer,
            "model": machine.get("printer_model", printer),
            "nozzle": float(machine.get("nozzle_diameter", ["0.4"])[0]),
            "default_process": machine.get("default_print_profile", ""),
            "default_filament": (machine.get("default_filament_profile") or [""])[0],
            "processes": processes,
            "filaments": filaments,
        })
    (out / "index.json").write_text(json.dumps(index, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    for p in index["printers"]:
        print(f"{p['name']}: {len(p['processes'])} procesos, {len(p['filaments'])} filamentos")


if __name__ == "__main__":
    main()
