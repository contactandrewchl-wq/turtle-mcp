//! `turtle-embed` — Cliente de embeddings local (Ollama), opcional y diferido.
//!
//! Genera embeddings localmente sin enviar contenido a servicios externos (RNF-SEG-02).
//! Si Ollama no está disponible, la búsqueda degrada a FTS (RNF-FIA-04). Arquitectura §6.
//!
//! TODO(F2, S1): POST /api/embeddings (nomic-embed-text), encolado en hilo de fondo
//! para no bloquear el guardado más de 500 ms (RNF-PER-04).

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
