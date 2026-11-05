#!/usr/bin/env bash
# Compila el motor de laminado (pinocchio-slicer) sobre libslic3r de OrcaSlicer.
#
# El motor queda congelado en una versión de Orca (por defecto 2.4.2).
#   - Linux: dentro de un contenedor Ubuntu 24.04 (el mismo sistema que el CI de
#     Orca): las dependencias de Orca no compilan con compiladores mucho más nuevos.
#   - Windows (Git Bash con el entorno de MSVC cargado) y macOS (arm64): directo en
#     el sistema, como el CI de Orca. Solo se usa en el CI (slicer-engine.yml).
#
#   scripts/build-orca-engine.sh [salida]
#   → salida/pinocchio-slicer   (por defecto libs/slicer/bin)
#     En Windows: salida/pinocchio-slicer.exe y las DLL que necesita (OCCT, GMP,
#     MPFR, runtime de MSVC...): Orca las compila compartidas allá.
#
# Variables:
#   ORCA_VERSION     versión de Orca (2.4.2)
#   ORCA_WORK_DIR    carpeta de trabajo, grande (~15 GB) ($HOME/.cargo-target/orca).
#                    En Windows conviene una ruta corta (las rutas de las dependencias
#                    pasan el límite de 260 caracteres)
#   ORCA_DEPS_JOBS   compilaciones en paralelo de las dependencias (8)
#   ORCA_NO_CONTAINER=1  Linux: compilar en el sistema actual (CI con Ubuntu 24.04)
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

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  MINGW* | MSYS* | CYGWIN*) OS=windows ;;
  *) echo "Sistema no soportado: $(uname -s)" >&2; exit 1 ;;
esac

mkdir -p "$WORK" "$OUT"
WORK="$(cd "$WORK" && pwd)"
OUT="$(cd "$OUT" && pwd)"
SRC="$WORK/OrcaSlicer-$VERSION"

# Objetivos de dependencias que necesita libslic3r (sin wxWidgets ni nada de la GUI).
DEPS=(dep_Boost dep_TBB dep_Cereal dep_Draco dep_NLopt dep_OpenVDB dep_OpenCV dep_Eigen dep_CGAL dep_OCCT dep_libnoise dep_Qhull)
# En Linux el resto sale del sistema (paquetes del contenedor). En Windows y macOS Orca
# las compila: el CMake de Orca exige OpenSSL, CURL y GLFW aunque no haya GUI, y
# libslic3r usa OpenSSL. Las que no existen en un sistema se saltan (ver has_target).
if [[ "$OS" != linux ]]; then
  DEPS+=(dep_ZLIB dep_PNG dep_JPEG dep_EXPAT dep_FREETYPE dep_GMP dep_MPFR dep_OpenSSL dep_CURL dep_GLFW)
fi

# (se mira un archivo y no la carpeta: la caché del CI la crea al restaurar las dependencias)
if [[ ! -f "$SRC/CMakeLists.txt" ]]; then
  echo "Descargando OrcaSlicer $VERSION..."
  curl -fsSL "https://github.com/OrcaSlicer/OrcaSlicer/archive/refs/tags/v${VERSION}.tar.gz" -o "$WORK/orca.tar.gz"
  (cd "$WORK" && tar xzf orca.tar.gz)
  rm "$WORK/orca.tar.gz"
fi

# Parches (idempotentes) sobre la copia congelada (perl y no sed -i: el de macOS es otro):
# 1. ORCA_DEPS_JOBS limita el paralelismo de las dependencias (con 16 hilos se queda sin RAM).
if ! grep -q ORCA_DEPS_JOBS "$SRC/deps/CMakeLists.txt"; then
  perl -pi -e 's/^ProcessorCount\(NPROC\)$/ProcessorCount(NPROC)\nif (DEFINED ENV{ORCA_DEPS_JOBS})\n    set(NPROC \$ENV{ORCA_DEPS_JOBS})\nendif ()/' "$SRC/deps/CMakeLists.txt"
fi
# 2. Nuestro motor como subdirectorio de src/ (hereda libslic3r y sus opciones).
rm -rf "$SRC/src/pinocchio-engine"
cp -r "$ROOT/libs/slicer/engine" "$SRC/src/pinocchio-engine"
if ! grep -q pinocchio-engine "$SRC/src/CMakeLists.txt"; then
  echo 'add_subdirectory(pinocchio-engine)' >> "$SRC/src/CMakeLists.txt"
fi

PREFIX_REL="deps/build/OrcaSlicer_dep"

# ── Linux: igual que siempre (contenedor o sistema) ─────────────────────────
if [[ "$OS" == linux ]]; then
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
  exit 0
fi

# ── Windows y macOS: en el sistema, como build_release_vs.bat / build_release_macos.sh ──
case "$STAGE" in deps | engine | all) ;; *) echo "ORCA_STAGE desconocido: $STAGE" >&2; exit 1 ;; esac
export CMAKE_POLICY_VERSION_MINIMUM=3.5 ORCA_DEPS_JOBS="$JOBS"
cd "$SRC"

# Ruta que entiende CMake (en Windows C:/... y no /c/...)
native() { if [[ "$OS" == windows ]]; then cygpath -m "$1"; else echo "$1"; fi; }

