//! Tipos de dominio de las memorias (sin I/O). Realización lógica del esquema de
//! la arquitectura §3.1. La persistencia vive en `turtle-data`.

/// Tipo de una memoria (RF-MEM-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Decision,
    Architecture,
    Correction,
    Convention,
    Note,
}

impl MemoryKind {
    /// Representación textual estable usada para persistir el tipo.
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Decision => "decision",
            MemoryKind::Architecture => "architecture",
            MemoryKind::Correction => "correction",
            MemoryKind::Convention => "convention",
            MemoryKind::Note => "note",
        }
    }

    /// Interpreta el texto persistido; devuelve `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "decision" => MemoryKind::Decision,
            "architecture" => MemoryKind::Architecture,
            "correction" => MemoryKind::Correction,
            "convention" => MemoryKind::Convention,
            "note" => MemoryKind::Note,
            _ => return None,
        })
    }
}

/// Importancia de una memoria (RF-MEM-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importance {
    Pinned,
    Normal,
    Ephemeral,
}

impl Importance {
    pub fn as_str(self) -> &'static str {
        match self {
            Importance::Pinned => "pinned",
            Importance::Normal => "normal",
            Importance::Ephemeral => "ephemeral",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pinned" => Importance::Pinned,
            "normal" => Importance::Normal,
            "ephemeral" => Importance::Ephemeral,
            _ => return None,
        })
    }
}

/// Alcance de una memoria: del proyecto o transversal al usuario (paridad funcional
/// `project|personal`). Una memoria `Personal` se incluye en la búsqueda y el contexto de
/// **todos** los proyectos, además del filtro del proyecto activo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// Memoria atada a un proyecto (comportamiento por defecto, como hoy).
    #[default]
    Project,
    /// Memoria del usuario, visible en cualquier proyecto.
    Personal,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Personal => "personal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "project" | "proyecto" => Scope::Project,
            "personal" => Scope::Personal,
            _ => return None,
        })
    }
}

/// Estado del ciclo de vida de una memoria (paridad con el estado de revisión). `NeedsReview` la
/// marca como contexto añejo: el agente debería verificarla antes de confiar en ella. El
/// escalonamiento a frío la setea automáticamente; `mark_reviewed` la vuelve a `Active`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewState {
    /// Vigente: se puede usar sin reservas.
    #[default]
    Active,
    /// Añeja: verificar antes de confiar (la marca el escalonamiento a frío).
    NeedsReview,
}

impl ReviewState {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewState::Active => "active",
            ReviewState::NeedsReview => "needs_review",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "active" | "activo" => ReviewState::Active,
            "needs_review" | "needs-review" | "revisar" => ReviewState::NeedsReview,
            _ => return None,
        })
    }
}

/// Nivel de escalonamiento caliente/tibio/frío (RF-TOK-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Hot,
    Warm,
    Cold,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Hot => "hot",
            Tier::Warm => "warm",
            Tier::Cold => "cold",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "hot" => Tier::Hot,
            "warm" => Tier::Warm,
            "cold" => Tier::Cold,
            _ => return None,
        })
    }
}

/// Perfil de verbosidad de la búsqueda (RF-REC-04): cuánto detalle trae cada resultado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosidad {
    /// Solo metadatos baratos (id, título, tipo, resumen). El perfil por defecto.
    #[default]
    Indice,
    /// Índice más un extracto del contenido.
    Compacto,
    /// Índice más el contenido completo.
    Completo,
}

impl Verbosidad {
    pub fn as_str(self) -> &'static str {
        match self {
            Verbosidad::Indice => "indice",
            Verbosidad::Compacto => "compacto",
            Verbosidad::Completo => "completo",
        }
    }

    /// Interpreta el perfil (acepta variantes con acento); `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "indice" | "índice" | "index" => Verbosidad::Indice,
            "compacto" | "compact" => Verbosidad::Compacto,
            "completo" | "full" => Verbosidad::Completo,
            _ => return None,
        })
    }
}

/// Datos para crear una memoria nueva (sin id ni marcas de tiempo: los asigna la capa de datos).
#[derive(Debug, Clone)]
pub struct NewMemory {
    pub project: String,
    pub kind: MemoryKind,
    pub title: String,
    pub what: Option<String>,
    pub why: Option<String>,
    pub where_: Option<String>,
    pub learned: Option<String>,
    pub content: String,
    pub summary: Option<String>,
    /// Alcance: del proyecto (por defecto) o personal (transversal al usuario).
    pub scope: Scope,
    /// Clave estable de tema evolutivo (paridad funcional). Si viene y ya existe una memoria con la
    /// misma `(project, scope, topic_key)`, el guardado hace UPSERT en vez de duplicar.
    pub topic_key: Option<String>,
    /// Prompt del usuario que originó la memoria, si se conoce. Best-effort, nunca inventado.
    pub prompt: Option<String>,
}

