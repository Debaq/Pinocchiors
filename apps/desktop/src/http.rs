//! Clientes HTTP de la app (chat con proveedores de IA, impresoras): rustls con `ring`.
//!
//! En escritorio reqwest revisa los certificados con el verificador del sistema
//! (`rustls-platform-verifier`). En Android ese verificador necesita iniciarse por
//! JNI y llevar sus clases Kotlin dentro del APK; sin eso, todo HTTPS falla. Por
//! eso en Android se confía en los certificados raíz del propio teléfono, leídos
//! de sus carpetas (las mismas que usa Android para sus apps).

use std::path::Path;

/// Carpetas de certificados raíz de Android: la del módulo que se actualiza solo
/// (Android 14 en adelante) y la de siempre
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const ANDROID_CA_DIRS: &[&str] = &["/apex/com.android.conscrypt/cacerts", "/system/etc/security/cacerts"];

/// `Client::builder()` de reqwest (bloqueante) listo para HTTPS en todas las plataformas
pub fn blocking_builder() -> reqwest::blocking::ClientBuilder {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let builder = reqwest::blocking::Client::builder();
    #[cfg(target_os = "android")]
    let builder = match android_tls() {
        Some(config) => builder.use_preconfigured_tls(config),
        None => builder,
    };
    builder
}

#[cfg(target_os = "android")]
fn android_tls() -> Option<rustls::ClientConfig> {
    static CONFIG: std::sync::OnceLock<Option<rustls::ClientConfig>> = std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| ANDROID_CA_DIRS.iter().find_map(|dir| tls_from_dir(Path::new(dir))))
        .clone()
}

/// Configuración TLS que confía en los certificados (PEM) de una carpeta; `None`
/// si no hay ninguno usable. Los que no se pueden leer se saltan.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
fn tls_from_dir(dir: &Path) -> Option<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(bytes) = std::fs::read(entry.path()) else { continue };
        let ders = pem_certificates(&String::from_utf8_lossy(&bytes));
        roots.add_parsable_certificates(ders.into_iter().map(rustls::pki_types::CertificateDer::from));
    }
    if roots.is_empty() {
        return None;
    }
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .ok()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Some(config)
}

/// Los certificados (DER) de un texto PEM. Los archivos de Android traen, además
/// del bloque PEM, la descripción del certificado en texto: se ignora.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
fn pem_certificates(text: &str) -> Vec<Vec<u8>> {
    use base64::Engine;
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(BEGIN) {
        let body = &rest[start + BEGIN.len()..];
        let Some(end) = body.find(END) else { break };
        let b64: String = body[..end].chars().filter(|c| !c.is_whitespace()).collect();
        if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(b64) {
            out.push(der);
        }
        rest = &body[end + END.len()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_with_android_text_dump() {
        use base64::Engine;
        let one = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4, 5]);
        let two = base64::engine::general_purpose::STANDARD.encode([9u8; 70]);
        // Como /system/etc/security/cacerts/*.0: PEM y luego el certificado en texto
        let text = format!(
            "-----BEGIN CERTIFICATE-----\n{one}\n-----END CERTIFICATE-----\nCertificate:\n    Data:\n        Version: 3\n\
             -----BEGIN CERTIFICATE-----\n{}\n{}\n-----END CERTIFICATE-----\n",
            &two[..40],
            &two[40..]
        );
        assert_eq!(pem_certificates(&text), vec![vec![1, 2, 3, 4, 5], vec![9u8; 70]]);
        assert!(pem_certificates("sin certificados").is_empty());
        assert!(pem_certificates("-----BEGIN CERTIFICATE-----\nAAAA").is_empty());
    }

    #[test]
    fn empty_dir_gives_no_config() {
        let dir = std::env::temp_dir().join(format!("pinocchio-cacerts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("roto.0"), "nada útil").unwrap();
        assert!(tls_from_dir(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// HTTPS de verdad con certificados de una carpeta al estilo Android
    /// (`PINOCCHIO_TEST_CACERTS`, p. ej. copiada del emulador con `adb pull
    /// /apex/com.android.conscrypt/cacerts`): con una clave falsa la API contesta
    /// «no autorizado». Usa la red: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn android_style_roots_reach_a_real_api() {
        let dir = std::env::var("PINOCCHIO_TEST_CACERTS").expect("PINOCCHIO_TEST_CACERTS");
        let config = tls_from_dir(Path::new(&dir)).expect("sin certificados");
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::blocking::Client::builder().use_preconfigured_tls(config).build().unwrap();
        let status = client.get("https://api.deepseek.com/models").bearer_auth("sk-falsa").send().unwrap().status();
        assert_eq!(status.as_u16(), 401);
    }
}
