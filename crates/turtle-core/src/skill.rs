//! Tipos de dominio de las skills (y los agentes ingeridos), sin I/O. Realización lógica de la
//! capa de skills (RF-SKL-01/02). La persistencia, el índice FTS5 y la ingesta de `skills/` y
//! `agents/` viven en `turtle-data` y `turtle-service`.

/// Tipo de una skill (RF-SKL-01). `Agent` representa subagentes ingeridos desde `agents/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillKind {
    /// Modifica el comportamiento del agente de forma persistente.
    Behavior,
    /// Conocimiento de referencia que se carga bajo demanda.
    Knowledge,
    /// Describe una herramienta o script invocable.
    Tool,
    /// Un subagente (formato `agents/<nombre>.md`).
    Agent,
}

impl SkillKind {
    /// Representación textual estable usada para persistir el tipo.
    pub fn as_str(self) -> &'static str {
        match self {
            SkillKind::Behavior => "behavior",
            SkillKind::Knowledge => "knowledge",
            SkillKind::Tool => "tool",
            SkillKind::Agent => "agent",
        }
    }

    /// Interpreta el texto persistido o el `type` del frontmatter (acepta alias en español);
    /// `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "behavior" | "behaviour" | "comportamiento" => SkillKind::Behavior,
            "knowledge" | "conocimiento" => SkillKind::Knowledge,
            "tool" | "herramienta" => SkillKind::Tool,
            "agent" | "agente" | "subagent" => SkillKind::Agent,
            _ => return None,
        })
    }

    /// Etiqueta legible en español neutro (RNF-LOC-01).
    pub fn etiqueta(self) -> &'static str {
        match self {
            SkillKind::Behavior => "comportamiento",
            SkillKind::Knowledge => "conocimiento",
            SkillKind::Tool => "herramienta",
            SkillKind::Agent => "agente",
        }
    }
}

/// Intensidad de activación de una skill de comportamiento (RF-SKL-07). `Off` = inactiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Intensidad {
    #[default]
    Off,
    Lite,
    Full,
    Ultra,
}

impl Intensidad {
    pub fn as_str(self) -> &'static str {
        match self {
            Intensidad::Off => "off",
            Intensidad::Lite => "lite",
            Intensidad::Full => "full",
            Intensidad::Ultra => "ultra",
        }
    }

    /// Interpreta el nivel (acepta `apagado`); `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "off" | "apagado" => Intensidad::Off,
            "lite" => Intensidad::Lite,
            "full" => Intensidad::Full,
            "ultra" => Intensidad::Ultra,
            _ => return None,
        })
    }

    /// `true` si la skill está activa (cualquier nivel distinto de `off`).
    pub fn activa(self) -> bool {
        self != Intensidad::Off
    }
}

/// Datos para crear o actualizar una skill (RF-SKL-02/05). `project` vacío = global.
#[derive(Debug, Clone)]
pub struct NewSkill {
    pub project: String,
    pub name: String,
    pub kind: SkillKind,
    /// Cuándo usarla (de la `description` del frontmatter, normalmente).
    pub when_to_use: Option<String>,
    pub content: String,
    /// Etiquetas separadas por coma.
    pub tags: Option<String>,
    /// Origen: ruta del archivo o URL de la que se importó.
    pub source: Option<String>,
}

/// Una skill completa tal como se persiste. Marcas de tiempo en epoch ms UTC.
#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub project: String,
    pub name: String,
    pub kind: SkillKind,
    pub when_to_use: Option<String>,
    pub content: String,
    pub tags: Option<String>,
    pub source: Option<String>,
    /// Intensidad de activación, para skills de comportamiento (RF-SKL-07).
    pub intensity: Intensidad,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Fila del índice barato de skills: metadatos sin el contenido completo (RF-SKL-03).
#[derive(Debug, Clone)]
pub struct SkillIndexRow {
    pub id: String,
    pub name: String,
    pub kind: SkillKind,
    pub when_to_use: Option<String>,
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::SkillKind;

    #[test]
    fn tipo_ida_y_vuelta_y_alias() {
        for k in [
            SkillKind::Behavior,
            SkillKind::Knowledge,
            SkillKind::Tool,
            SkillKind::Agent,
        ] {
            assert_eq!(SkillKind::parse(k.as_str()), Some(k));
            assert!(!k.etiqueta().is_empty());
        }
        assert_eq!(
            SkillKind::parse("Comportamiento"),
            Some(SkillKind::Behavior)
        );
        assert_eq!(SkillKind::parse("subagent"), Some(SkillKind::Agent));
        assert_eq!(SkillKind::parse("?"), None);
    }
}
