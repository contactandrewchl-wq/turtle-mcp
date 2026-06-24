//! `turtle-embed` — Cliente de embeddings local (Ollama), **opcional y opt-in**.
//!
//! Genera embeddings localmente sin enviar contenido a servicios externos (RNF-SEG-02): habla con
//! Ollama por HTTP en `localhost`. Si Ollama no está disponible, la búsqueda degrada a FTS
//! (RNF-FIA-04) — nunca rompe. La semántica se prende con `turtle semantic on`.
//!
//! Sin TLS a propósito (Ollama es local): el binario no arrastra `ring`/openssl y el cross-compile
//! a musl del release sigue simple.

use std::time::Duration;

/// Modelo de embeddings por defecto (chico y bueno; ~270 MB en Ollama).
pub const MODELO_POR_DEFECTO: &str = "nomic-embed-text";

/// Host de Ollama; configurable con `$OLLAMA_HOST`. Por defecto el local.
pub fn ollama_host() -> String {
    std::env::var("OLLAMA_HOST")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "http://localhost:11434".to_string())
}

/// Falla al generar o pedir un embedding. Todas son no-fatales: la búsqueda degrada a FTS.
#[derive(Debug)]
pub enum EmbedError {
    /// Ollama no respondió (no está corriendo, timeout, red).
    Unreachable(String),
    /// Respondió, pero con error/cuerpo inesperado.
    Respuesta(String),
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbedError::Unreachable(e) => write!(f, "Ollama no disponible: {e}"),
            EmbedError::Respuesta(e) => write!(f, "respuesta de Ollama inesperada: {e}"),
        }
    }
}
impl std::error::Error for EmbedError {}

/// ¿Ollama responde? (`GET /api/tags`, timeout corto). No falla la búsqueda si está abajo.
pub fn disponible(host: &str) -> bool {
    let url = format!("{host}/api/tags");
    ureq::get(&url)
        .timeout(Duration::from_millis(800))
        .call()
        .is_ok()
}

/// `true` si el modelo ya está descargado en Ollama (aparece en `GET /api/tags`).
pub fn modelo_presente(host: &str, modelo: &str) -> bool {
    let url = format!("{host}/api/tags");
    let Ok(resp) = ureq::get(&url).timeout(Duration::from_secs(5)).call() else {
        return false;
    };
    let Ok(txt) = resp.into_string() else {
        return false;
    };
    // Coincidencia laxa: el nombre puede venir como "nomic-embed-text" o "nomic-embed-text:latest".
    txt.contains(modelo)
}

/// Genera el embedding de un texto (`POST /api/embeddings`). Bloqueante (~decenas de ms en local).
pub fn embed(host: &str, modelo: &str, texto: &str) -> Result<Vec<f32>, EmbedError> {
    let url = format!("{host}/api/embeddings");
    let cuerpo = serde_json::json!({ "model": modelo, "prompt": texto }).to_string();
    let resp = ureq::post(&url)
        .timeout(Duration::from_secs(30))
        .set("content-type", "application/json")
        .send_string(&cuerpo)
        .map_err(|e| EmbedError::Unreachable(e.to_string()))?;
    let txt = resp
        .into_string()
        .map_err(|e| EmbedError::Respuesta(e.to_string()))?;
    let v: serde_json::Value =
        serde_json::from_str(&txt).map_err(|e| EmbedError::Respuesta(e.to_string()))?;
    let arr = v
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| EmbedError::Respuesta("falta el campo 'embedding'".into()))?;
    let vec: Vec<f32> = arr
        .iter()
        .filter_map(|x| x.as_f64().map(|f| f as f32))
        .collect();
    if vec.is_empty() {
        return Err(EmbedError::Respuesta("embedding vacío".into()));
    }
    Ok(vec)
}

/// Descarga un modelo en Ollama (`POST /api/pull`). Bloqueante y potencialmente lento (descarga).
pub fn pull(host: &str, modelo: &str) -> Result<(), EmbedError> {
    let url = format!("{host}/api/pull");
    let cuerpo = serde_json::json!({ "model": modelo, "stream": false }).to_string();
    ureq::post(&url)
        .timeout(Duration::from_secs(900))
        .set("content-type", "application/json")
        .send_string(&cuerpo)
        .map_err(|e| EmbedError::Unreachable(e.to_string()))?;
    Ok(())
}

/// Similitud coseno entre dos vectores. Para el KNN en Rust (sin extensión nativa de SQLite).
pub fn coseno(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Serializa un vector a bytes little-endian (para guardarlo como BLOB en SQLite).
pub fn a_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Reconstruye un vector desde el BLOB little-endian. Bytes sobrantes (no múltiplo de 4) se ignoran.
pub fn desde_bytes(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bytes() {
        let v = vec![0.0f32, 1.5, -2.25, 3.125];
        assert_eq!(desde_bytes(&a_bytes(&v)), v);
    }

    #[test]
    fn coseno_identico_es_uno() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((coseno(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn coseno_ortogonal_es_cero() {
        assert!(coseno(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn host_por_defecto_local() {
        // Sin $OLLAMA_HOST seteado en el entorno de test.
        if std::env::var("OLLAMA_HOST").is_err() {
            assert_eq!(ollama_host(), "http://localhost:11434");
        }
    }
}
