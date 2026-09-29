#!/usr/bin/env bash
#
# Pinocchio — dev, build y ejecución de la app de escritorio (Tauri) y la web
# Generado con tauri-script-template (https://github.com/Debaq/tauri-script-template)
# y adaptado al monorepo: frontend en apps/web, Tauri en apps/desktop.
# Uso: ./pinocchio.sh [comando]   (sin argumentos = menú interactivo)
#

set -uo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_DIR="$PROJECT_DIR/apps/web"
TAURI_DIR="$PROJECT_DIR/apps/desktop"
DIST_DIR="$APP_DIR/dist"

APP_NAME=$(grep '"name"' "$APP_DIR/package.json" 2>/dev/null | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
APP_NAME="${APP_NAME:-app}"
VERSION=$(grep '"version"' "$APP_DIR/package.json" 2>/dev/null | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
VERSION="${VERSION:-0.0.0}"
PRODUCT_NAME=$(grep '"productName"' "$TAURI_DIR/tauri.conf.json" 2>/dev/null | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
PRODUCT_NAME="${PRODUCT_NAME:-$APP_NAME}"
# Nombre del binario que deja cargo (el paquete, no el productName)
# Puerto de Vite (devUrl); con strictPort el dev no arranca si está tomado
DEV_PORT=$(grep '"devUrl"' "$TAURI_DIR/tauri.conf.json" 2>/dev/null | head -1 | sed 's/.*:\([0-9]\+\).*/\1/')
DEV_PORT="${DEV_PORT:-5173}"
BIN_NAME=$(grep '^name' "$TAURI_DIR/Cargo.toml" 2>/dev/null | head -1 | sed 's/.*= *"\(.*\)".*/\1/')

# ── i18n (baked at generation time) ──────────────────────────────────────────
LANG_UI="es"
if [[ "$LANG_UI" == "es" ]]; then
    T_CHECK_DEPS="Verificando dependencias"
    T_NOT_FOUND="no encontrado"
    T_TAURI_NOT_FOUND="tauri-cli no encontrado (./pinocchio.sh install)"
    T_MISSING_DEPS="Faltan dependencias requeridas"
    T_ALL_DEPS_OK="Todas las dependencias disponibles"
    T_INSTALL_DEPS="Instalando dependencias"
    T_NPM_INSTALLED="Dependencias npm instaladas"
    T_DEV_TAURI="Modo desarrollo (Tauri)"
    T_DEV_STARTING="Iniciando Tauri + Vite hot reload..."
    T_PORT_BUSY_FMT="El puerto %s ya está en uso:"
    T_PORT_USING_FMT="Usando el puerto %s"
    T_PORT_NONE_FMT="No hay puertos libres entre %s y %s"
    T_DEV_WEB="Frontend dev (solo navegador)"
    T_DEV_WEB_STARTING="Iniciando Vite dev server... (Ctrl+C para detener)"
    T_BUILD_DESKTOP_FMT="Build %s desktop v%s"
    T_BUILD_COMPILING="Compilando app Tauri (release, solo binario)..."
    T_BUILD_DONE_FMT="Build Tauri completado en %s"
    T_BUILD_DEBUG="Build Tauri debug"
    T_BUILD_DEBUG_COMPILING="Compilando en modo debug..."
    T_BUILD_DEBUG_DONE_FMT="Build debug completado en %s"
    T_BUILD_WEB_FMT="Build %s web v%s"
    T_BUILD_WEB_COMPILING="Compilando frontend (Vite build)..."
    T_BUILD_WEB_DONE_FMT="Frontend compilado en %s"
    T_BUILD_ALL_FMT="Build completo %s v%s (Tauri + Web)"
    T_BUILD_ALL_STEP1="Paso 1/2: Build Tauri desktop..."
    T_BUILD_ALL_STEP2="Paso 2/2: Build Web..."
    T_BUILD_ALL_DONE_FMT="Build completo en %s"
    T_BINARY_FMT="Binario: %s"
    T_WEB_ZIP_FMT="ZIP web: %s"
    T_DIST_NOT_FOUND="dist/ no encontrado, ¿fallo build web?"
    T_ARTIFACTS_IN="Artefactos en out/"
    T_NO_ARTIFACTS="Sin artefactos en out/"
    T_BIN_NOT_FOUND="Binario no encontrado. Ejecuta 'build' primero."
    T_RUNNING_FMT="Ejecutando %s v%s"
    T_CLEANING="Limpieza"
    T_CLEAN_DOING="Limpiando dist/ + out/ + crates del workspace (el resto del target compartido queda)..."
    T_CLEAN_DONE="Limpio"
    T_DIRECTORY="Directorio:"
    T_NODE_LBL="Node:"
    T_RUST_LBL="Rust:"
    T_TAURI_LBL="Tauri:"
    T_BRANCH="Rama:"
    T_COMMIT="Commit:"
    T_ARTIFACTS_LBL="Artefactos:"
    T_PRESS_ENTER="Presiona ENTER para volver al menú..."
    T_MENU_DEV="DESARROLLO"
    T_MENU_BUILD="BUILD"
    T_MENU_MANAGE="GESTIÓN"
    T_OPT_DEV_TAURI="Dev Tauri"
    T_OPT_DEV_TAURI_DESC="Tauri + Vite hot reload"
    T_OPT_DEV_WEB="Dev Web"
    T_OPT_DEV_WEB_DESC="Solo frontend en navegador"
    T_OPT_BUILD="Build Tauri"
    T_OPT_BUILD_DESC="Desktop release (solo binario)"
    T_OPT_BUILD_DEBUG="Build Tauri debug"
    T_OPT_BUILD_DEBUG_DESC="Sin optimizaciones"
    T_OPT_BUILD_WEB="Build Web"
    T_OPT_BUILD_WEB_DESC="Frontend (dist/ + .zip)"
    T_OPT_BUILD_ALL="Build TODO"
    T_OPT_BUILD_ALL_DESC="Tauri + Web secuencial"
    T_OPT_RUN="Ejecutar app"
    T_OPT_RUN_DESC="Lanzar binario release"
    T_OPT_INSTALL="Instalar deps"
    T_OPT_INSTALL_DESC="npm install"
    T_OPT_INFO="Info proyecto"
    T_OPT_INFO_DESC="Versiones y artefactos"
    T_OPT_CLEAN="Limpiar"
    T_OPT_CLEAN_DESC="dist/ + out/ + crates propios"
    T_EXIT="Salir"
    T_OPTION="Opción"
    T_BYE="Hasta luego"
    T_INVALID_FMT="Opción inválida: %s"
    T_USAGE="Uso:"
    T_USAGE_MENU="(menú interactivo)"
    T_HELP_DEV="Tauri + Vite hot reload"
    T_HELP_DEV_WEB="Solo frontend en navegador"
    T_HELP_BUILD="Build Tauri release (solo binario)"
    T_HELP_BUILD_DEBUG="Build Tauri debug"
    T_HELP_BUILD_WEB="Build frontend (dist/ + .zip)"
    T_HELP_BUILD_ALL="Build completo Tauri + Web"
    T_HELP_RUN="Ejecutar binario release"
    T_HELP_INSTALL="npm install"
    T_HELP_INFO="Info del proyecto"
    T_HELP_CLEAN="Limpiar dist/ + out/ + crates propios"
    T_HELP_HELP="Esta ayuda"
    T_UNKNOWN_CMD_FMT="Comando desconocido: %s"
else
    T_CHECK_DEPS="Checking dependencies"
    T_NOT_FOUND="not found"
    T_TAURI_NOT_FOUND="tauri-cli not found (npm i -D @tauri-apps/cli)"
    T_MISSING_DEPS="Missing required deps"
    T_ALL_DEPS_OK="All deps available"
    T_INSTALL_DEPS="Installing deps"
    T_NPM_INSTALLED="npm deps installed"
    T_DEV_TAURI="Dev mode (Tauri)"
    T_DEV_STARTING="Starting Tauri + Vite hot reload..."
    T_PORT_BUSY_FMT="Port %s is already in use:"
    T_PORT_USING_FMT="Using port %s"
    T_PORT_NONE_FMT="No free ports between %s and %s"
    T_DEV_WEB="Frontend dev (browser only)"
    T_DEV_WEB_STARTING="Starting Vite dev server... (Ctrl+C to stop)"
    T_BUILD_DESKTOP_FMT="Build %s desktop v%s"
    T_BUILD_COMPILING="Compiling Tauri app (release, binary only)..."
    T_BUILD_DONE_FMT="Tauri build done in %s"
    T_BUILD_DEBUG="Tauri debug build"
    T_BUILD_DEBUG_COMPILING="Compiling debug..."
    T_BUILD_DEBUG_DONE_FMT="Debug build done in %s"
    T_BUILD_WEB_FMT="Build %s web v%s"
    T_BUILD_WEB_COMPILING="Compiling frontend (Vite build)..."
    T_BUILD_WEB_DONE_FMT="Frontend built in %s"
    T_BUILD_ALL_FMT="Full build %s v%s (Tauri + Web)"
    T_BUILD_ALL_STEP1="Step 1/2: Tauri desktop build..."
    T_BUILD_ALL_STEP2="Step 2/2: Web build..."
    T_BUILD_ALL_DONE_FMT="Full build in %s"
    T_BINARY_FMT="Binary: %s"
    T_WEB_ZIP_FMT="Web ZIP: %s"
    T_DIST_NOT_FOUND="dist/ not found, web build failed?"
    T_ARTIFACTS_IN="Artifacts in out/"
    T_NO_ARTIFACTS="No artifacts in out/"
    T_BIN_NOT_FOUND="Binary not found. Run 'build' first."
    T_RUNNING_FMT="Running %s v%s"
    T_CLEANING="Cleaning"
    T_CLEAN_DOING="Cleaning dist/ + out/ + cargo clean..."
    T_CLEAN_DONE="Clean"
    T_DIRECTORY="Directory:"
    T_NODE_LBL="Node:"
    T_RUST_LBL="Rust:"
    T_TAURI_LBL="Tauri:"
    T_BRANCH="Branch:"
    T_COMMIT="Commit:"
    T_ARTIFACTS_LBL="Artifacts:"
    T_PRESS_ENTER="Press ENTER to return to menu..."
    T_MENU_DEV="DEV"
    T_MENU_BUILD="BUILD"
    T_MENU_MANAGE="MANAGE"
    T_OPT_DEV_TAURI="Dev Tauri"
    T_OPT_DEV_TAURI_DESC="Tauri + Vite hot reload"
    T_OPT_DEV_WEB="Dev Web"
    T_OPT_DEV_WEB_DESC="Frontend only in browser"
    T_OPT_BUILD="Build Tauri"
    T_OPT_BUILD_DESC="Desktop release (binary only)"
    T_OPT_BUILD_DEBUG="Build Tauri debug"
    T_OPT_BUILD_DEBUG_DESC="No optimizations"
    T_OPT_BUILD_WEB="Build Web"
    T_OPT_BUILD_WEB_DESC="Frontend (dist/ + .zip)"
    T_OPT_BUILD_ALL="Build ALL"
    T_OPT_BUILD_ALL_DESC="Tauri + Web sequential"
    T_OPT_RUN="Run app"
    T_OPT_RUN_DESC="Launch release binary"
    T_OPT_INSTALL="Install deps"
    T_OPT_INSTALL_DESC="npm install"
    T_OPT_INFO="Project info"
    T_OPT_INFO_DESC="Versions and artifacts"
    T_OPT_CLEAN="Clean"
    T_OPT_CLEAN_DESC="dist/ + out/ + cargo clean"
    T_EXIT="Exit"
    T_OPTION="Option"
    T_BYE="Bye"
    T_INVALID_FMT="Invalid option: %s"
    T_USAGE="Usage:"
    T_USAGE_MENU="(interactive menu)"
    T_HELP_DEV="Tauri + Vite hot reload"
    T_HELP_DEV_WEB="Frontend only in browser"
    T_HELP_BUILD="Tauri release (binary only)"
    T_HELP_BUILD_DEBUG="Tauri debug build"
    T_HELP_BUILD_WEB="Frontend (dist/ + .zip)"
    T_HELP_BUILD_ALL="Full Tauri + Web build"
    T_HELP_RUN="Run release binary"
    T_HELP_INSTALL="npm install"
    T_HELP_INFO="Project info"
    T_HELP_CLEAN="Clean dist/ + out/ + cargo clean"
    T_HELP_HELP="This help"
    T_UNKNOWN_CMD_FMT="Unknown command: %s"
fi

_detect_cargo_target() {
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        echo "$CARGO_TARGET_DIR"
    elif grep -q 'target-dir' ~/.cargo/config.toml 2>/dev/null; then
        grep 'target-dir' ~/.cargo/config.toml | head -1 | sed 's/.*= *"\(.*\)".*/\1/' | sed "s|~|$HOME|"
    else
        echo "$PROJECT_DIR/target"
    fi
}
CARGO_TARGET="$(_detect_cargo_target)"

find_binary() {
    for dir in "$CARGO_TARGET/release" "$PROJECT_DIR/target/release"; do
        for name in "$BIN_NAME" "$PRODUCT_NAME" "$APP_NAME"; do
            if [[ -f "$dir/$name" ]]; then
                echo "$dir/$name"
                return 0
            fi
        done
    done
    return 1
}

# El CLI de Tauri está en apps/web (npm) pero solo reconoce el proyecto
# parado donde está tauri.conf.json
tauri_cli() {
    (cd "$TAURI_DIR" && "$APP_DIR/node_modules/.bin/tauri" "$@")
}

info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
header()  { echo -e "\n${BOLD}${CYAN}=== $* ===${NC}\n"; }

elapsed() {
    local start=$1
    local end=$(date +%s)
    local diff=$((end - start))
    echo "$((diff / 60))m $((diff % 60))s"
}

pause_after() {
    echo ""
    echo -e "${DIM}$T_PRESS_ENTER${NC}"
    read -r
}

check_deps() {
    header "$T_CHECK_DEPS"
    local missing=0
    for cmd in node npm cargo rustc; do
        if command -v "$cmd" &>/dev/null; then
            success "$cmd -> $(command $cmd --version 2>/dev/null | head -1)"
        else
            error "$cmd $T_NOT_FOUND"
            missing=1
        fi
    done
    if tauri_cli --version &>/dev/null; then
        success "tauri-cli -> $(tauri_cli --version 2>/dev/null)"
    else
        error "$T_TAURI_NOT_FOUND"
        missing=1
    fi
    [[ $missing -eq 1 ]] && { error "$T_MISSING_DEPS"; return 1; }
    success "$T_ALL_DEPS_OK"
}

port_busy() {
    ss -ltnH "sport = :$1" 2>/dev/null | grep -q .
}

# Si el puerto de devUrl está tomado (otro Vite, otra app), muestra quién lo
# tiene y deja en DEV_PORT el siguiente libre
pick_dev_port() {
    local base=$DEV_PORT port pid
    port_busy "$base" || return 0
    warn "$(printf "$T_PORT_BUSY_FMT" "$base")"
    for pid in $(ss -ltnpH "sport = :$base" 2>/dev/null | grep -o 'pid=[0-9]*' | cut -d= -f2 | sort -u); do
        echo "    $(ps -o pid=,etime=,args= -p "$pid" 2>/dev/null)"
    done
    for ((port = base + 1; port <= base + 50; port++)); do
        if ! port_busy "$port"; then
            DEV_PORT=$port
            info "$(printf "$T_PORT_USING_FMT" "$DEV_PORT")"
            return 0
        fi
    done
    error "$(printf "$T_PORT_NONE_FMT" "$((base + 1))" "$((base + 50))")"
    return 1
}

cmd_install() {
    header "$T_INSTALL_DEPS"
    cd "$APP_DIR"
    npm install
    success "$T_NPM_INSTALLED"
}

cmd_dev() {
    header "$T_DEV_TAURI"
    check_deps || return
    pick_dev_port || return
    cd "$APP_DIR"
    info "$T_DEV_STARTING"
    # Tauri espera a Vite en devUrl: los dos van al puerto elegido
    local config
    config=$(printf '{"build":{"devUrl":"http://localhost:%s","beforeDevCommand":{"script":"npm run dev -- --port %s","cwd":"../web"}}}' "$DEV_PORT" "$DEV_PORT")
    tauri_cli dev --config "$config" || true
}

cmd_dev_web() {
    header "$T_DEV_WEB"
    pick_dev_port || return
    cd "$APP_DIR"
    info "$T_DEV_WEB_STARTING"
    npm run dev -- --port "$DEV_PORT" || true
}

cmd_build() {
    header "$(printf "$T_BUILD_DESKTOP_FMT" "$PRODUCT_NAME" "$VERSION")"
    check_deps || return
    local start=$(date +%s)
    cd "$APP_DIR"
    info "$T_BUILD_COMPILING"
    tauri_cli build --no-bundle
    success "$(printf "$T_BUILD_DONE_FMT" "$(elapsed $start)")"
    collect_artifacts_tauri
}

cmd_build_debug() {
    header "$T_BUILD_DEBUG"
    cd "$APP_DIR"
    local start=$(date +%s)
    info "$T_BUILD_DEBUG_COMPILING"
    tauri_cli build --debug --no-bundle
    success "$(printf "$T_BUILD_DEBUG_DONE_FMT" "$(elapsed $start)")"
}

cmd_build_web() {
    header "$(printf "$T_BUILD_WEB_FMT" "$APP_NAME" "$VERSION")"
    local start=$(date +%s)
    cd "$APP_DIR"
    info "$T_BUILD_WEB_COMPILING"
    npm run build
    success "$(printf "$T_BUILD_WEB_DONE_FMT" "$(elapsed $start)")"
    collect_artifacts_web
}

cmd_build_all() {
    header "$(printf "$T_BUILD_ALL_FMT" "$APP_NAME" "$VERSION")"
    check_deps || return
    local start=$(date +%s)
    info "$T_BUILD_ALL_STEP1"
    tauri_cli build --no-bundle
    collect_artifacts_tauri
    cd "$APP_DIR"
    info "$T_BUILD_ALL_STEP2"
    npm run build
    collect_artifacts_web
    echo ""
    success "$(printf "$T_BUILD_ALL_DONE_FMT" "$(elapsed $start)")"
    echo ""
    list_artifacts
}

collect_artifacts_tauri() {
    local out="$PROJECT_DIR/out"
    mkdir -p "$out"
    local bin
    bin="$(find_binary || echo '')"
    if [[ -f "$bin" ]]; then
        local target="$out/$(basename "$bin").bin"
        cp "$bin" "$target"
        chmod +x "$target"
        success "$(printf "$T_BINARY_FMT" "$target")"
    fi
}

collect_artifacts_web() {
    local out="$PROJECT_DIR/out"
    mkdir -p "$out"
    if [[ -d "$DIST_DIR" ]]; then
        local webzip="$out/${APP_NAME}-web-v${VERSION}.zip"
        cd "$DIST_DIR"
        zip -rq "$webzip" .
        cd "$PROJECT_DIR"
        success "$(printf "$T_WEB_ZIP_FMT" "$webzip")"
    else
        warn "$T_DIST_NOT_FOUND"
    fi
}

list_artifacts() {
    local out="$PROJECT_DIR/out"
    if [[ -d "$out" ]] && [[ -n "$(ls -A "$out" 2>/dev/null)" ]]; then
        header "$T_ARTIFACTS_IN"
        ls -lh "$out" | tail -n +2
    else
        warn "$T_NO_ARTIFACTS"
    fi
}

cmd_run() {
    local bin
    bin="$(find_binary || echo '')"
    if [[ ! -f "$bin" ]]; then
        error "$T_BIN_NOT_FOUND"
        return 1
    fi
    header "$(printf "$T_RUNNING_FMT" "$PRODUCT_NAME" "$VERSION")"
    "$bin" "$@" || true
}

cmd_clean() {
    header "$T_CLEANING"
    info "$T_CLEAN_DOING"
    rm -rf "$DIST_DIR" "$PROJECT_DIR/out"
    # CARGO_TARGET_DIR es compartido entre proyectos: solo se borran los
    # crates de este workspace, no las dependencias ni lo de otros proyectos
    local pkg
    for pkg in $(grep -h '^name *=' "$PROJECT_DIR"/libs/*/Cargo.toml "$PROJECT_DIR"/libs/*/*/Cargo.toml "$PROJECT_DIR"/apps/*/Cargo.toml 2>/dev/null | sed 's/.*= *"\(.*\)".*/\1/'); do
        (cd "$PROJECT_DIR" && cargo clean -q -p "$pkg" 2>/dev/null)
    done
    success "$T_CLEAN_DONE"
}

cmd_info() {
    header "$PRODUCT_NAME v$VERSION"
    echo -e "${BOLD}${T_DIRECTORY}${NC}  $PROJECT_DIR"
    echo -e "${BOLD}${T_NODE_LBL}${NC}       $(node --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}${T_RUST_LBL}${NC}       $(rustc --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}${T_TAURI_LBL}${NC}      $(tauri_cli --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}${T_BRANCH}${NC}     $(git -C "$PROJECT_DIR" branch --show-current 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}${T_COMMIT}${NC}     $(git -C "$PROJECT_DIR" log --oneline -1 2>/dev/null || echo 'N/A')"
    echo ""
    if [[ -d "$PROJECT_DIR/out" ]] && [[ -n "$(ls -A "$PROJECT_DIR/out" 2>/dev/null)" ]]; then
        echo -e "${BOLD}${T_ARTIFACTS_LBL}${NC}"
        ls -lh "$PROJECT_DIR/out" | tail -n +2
    fi
}

show_banner() {
    clear
    echo -e "${BOLD}${CYAN}"
    cat <<'BANNER_EOF'
 ____   ___  _   _   ___    ____   ____  _   _  ___   ___  
|  _ \ |_ _|| \ | | / _ \  / ___| / ___|| | | ||_ _| / _ \ 
| |_) | | | |  \| || | | || |    | |    | |_| | | | | | | |
|  __/  | | | |\  || |_| || |___ | |___ |  _  | | | | |_| |
|_|    |___||_| \_| \___/  \____| \____||_| |_||___| \___/ 
BANNER_EOF
    echo -e "${NC}"
    echo -e "${DIM}  v$VERSION · $(git -C "$PROJECT_DIR" branch --show-current 2>/dev/null || echo '-') · $(git -C "$PROJECT_DIR" log --oneline -1 2>/dev/null | cut -c1-50 || echo '-')${NC}"
    echo ""
}

show_menu() {
    echo -e "${BOLD} ${T_MENU_DEV}${NC}"
    echo -e "  ${GREEN}1${NC})  ${T_OPT_DEV_TAURI}          ${DIM}${T_OPT_DEV_TAURI_DESC}${NC}"
    echo -e "  ${GREEN}2${NC})  ${T_OPT_DEV_WEB}            ${DIM}${T_OPT_DEV_WEB_DESC}${NC}"
    echo ""
    echo -e "${BOLD} ${T_MENU_BUILD}${NC}"
    echo -e "  ${YELLOW}3${NC})  ${T_OPT_BUILD}        ${DIM}${T_OPT_BUILD_DESC}${NC}"
    echo -e "  ${YELLOW}4${NC})  ${T_OPT_BUILD_DEBUG}  ${DIM}${T_OPT_BUILD_DEBUG_DESC}${NC}"
    echo -e "  ${YELLOW}5${NC})  ${T_OPT_BUILD_WEB}          ${DIM}${T_OPT_BUILD_WEB_DESC}${NC}"
    echo -e "  ${MAGENTA}6${NC})  ${T_OPT_BUILD_ALL}          ${DIM}${T_OPT_BUILD_ALL_DESC}${NC}"
    echo ""
    echo -e "${BOLD} ${T_MENU_MANAGE}${NC}"
    echo -e "  ${BLUE}7${NC})  ${T_OPT_RUN}        ${DIM}${T_OPT_RUN_DESC}${NC}"
    echo -e "  ${BLUE}8${NC})  ${T_OPT_INSTALL}       ${DIM}${T_OPT_INSTALL_DESC}${NC}"
    echo -e "  ${BLUE}9${NC})  ${T_OPT_INFO}       ${DIM}${T_OPT_INFO_DESC}${NC}"
    echo -e "  ${RED}10${NC}) ${T_OPT_CLEAN}             ${DIM}${T_OPT_CLEAN_DESC}${NC}"
    echo ""
    echo -e "  ${BOLD}0${NC})  ${T_EXIT}"
    echo ""
}

menu_loop() {
    while true; do
        show_banner
        show_menu
        echo -ne "${BOLD}  ${T_OPTION}: ${NC}"
        read -r choice
        case "${choice// /}" in
            1)  cmd_dev;            pause_after ;;
            2)  cmd_dev_web;        pause_after ;;
            3)  cmd_build;          pause_after ;;
            4)  cmd_build_debug;    pause_after ;;
            5)  cmd_build_web;      pause_after ;;
            6)  cmd_build_all;      pause_after ;;
            7)  cmd_run;            pause_after ;;
            8)  cmd_install;        pause_after ;;
            9)  cmd_info;           pause_after ;;
            10) cmd_clean;          pause_after ;;
            0|q|exit|salir) echo -e "\n${GREEN}${T_BYE}${NC}"; exit 0 ;;
            "") ;;
            *)  error "$(printf "$T_INVALID_FMT" "$choice")"; sleep 1 ;;
        esac
    done
}

