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
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/libs/slicer/bin}"
VERSION="${ORCA_VERSION:-2.4.2}"
WORK="${ORCA_WORK_DIR:-$HOME/.cargo-target/orca}"
JOBS="${ORCA_DEPS_JOBS:-8}"
IMAGE="localhost/pinocchio-orca-builder:24.04"
SRC="$WORK/OrcaSlicer-$VERSION"

# Objetivos de dependencias que necesita libslic3r (sin wxWidgets ni nada de la GUI).
DEPS=(dep_Boost dep_TBB dep_Cereal dep_Draco dep_NLopt dep_OpenVDB dep_OpenCV dep_Eigen dep_CGAL dep_OCCT dep_libnoise dep_Qhull)

mkdir -p "$WORK" "$OUT"

if [[ ! -d "$SRC" ]]; then
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

BUILD_CMDS="set -e
export CMAKE_POLICY_VERSION_MINIMUM=3.5 ORCA_DEPS_JOBS=$JOBS
cmake -S deps -B deps/build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build deps/build -j1 --target ${DEPS[*]}
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release -DSLIC3R_GUI=0 -DSLIC3R_STATIC=1 -DSLIC3R_PCH=0 \
  -DCMAKE_PREFIX_PATH=\$PWD/deps/build/OrcaSlicer_dep/usr/local
cmake --build build -j$JOBS --target pinocchio-slicer
strip build/src/pinocchio-engine/pinocchio-slicer"

if [[ -n "${ORCA_NO_CONTAINER:-}" ]]; then
  (cd "$SRC" && bash -c "$BUILD_CMDS")
else
  if ! podman image exists "$IMAGE"; then
    podman build -t "$IMAGE" -f "$ROOT/libs/slicer/engine/Containerfile" "$ROOT/libs/slicer/engine"
  fi
  podman run --rm -v "$WORK:/work:Z" -w "/work/OrcaSlicer-$VERSION" "$IMAGE" bash -c "$BUILD_CMDS"
fi

cp "$SRC/build/src/pinocchio-engine/pinocchio-slicer" "$OUT/"
echo "Listo: $OUT/pinocchio-slicer"