impl NewMemory {
    /// Constructor mínimo con los campos nuevos en su valor por defecto (scope project, sin
    /// topic_key ni prompt). Mantiene compacto el código que no usa estas features.
    pub fn nueva(project: String, kind: MemoryKind, title: String, content: String) -> Self {
        NewMemory {
            project,
            kind,
            title,
            what: None,
            why: None,
            where_: None,
            learned: None,
            content,
            summary: None,
            scope: Scope::Project,
            topic_key: None,
            prompt: None,
        }
    }
}

/// Una memoria completa tal como se persiste (RF-BD-01). Marcas de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Memory {
    pub id: String,
    pub project: String,
    pub kind: MemoryKind,
    pub title: String,
    pub what: Option<String>,
    pub why: Option<String>,
    pub where_: Option<String>,
    pub learned: Option<String>,
    pub content: String,
    pub summary: Option<String>,
    pub importance: Importance,
    pub tier: Tier,
    /// Alcance: del proyecto o personal (transversal al usuario).
    pub scope: Scope,
    /// Clave estable de tema evolutivo, si la memoria pertenece a un tema.
    pub topic_key: Option<String>,
    /// Estado del ciclo de vida: vigente o por revisar (contexto añejo).
    pub review_state: ReviewState,
    /// Prompt del usuario que originó la memoria, si se capturó.
    pub prompt: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub accessed_at: i64,
}

/// Fila del índice barato de búsqueda: primera etapa de la recuperación en dos etapas
/// (RF-REC-01). **No** incluye el contenido completo.
#[derive(Debug, Clone)]
pub struct MemoryIndexRow {
    pub id: String,
    pub title: String,
    pub kind: MemoryKind,
    pub summary: Option<String>,
    pub score: f64,
    /// `true` si la memoria está marcada para revisión (contexto añejo): el índice lo señala para
    /// que el agente la trate como "verificar antes de confiar" (paridad con el estado de revisión).
    /// Un solo bit, no infla el presupuesto del índice barato.
    pub needs_review: bool,
    /// Cuerpo opcional según la verbosidad (RF-REC-04): extracto (compacto) o contenido
    /// completo (completo). `None` en el perfil índice.
    pub cuerpo: Option<String>,
}

/// Una versión histórica de una memoria de tema evolutivo (versionado temporal de temas). Cuando un
/// `memory_save` con un `topic_key` ya existente actualiza la memoria viva, la versión anterior se
/// archiva acá con su intervalo de validez `[valid_from, valid_to)`. Permite responder "qué
/// sabíamos del tema en tal fecha" sin inflar la búsqueda (no se indexa en FTS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryVersion {
    /// Id de la versión (ULID propio).
    pub id: String,
    /// Id de la memoria viva a la que pertenece esta versión.
    pub memory_id: String,
    pub project: String,
    pub kind: MemoryKind,
    pub title: String,
    pub what: Option<String>,
    pub why: Option<String>,
    pub where_: Option<String>,
    pub learned: Option<String>,
    pub content: String,
    pub summary: Option<String>,
    /// Desde cuándo era válida esta versión (el `updated_at` que tenía cuando estaba viva).
    pub valid_from: i64,
    /// Hasta cuándo fue válida (cuándo la reemplazó una versión nueva).
    pub valid_to: i64,
}

#[cfg(test)]
mod tests {
    use super::{Importance, MemoryKind, ReviewState, Scope, Tier, Verbosidad};

    #[test]
    fn tipos_ida_y_vuelta() {
        for k in [
            MemoryKind::Decision,
            MemoryKind::Architecture,
            MemoryKind::Correction,
            MemoryKind::Convention,
            MemoryKind::Note,
        ] {
            assert_eq!(MemoryKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(MemoryKind::parse("desconocido"), None);
        assert_eq!(
            Importance::parse(Importance::Pinned.as_str()),
            Some(Importance::Pinned)
        );
        assert_eq!(Tier::parse(Tier::Cold.as_str()), Some(Tier::Cold));
        for v in [
            Verbosidad::Indice,
            Verbosidad::Compacto,
            Verbosidad::Completo,
        ] {
            assert_eq!(Verbosidad::parse(v.as_str()), Some(v));
        }
        assert_eq!(Verbosidad::parse("índice"), Some(Verbosidad::Indice));
        assert_eq!(Verbosidad::default(), Verbosidad::Indice);
        assert_eq!(Verbosidad::parse("?"), None);

        // Scope y ReviewState: ida y vuelta, defaults y alias.
        for s in [Scope::Project, Scope::Personal] {
            assert_eq!(Scope::parse(s.as_str()), Some(s));
        }
        assert_eq!(Scope::default(), Scope::Project);
        assert_eq!(Scope::parse("proyecto"), Some(Scope::Project));
        assert_eq!(Scope::parse("?"), None);
        for r in [ReviewState::Active, ReviewState::NeedsReview] {
            assert_eq!(ReviewState::parse(r.as_str()), Some(r));
        }
        assert_eq!(ReviewState::default(), ReviewState::Active);
        assert_eq!(
            ReviewState::parse("needs-review"),
            Some(ReviewState::NeedsReview)
        );
        assert_eq!(ReviewState::parse("?"), None);
    }
}
