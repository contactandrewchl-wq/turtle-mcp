//! Tipos de dominio de los agentes (sin I/O). Realización lógica del registro de agentes
//! (RF-AGN-01..05). La persistencia vive en `turtle-data`.

/// Estado de actividad de un agente (RF-N-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Tiene una sesión abierta: está trabajando.
    Working,
    /// Registrado pero sin sesión activa.
    Idle,
}

impl AgentStatus {
    /// Representación textual estable usada para persistir el estado.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentStatus::Working => "working",
            AgentStatus::Idle => "idle",
        }
    }

    /// Interpreta el texto persistido; devuelve `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "working" => AgentStatus::Working,
            "idle" => AgentStatus::Idle,
            _ => return None,
        })
    }
}

/// Datos para registrar (o actualizar) un agente. La identidad es `(project, label)`.
#[derive(Debug, Clone)]
pub struct NewAgent {
    pub project: String,
    /// Rótulo del agente: rol o dominio (RF-AGN-01).
    pub label: String,
    /// Tarea declarada en curso.
    pub task: Option<String>,
    /// Rama de git en la que trabaja (RF-AGN-02).
    pub branch: Option<String>,
}

/// Un agente registrado (RF-AGN-*). Marcas de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub project: String,
    pub label: String,
    pub status: AgentStatus,
    pub task: Option<String>,
    pub branch: Option<String>,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[cfg(test)]
mod tests {
    use super::AgentStatus;

    #[test]
    fn estado_ida_y_vuelta() {
        for s in [AgentStatus::Working, AgentStatus::Idle] {
            assert_eq!(AgentStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(AgentStatus::parse("?"), None);
    }
}
