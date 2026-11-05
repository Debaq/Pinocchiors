#!/usr/bin/env bash
# Compila el motor de laminado (pinocchio-slicer) sobre libslic3r de OrcaSlicer.
#
# El motor queda congelado en una versión de Orca (por defecto 2.4.2). Se compila
# dentro de un contenedor Ubuntu 24.04 (el mismo sistema que el CI de Orca): las
# dependencias de Orca no compilan con compiladores mucho más nuevos.
#
#   scripts/build-orca-engine.sh [salida]
#   → salida/pinocchio-slicer   (por defecto libs/slicer/bin)
#
# Variables:
#   ORCA_VERSION     versión de Orca (2.4.2)
#   ORCA_WORK_DIR    carpeta de trabajo, grande (~15 GB) ($HOME/.cargo-target/orca)
#   ORCA_DEPS_JOBS   compilaciones en paralelo de las dependencias (8)
#   ORCA_NO_CONTAINER=1  compilar en el sistema actual (CI con Ubuntu 24.04)
#   ORCA_STAGE       deps (solo dependencias), engine (solo el motor) o all (por defecto)
#
# Las dependencias quedan instaladas en $ORCA_WORK_DIR/OrcaSlicer-<v>/deps/build/OrcaSlicer_dep
# (~600 MB, lo que guarda la caché del CI); si ya están (marca .pinocchio-deps-ok), no se recompilan.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/libs/slicer/bin}"
VERSION="${ORCA_VERSION:-2.4.2}"
WORK="${ORCA_WORK_DIR:-$HOME/.cargo-target/orca}"
JOBS="${ORCA_DEPS_JOBS:-8}"
STAGE="${ORCA_STAGE:-all}"
IMAGE="localhost/pinocchio-orca-builder:24.04"
SRC="$WORK/OrcaSlicer-$VERSION"

# Objetivos de dependencias que necesita libslic3r (sin wxWidgets ni nada de la GUI).
DEPS=(dep_Boost dep_TBB dep_Cereal dep_Draco dep_NLopt dep_OpenVDB dep_OpenCV dep_Eigen dep_CGAL dep_OCCT dep_libnoise dep_Qhull)

mkdir -p "$WORK" "$OUT"

# (se mira un archivo y no la carpeta: la caché del CI la crea al restaurar las dependencias)
if [[ ! -f "$SRC/CMakeLists.txt" ]]; then
  echo "Descargando OrcaSlicer $VERSION..."
  curl -fsSL "https://github.com/OrcaSlicer/OrcaSlicer/archive/refs/tags/v${VERSION}.tar.gz" -o "$WORK/orca.tar.gz"
  tar xzf "$WORK/orca.tar.gz" -C "$WORK"
  rm "$WORK/orca.tar.gz"
fi

# Parches (idempotentes) sobre la copia congelada:
# 1. ORCA_DEPS_JOBS limita el paralelismo de las dependencias (con 16 hilos se queda sin RAM).
if ! grep -q ORCA_DEPS_JOBS "$SRC/deps/CMakeLists.txt"; then
  sed -i 's/^ProcessorCount(NPROC)$/ProcessorCount(NPROC)\nif (DEFINED ENV{ORCA_DEPS_JOBS})\n    set(NPROC $ENV{ORCA_DEPS_JOBS})\nendif ()/' "$SRC/deps/CMakeLists.txt"
fi
# 2. Nuestro motor como subdirectorio de src/ (hereda libslic3r y sus opciones).
rm -rf "$SRC/src/pinocchio-engine"
cp -r "$ROOT/libs/slicer/engine" "$SRC/src/pinocchio-engine"
if ! grep -q pinocchio-engine "$SRC/src/CMakeLists.txt"; then
  echo 'add_subdirectory(pinocchio-engine)' >> "$SRC/src/CMakeLists.txt"
fi

PREFIX_REL="deps/build/OrcaSlicer_dep"
DEPS_CMDS="set -e
export CMAKE_POLICY_VERSION_MINIMUM=3.5 ORCA_DEPS_JOBS=$JOBS
if [ -f $PREFIX_REL/.pinocchio-deps-ok ]; then
  echo 'Dependencias ya instaladas'
else
  cmake -S deps -B deps/build -G Ninja -DCMAKE_BUILD_TYPE=Release
  cmake --build deps/build -j1 --target ${DEPS[*]}
  touch $PREFIX_REL/.pinocchio-deps-ok
fi"
ENGINE_CMDS="set -e
export CMAKE_POLICY_VERSION_MINIMUM=3.5
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DSLIC3R_GUI=0 -DSLIC3R_STATIC=1 -DSLIC3R_PCH=0 \
  -DCMAKE_PREFIX_PATH=\$PWD/$PREFIX_REL/usr/local
cmake --build build -j$JOBS --target pinocchio-slicer
strip build/src/pinocchio-engine/pinocchio-slicer"
case "$STAGE" in
  deps) BUILD_CMDS="$DEPS_CMDS" ;;
  engine) BUILD_CMDS="$ENGINE_CMDS" ;;
  all) BUILD_CMDS="$DEPS_CMDS
$ENGINE_CMDS" ;;
  *) echo "ORCA_STAGE desconocido: $STAGE" >&2; exit 1 ;;
esac

if [[ -n "${ORCA_NO_CONTAINER:-}" ]]; then
  (cd "$SRC" && bash -c "$BUILD_CMDS")
else
  if ! podman image exists "$IMAGE"; then
    podman build -t "$IMAGE" -f "$ROOT/libs/slicer/engine/Containerfile" "$ROOT/libs/slicer/engine"
  fi
  podman run --rm -v "$WORK:/work:Z" -w "/work/OrcaSlicer-$VERSION" "$IMAGE" bash -c "$BUILD_CMDS"
fi

if [[ "$STAGE" != deps ]]; then
  cp "$SRC/build/src/pinocchio-engine/pinocchio-slicer" "$OUT/"
  echo "Listo: $OUT/pinocchio-slicer"
fi
