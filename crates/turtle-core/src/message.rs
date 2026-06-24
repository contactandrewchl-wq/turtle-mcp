//! Tipos de dominio de los mensajes entre agentes (la bandeja). Permiten relevos/handoffs
//! dirigidos a un rol o por difusión (RF-COM-03..06). La persistencia vive en `turtle-data`.

/// Datos para enviar un mensaje (sin id ni marca de tiempo: los asigna la capa de datos).
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub project: String,
    /// Rótulo del remitente, si se conoce.
    pub from_agent: Option<String>,
    /// Rótulo del destinatario; `None` significa difusión a todos los del proyecto.
    pub to_agent: Option<String>,
    pub body: String,
}

/// Un mensaje persistido (RF-COM-03). Marcas de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub project: String,
    pub from_agent: Option<String>,
    pub to_agent: Option<String>,
    pub body: String,
    pub created_at: i64,
    /// Momento en que se entregó/leyó; `None` mientras está pendiente en la bandeja.
    pub read_at: Option<i64>,
}
