#!/usr/bin/env bash
# Compila OpenCASCADE estático y mínimo para el CAD de Pinocchio (cad-occt).
#
# Solo modelado + STEP: sin visualización, Draw, OpenGL, freetype ni X11. Al
# enlazar estático entran solo los objetos que usa el puente, así que la app
# no arrastra la parte gráfica de OCCT.
#
#   scripts/build-occt.sh [prefijo] [versión]
#   → prefijo/include/opencascade, prefijo/lib/libTK*.a
#
# Después:
#   OCCT_STATIC=1 OCCT_INCLUDE_DIR=prefijo/include/opencascade \
#   OCCT_LIB_DIR=prefijo/lib CAD_REQUIRE_OCCT=1 cargo build --release -p pinocchio-app
set -euo pipefail

PREFIX="${1:-$PWD/occt-static}"
VERSION="${2:-7.9.3}"
TAG="V${VERSION//./_}"
WORK="${OCCT_WORK_DIR:-$PWD/occt-work}"
JOBS="${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"

mkdir -p "$WORK"
cd "$WORK"
# GitHub nombra la carpeta sin la "V" del tag (OCCT-7_9_3)
SRC="OCCT-${TAG#V}"
if [[ ! -d "$SRC" ]]; then
  echo "Descargando OCCT ${VERSION}..."
  curl -fsSL "https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/${TAG}.tar.gz" -o occt.tar.gz
  tar xzf occt.tar.gz
  rm occt.tar.gz
fi

# Toolkits que usa el puente (cpp/cad_occt.cpp); CMake agrega sus dependencias.
TOOLKITS="TKDESTEP;TKFeat;TKOffset;TKFillet;TKBool;TKBO;TKMesh;TKShHealing;TKPrim;TKTopAlgo"

cmake -S "$SRC" -B build -G "${CMAKE_GENERATOR:-Unix Makefiles}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DINSTALL_DIR="$PREFIX" \
  -DINSTALL_DIR_LAYOUT=Unix \
  -DBUILD_LIBRARY_TYPE=Static \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DBUILD_MODULE_FoundationClasses=ON \
  -DBUILD_MODULE_ModelingData=ON \
  -DBUILD_MODULE_ModelingAlgorithms=ON \
  -DBUILD_MODULE_DataExchange=OFF \
  -DBUILD_MODULE_ApplicationFramework=OFF \
  -DBUILD_MODULE_Visualization=OFF \
  -DBUILD_MODULE_Draw=OFF \
  -DBUILD_ADDITIONAL_TOOLKITS="$TOOLKITS" \
  -DBUILD_DOC_Overview=OFF \
  -DBUILD_SAMPLES_QT=OFF \
  -DUSE_FREETYPE=OFF \
  -DUSE_OPENGL=OFF \
  -DUSE_GLES2=OFF \
  -DUSE_XLIB=OFF \
  -DUSE_TK=OFF \
  -DUSE_FREEIMAGE=OFF \
  -DUSE_RAPIDJSON=OFF \
  -DUSE_DRACO=OFF \
  -DUSE_TBB=OFF \
  -DUSE_VTK=OFF

cmake --build build --config Release --parallel "$JOBS"
cmake --install build --config Release
echo "OCCT estático en $PREFIX"