cmd_help() {
    echo -e "${BOLD}${CYAN}$PRODUCT_NAME v$VERSION${NC}"
    echo ""
    echo -e "${BOLD}${T_USAGE}${NC} ./$(basename "$0") [command]"
    echo -e "       ./$(basename "$0")          ${DIM}${T_USAGE_MENU}${NC}"
    echo ""
    echo "  dev            $T_HELP_DEV"
    echo "  dev:web        $T_HELP_DEV_WEB"
    echo "  build          $T_HELP_BUILD"
    echo "  build:debug    $T_HELP_BUILD_DEBUG"
    echo "  build:web      $T_HELP_BUILD_WEB"
    echo "  build:all      $T_HELP_BUILD_ALL"
    echo "  run            $T_HELP_RUN"
    echo "  install        $T_HELP_INSTALL"
    echo "  info           $T_HELP_INFO"
    echo "  clean          $T_HELP_CLEAN"
    echo "  help           $T_HELP_HELP"
}

main() {
    cd "$PROJECT_DIR"
    if [[ $# -eq 0 ]]; then
        menu_loop
        exit 0
    fi
    case "$1" in
        dev)            cmd_dev ;;
        dev:web)        cmd_dev_web ;;
        build)          cmd_build ;;
        build:debug)    cmd_build_debug ;;
        build:web)      cmd_build_web ;;
        build:all)      cmd_build_all ;;
        run)            shift; cmd_run "$@" ;;
        install)        cmd_install ;;
        info)           cmd_info ;;
        clean)          cmd_clean ;;
        help|--help|-h) cmd_help ;;
        *)              error "$(printf "$T_UNKNOWN_CMD_FMT" "$1")"; cmd_help; exit 1 ;;
    esac
}

main "$@"