if [[ "$OS" == windows ]]; then
  # Generador de Visual Studio (el que usa Orca); las dependencias van con msbuild
  GEN=(-A x64)
  BUILD_TOOL_ARGS=(-- -m)
  # OpenSSL se configura con perl: tiene que ser el de Windows (Strawberry) y no el de Git Bash
  for p in /c/Strawberry/perl/bin "/c/Program Files/Strawberry/perl/bin"; do
    if [[ -x "$p/perl.exe" ]]; then export PATH="$p:$PATH"; break; fi
  done
  # El link.exe de Git (coreutils) tapa al linker de MSVC
  if [[ "$(command -v link)" == /usr/bin/* ]]; then
    echo "Advertencia: el link.exe de Git va antes que el de MSVC" >&2
  fi
else
  # macOS arm64; sin Homebrew en la búsqueda (sus versiones no son las de Orca)
  GEN=(-G Ninja -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_DEPLOYMENT_TARGET=11.3
    "-DCMAKE_IGNORE_PREFIX_PATH=/opt/homebrew;/usr/local;/opt/local")
  BUILD_TOOL_ARGS=()
fi

# ¿Existe el objetivo en el proyecto de dependencias ya configurado?
has_target() {
  if [[ -f deps/build/build.ninja ]]; then
    grep -q "^build $1:" deps/build/build.ninja
  else
    [[ -f "deps/build/$1.vcxproj" ]]
  fi
}

if [[ "$STAGE" != engine ]]; then
  if [[ -f "$PREFIX_REL/.pinocchio-deps-ok" ]]; then
    echo "Dependencias ya instaladas"
  else
    cmake -S deps -B deps/build "${GEN[@]}" -DCMAKE_BUILD_TYPE=Release
    targets=()
    for t in "${DEPS[@]}"; do
      if has_target "$t"; then targets+=("$t"); else echo "(sin $t en este sistema)"; fi
    done
    echo "Dependencias: ${targets[*]}"
    if [[ "$OS" == windows ]]; then
      cmake --build deps/build --config Release --target "${targets[@]}" "${BUILD_TOOL_ARGS[@]}"
    else
      cmake --build deps/build --config Release -j1 --target "${targets[@]}"
    fi
    touch "$PREFIX_REL/.pinocchio-deps-ok"
  fi
fi
[[ "$STAGE" == deps ]] && exit 0

PREFIX="$(native "$SRC/$PREFIX_REL/usr/local")"
# Con cabecera precompilada, como el CI de Orca: en Windows el código de libslic3r cuenta
# con que ella trae boost/filesystem y windows.h (sin ella no compila)
cmake -S . -B build "${GEN[@]}" -DCMAKE_BUILD_TYPE=Release -DSLIC3R_GUI=0 -DSLIC3R_STATIC=1 -DSLIC3R_PCH=1 \
  "-DCMAKE_PREFIX_PATH=$PREFIX"
if [[ "$OS" == windows ]]; then
  cmake --build build --config Release --target pinocchio-slicer "${BUILD_TOOL_ARGS[@]}"
  EXE="build/src/pinocchio-engine/Release/pinocchio-slicer.exe"
else
  cmake --build build --config Release -j"$JOBS" --target pinocchio-slicer
  EXE="build/src/pinocchio-engine/pinocchio-slicer"
fi

if [[ "$OS" == macos ]]; then
  cp "$EXE" "$OUT/pinocchio-slicer"
  strip "$OUT/pinocchio-slicer"
  # En arm64 todo binario necesita firma; strip rompe la ad-hoc que puso el linker
  codesign --force --sign - "$OUT/pinocchio-slicer"
  # Solo bibliotecas del sistema: el motor viaja solo
  if otool -L "$OUT/pinocchio-slicer" | tail -n +2 | grep -vE '^[[:space:]]*(/usr/lib/|/System/)'; then
    echo "El motor depende de bibliotecas fuera del sistema (arriba)" >&2
    exit 1
  fi
  echo "Listo: $OUT/pinocchio-slicer"
  exit 0
fi

# Windows: el .exe y, recorriendo sus dependencias (dumpbin), las DLL que no son del
# sistema: las de las dependencias de Orca y la runtime de MSVC (se permite llevarla
# junto al programa; así no hace falta instalar el redistribuible).
rm -f "$OUT"/*.dll "$OUT/pinocchio-slicer.exe"
cp "$EXE" "$OUT/pinocchio-slicer.exe"
declare -A dll_at=()
while IFS= read -r f; do
  name="$(basename "$f" | tr '[:upper:]' '[:lower:]')"
  [[ -n "${dll_at[$name]:-}" ]] || dll_at[$name]="$f"
done < <(
  find "$SRC/$PREFIX_REL" -iname '*.dll'
  if [[ -n "${VCToolsRedistDir:-}" ]]; then
    find "$(cygpath -u "$VCToolsRedistDir")/x64" -ipath '*Microsoft.VC*.CRT*' -iname '*.dll' 2>/dev/null || true
  fi
)
queue=("$OUT/pinocchio-slicer.exe")
declare -A seen=()
while ((${#queue[@]})); do
  f="${queue[0]}"
  queue=("${queue[@]:1}")
  while IFS= read -r dep; do
    [[ -n "$dep" && -z "${seen[$dep]:-}" ]] || continue
    seen[$dep]=1
    if [[ -n "${dll_at[$dep]:-}" ]]; then
      cp "${dll_at[$dep]}" "$OUT/"
      queue+=("$OUT/$(basename "${dll_at[$dep]}")")
      echo "  + $(basename "${dll_at[$dep]}")"
    fi
  done < <(MSYS2_ARG_CONV_EXCL='*' dumpbin /nologo /dependents "$(cygpath -w "$f")" | tr -d '\r' |
    awk '{ n = tolower($1) } n ~ /\.dll$/ && NF == 1 { print n }')
done
echo "Listo: $OUT/pinocchio-slicer.exe ($(find "$OUT" -iname '*.dll' | wc -l) DLL)"
