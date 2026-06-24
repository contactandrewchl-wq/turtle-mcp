//! Tipos de dominio del bus de actividad (sin I/O). Cada operación relevante se registra como
//! un evento atribuido a un agente (RF-COM-01, RF-BD-05). La persistencia vive en `turtle-data`.

/// Tipo de evento del bus de actividad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    MemorySaved,
    MemoryUpdated,
    MemoryDeleted,
    SessionStarted,
    SessionClosed,
    AgentRegistered,
    MessageSent,
    /// Claude usó una herramienta (capturado por hook PreToolUse).
    ToolUsed,
    /// Claude delegó en un subagente (capturado por hook PreToolUse del tool `Task`).
    AgentDispatched,
}

impl EventKind {
    /// Representación textual estable usada para persistir el tipo.
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::MemorySaved => "memory_saved",
            EventKind::MemoryUpdated => "memory_updated",
            EventKind::MemoryDeleted => "memory_deleted",
            EventKind::SessionStarted => "session_started",
            EventKind::SessionClosed => "session_closed",
            EventKind::AgentRegistered => "agent_registered",
            EventKind::MessageSent => "message_sent",
            EventKind::ToolUsed => "tool_used",
            EventKind::AgentDispatched => "agent_dispatched",
        }
    }

    /// Interpreta el texto persistido; devuelve `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "memory_saved" => EventKind::MemorySaved,
            "memory_updated" => EventKind::MemoryUpdated,
            "memory_deleted" => EventKind::MemoryDeleted,
            "session_started" => EventKind::SessionStarted,
            "session_closed" => EventKind::SessionClosed,
            "agent_registered" => EventKind::AgentRegistered,
            "message_sent" => EventKind::MessageSent,
            "tool_used" => EventKind::ToolUsed,
            "agent_dispatched" => EventKind::AgentDispatched,
            _ => return None,
        })
    }

    /// Etiqueta legible en español neutro para la CLI (RNF-LOC-01).
    pub fn etiqueta(self) -> &'static str {
        match self {
            EventKind::MemorySaved => "guardó una memoria",
            EventKind::MemoryUpdated => "actualizó una memoria",
            EventKind::MemoryDeleted => "eliminó una memoria",
            EventKind::SessionStarted => "inició una sesión",
            EventKind::SessionClosed => "cerró una sesión",
            EventKind::AgentRegistered => "se registró",
            EventKind::MessageSent => "envió un mensaje",
            EventKind::ToolUsed => "usó una herramienta",
            EventKind::AgentDispatched => "delegó en un agente",
        }
    }
}

/// Datos para registrar un evento (sin id ni marca de tiempo: los asigna la capa de datos).
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub project: String,
    /// Rótulo del agente atribuido, si lo hay.
    pub agent: Option<String>,
    pub kind: EventKind,
    /// Id del objeto afectado (memoria, sesión…), si aplica.
    pub target_id: Option<String>,
    /// Texto corto legible (p. ej. el título de la memoria o la tarea).
    pub summary: Option<String>,
}

/// Un evento del bus de actividad (RF-BD-05). Marca de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Event {
    pub id: String,
    pub project: String,
    pub agent: Option<String>,
    pub kind: EventKind,
    pub target_id: Option<String>,
    pub summary: Option<String>,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::EventKind;

    #[test]
    fn tipo_ida_y_vuelta() {
        for k in [
            EventKind::MemorySaved,
            EventKind::MemoryUpdated,
            EventKind::MemoryDeleted,
            EventKind::SessionStarted,
            EventKind::SessionClosed,
            EventKind::AgentRegistered,
            EventKind::MessageSent,
            EventKind::ToolUsed,
            EventKind::AgentDispatched,
        ] {
            assert_eq!(EventKind::parse(k.as_str()), Some(k));
            assert!(!k.etiqueta().is_empty());
        }
        assert_eq!(EventKind::parse("?"), None);
    }
}
