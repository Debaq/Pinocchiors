# Se incluye desde src/CMakeLists.txt de OrcaSlicer (lo agrega scripts/build-orca-engine.sh),
# así hereda la configuración y los objetivos de libslic3r.
add_executable(pinocchio-slicer main.cpp nanosvg_impl.cpp)
target_link_libraries(pinocchio-slicer libslic3r cereal::cereal boost_headeronly)
target_compile_definitions(pinocchio-slicer PRIVATE -DBOOST_ALL_NO_LIB -DBOOST_SYSTEM_USE_UTF8)
if (NOT WIN32 AND NOT APPLE)
    # libstdc++ y libgcc estáticos: el binario corre en distros más nuevas que la de compilación.
    target_link_options(pinocchio-slicer PRIVATE -static-libstdc++ -static-libgcc)
endif ()
