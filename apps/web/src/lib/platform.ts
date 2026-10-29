/**
 * La app corriendo en Android (APK): sin auto-actualización, programas
 * externos (GIMP), conector MCP ni chat, controles de ventana, cámara en vivo
 * ni escáneres Orizon3D. El WebView de Android lo dice en el user agent.
 */
export const IS_ANDROID = /Android/i.test(navigator.userAgent);
