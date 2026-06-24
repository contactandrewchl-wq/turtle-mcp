//! Tipo de dominio del checkpoint de trabajo en curso (RF-SES-04), sin I/O. Persiste lo que el
//! agente está haciendo para sobrevivir a una compactación de contexto y recuperarlo al reanudar.
//! La persistencia vive en `turtle-data`.

/// Un checkpoint de trabajo en curso. Marca de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub id: String,
    pub project: String,
    pub content: String,
    pub created_at: i64,
}
