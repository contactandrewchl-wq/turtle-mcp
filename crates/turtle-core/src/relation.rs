//! Tipos de dominio de las relaciones entre memorias (sin I/O). Soportan la deduplicación y la
//! detección de conflictos: el agente, tras ver los candidatos de FTS5, registra cómo se
//! relacionan dos memorias (RF-CNF-02/03). La persistencia vive en `turtle-data`.

/// Tipo de relación entre dos memorias (de `from` hacia `to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    /// `from` reemplaza a `to` (la deja obsoleta).
    Replaces,
    /// `from` entra en conflicto con `to` (se contradicen).
    ConflictsWith,
    /// `from` se relaciona con `to` (temas conexos).
    RelatesTo,
    /// `from` es un duplicado de `to`.
    DuplicateOf,
}

impl RelationKind {
    /// Representación textual estable usada para persistir el tipo.
    pub fn as_str(self) -> &'static str {
        match self {
            RelationKind::Replaces => "replaces",
            RelationKind::ConflictsWith => "conflicts",
            RelationKind::RelatesTo => "relates",
            RelationKind::DuplicateOf => "duplicate",
        }
    }

    /// Interpreta el texto persistido o el valor recibido por API; `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "replaces" | "reemplaza" => RelationKind::Replaces,
            "conflicts" | "conflicto" => RelationKind::ConflictsWith,
            "relates" | "relaciona" => RelationKind::RelatesTo,
            "duplicate" | "duplicado" => RelationKind::DuplicateOf,
            _ => return None,
        })
    }

    /// Etiqueta legible en español neutro (RNF-LOC-01).
    pub fn etiqueta(self) -> &'static str {
        match self {
            RelationKind::Replaces => "reemplaza a",
            RelationKind::ConflictsWith => "entra en conflicto con",
            RelationKind::RelatesTo => "se relaciona con",
            RelationKind::DuplicateOf => "es duplicado de",
        }
    }
}

/// Datos para registrar una relación (sin id ni marca de tiempo: los asigna la capa de datos).
#[derive(Debug, Clone)]
pub struct NewRelation {
    pub from_id: String,
    pub to_id: String,
    pub kind: RelationKind,
    /// Nota opcional que explica el porqué (lo aporta el agente o la persona).
    pub note: Option<String>,
}

/// Una relación persistida (RF-CNF-02). Marca de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Relation {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: RelationKind,
    pub note: Option<String>,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::RelationKind;

    #[test]
    fn tipo_ida_y_vuelta_y_alias() {
        for k in [
            RelationKind::Replaces,
            RelationKind::ConflictsWith,
            RelationKind::RelatesTo,
            RelationKind::DuplicateOf,
        ] {
            assert_eq!(RelationKind::parse(k.as_str()), Some(k));
            assert!(!k.etiqueta().is_empty());
        }
        // Alias en español.
        assert_eq!(
            RelationKind::parse("duplicado"),
            Some(RelationKind::DuplicateOf)
        );
        assert_eq!(RelationKind::parse("?"), None);
    }
}
