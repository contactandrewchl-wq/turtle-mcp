//! Tipos de dominio de las sesiones de trabajo (sin I/O). Realización lógica del esquema
//! de la arquitectura §3 (`sessions`). La persistencia vive en `turtle-data`.

/// Estado de una sesión de trabajo (RF-SES-01, RF-SES-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// La sesión está en curso.
    Open,
    /// La sesión se cerró y tiene un resumen de lo realizado.
    Closed,
}

impl SessionStatus {
    /// Representación textual estable usada para persistir el estado.
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Open => "open",
            SessionStatus::Closed => "closed",
        }
    }

    /// Interpreta el texto persistido; devuelve `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => SessionStatus::Open,
            "closed" => SessionStatus::Closed,
            _ => return None,
        })
    }
}

/// Datos para iniciar una sesión (sin id ni marcas de tiempo: los asigna la capa de datos).
#[derive(Debug, Clone)]
pub struct NewSession {
    pub project: String,
    /// Tarea declarada que guía el contexto inicial (RF-SES-01).
    pub task: Option<String>,
    /// Rótulo del agente (rol o dominio). En el MVP es opcional y libre (RF-AGN-01 es de F3).
    pub agent_id: Option<String>,
}

/// Una sesión persistida (RF-BD-04). Marcas de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub project: String,
    pub agent_id: Option<String>,
    pub task: Option<String>,
    pub summary: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub status: SessionStatus,
}

#[cfg(test)]
mod tests {
    use super::SessionStatus;

    #[test]
    fn estado_ida_y_vuelta() {
        for s in [SessionStatus::Open, SessionStatus::Closed] {
            assert_eq!(SessionStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(SessionStatus::parse("desconocido"), None);
    }
}
