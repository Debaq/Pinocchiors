#!/bin/sh
# Banco de pruebas del puente contra OCCT real, sin Rust: compila cad_occt.cpp y la prueba $1
# (t_blend, …) y la corre. Uso: libs/cad/occt/bench/build.sh t_blend
# OCCT_INC apunta a los encabezados (por defecto /usr/include/opencascade). Se saca la llamada a
# WriteStream (no existe antes de OCCT 7.8 y el banco no exporta STEP).
set -e
here=$(cd "$(dirname "$0")" && pwd)
inc=${OCCT_INC:-/usr/include/opencascade}
out=${BENCH_OUT:-$here/out}
mkdir -p "$out"
libs="-lTKXDESTEP -lTKSTEP -lTKSTEPAttr -lTKSTEP209 -lTKSTEPBase -lTKXCAF -lTKVCAF -lTKV3d -lTKHLR -lTKService -lTKCAF -lTKLCAF -lTKCDF -lTKXSBase -lTKFeat -lTKOffset -lTKFillet -lTKBool -lTKBO -lTKMesh -lTKShHealing -lTKPrim -lTKTopAlgo -lTKGeomAlgo -lTKBRep -lTKGeomBase -lTKG3d -lTKG2d -lTKMath -lTKernel"
sed 's/w\.WriteStream(os)/IFSelect_RetFail/' "$here/../cpp/cad_occt.cpp" > "$out/bridge.cpp"
g++ -std=c++17 -O1 -w -fPIC -c -I"$inc" -I"$here/../cpp" "$out/bridge.cpp" -o "$out/bridge.o"
g++ -std=c++17 -O1 -w -I"$inc" -I"$here/../cpp" "$here/$1.cpp" "$out/bridge.o" $libs -o "$out/$1"
"$out/$1"
