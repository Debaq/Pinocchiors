#!/bin/sh
# Como build.sh, pero contra un OCCT estático propio (scripts/build-occt.sh), p. ej. la 8.0.1:
#   OCCT_PREFIX=~/.cargo-target/occt-8.0.1 libs/cad/occt/bench/build8.sh t_occt8
set -e
here=$(cd "$(dirname "$0")" && pwd)
prefix=${OCCT_PREFIX:-$HOME/.cargo-target/occt-8.0.1}
inc=$prefix/include/opencascade
out=${BENCH_OUT:-$here/out8}
mkdir -p "$out"
libs=$(ls "$prefix"/lib/libTK*.a | sed 's|.*/lib\(TK[^.]*\)\.a|-l\1|' | tr '\n' ' ')
g++ -std=c++17 -O2 -w -fPIC -c -I"$inc" -I"$here/../cpp" "$here/../cpp/cad_occt.cpp" -o "$out/bridge.o"
g++ -std=c++17 -O2 -w -I"$inc" -I"$here/../cpp" "$here/$1.cpp" "$out/bridge.o" \
  -L"$prefix/lib" -Wl,--start-group $libs -Wl,--end-group -lpthread -ldl -o "$out/$1"
name=$1; shift; "$out/$name" "$@"
