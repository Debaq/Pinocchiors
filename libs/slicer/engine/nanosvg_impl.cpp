// libslic3r usa nanosvg, pero su implementación la pone la GUI de Orca (que no
// compilamos). Va sola en este archivo para que ningún otro include la tape.
#define NANOSVG_IMPLEMENTATION
#include "nanosvg/nanosvg.h"
