//! Skills semilla embebidas en el binario (RF-SKL-09, Apéndice A): un conjunto curado y compacto
//! que `turtle skills seed` carga, para que el sistema traiga skills útiles sin necesitar un
//! directorio `skills/` presente. Son globales (proyecto vacío) y la carga es idempotente.

use turtle_core::skill::{NewSkill, SkillKind};

/// Las skills semilla preconfiguradas.
pub fn semillas() -> Vec<NewSkill> {
    vec![
        semilla(
            "turtle-protocol",
            SkillKind::Behavior,
            "Cómo usar la memoria de Turtle: cuándo guardar y buscar, y cómo recuperar contexto.",
            "turtle,memoria,protocolo",
            include_str!("../seeds/turtle-protocol.md"),
        ),
        semilla(
            "ponytail",
            SkillKind::Behavior,
            "Anti sobre-ingeniería: escalera de decisión antes de escribir código.",
            "calidad,diseño",
            include_str!("../seeds/ponytail.md"),
        ),
        semilla(
            "gh-cli",
            SkillKind::Tool,
            "Operar GitHub (PRs, issues, API) desde la terminal con gh.",
            "git,github,cli",
            include_str!("../seeds/gh-cli.md"),
        ),
    ]
}

fn semilla(name: &str, kind: SkillKind, cuando: &str, tags: &str, content: &str) -> NewSkill {
    NewSkill {
        project: String::new(),
        name: name.to_string(),
        kind,
        when_to_use: Some(cuando.to_string()),
        content: content.to_string(),
        tags: Some(tags.to_string()),
        source: Some("semilla".to_string()),
    }
}

// ─── Bundle completo: `skills/` + `agents/` del repo, embebidos en el binario ───
//
// Permite que `turtle skills seed` (y el configurador) siembren las 21 skills y 9 personas en
// cualquier máquina, sin necesitar el repositorio presente (paridad con gentle-ai).

use std::collections::BTreeMap;

use include_dir::{include_dir, Dir, File};

use crate::scan;

static SKILLS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../skills");
static AGENTS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../agents");

/// Todas las skills (`SKILL.md`) y personas (`AGENT.md`) embebidas, listas para persistir.
/// Son globales (proyecto vacío). Se ignoran índices como `catalog.md`/`README.md`/`roster.md`.
pub fn bundled() -> Vec<NewSkill> {
    let mut out = Vec::new();
    recolectar(&SKILLS_DIR, "SKILL.md", false, &mut out);
    recolectar(&AGENTS_DIR, "AGENT.md", true, &mut out);
    out
}

fn recolectar(dir: &Dir, archivo: &str, es_agente: bool, out: &mut Vec<NewSkill>) {
    for f in dir.files() {
        if f.path().file_name().and_then(|n| n.to_str()) == Some(archivo) {
            if let Some(sk) = parse_embebido(f, es_agente) {
                out.push(sk);
            }
        }
    }
    for sub in dir.dirs() {
        recolectar(sub, archivo, es_agente, out);
    }
}

fn parse_embebido(f: &File, es_agente: bool) -> Option<NewSkill> {
    let raw = f.contents_utf8()?;
    let path = f.path();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("skill");
    let dir_padre = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());
    let source = format!("bundle:{}", path.display());
    scan::parsear_contenido(stem, dir_padre, raw, es_agente, "", &source)
}

// ─── Personas como subagentes nativos de Claude Code ───
//
// Convierte cada persona embebida (`agents/<slug>/AGENT.md`) en un archivo de subagente para
// `~/.claude/agents/<slug>.md`. El `model` de la persona se pasa al campo `model` del subagente,
// que Claude Code respeta: así la selección de modelo por agente se vuelve real (no solo sugerencia).

/// Marcador que identifica un subagente generado por Turtle (para reemplazarlo sin pisar ajenos).
pub const MARCA_SUBAGENTE: &str =
    "<!-- TURTLE-AGENT: generado por `turtle install`; se reemplaza al re-ejecutar, no editar -->";

/// Un subagente de Claude Code (nombre de archivo por `slug`) generado desde una persona.
pub struct SubagenteClaude {
    pub slug: String,
    pub contenido: String,
}

/// Genera los subagentes de Claude Code a partir de las personas embebidas. `overrides` mapea
/// `slug → modelo` elegido por el usuario (vía `turtle modelos`); cuando hay uno, pisa el `model`
/// del frontmatter. Pasá un mapa vacío para usar los modelos por defecto del bundle.
pub fn subagentes_claude(overrides: &BTreeMap<String, String>) -> Vec<SubagenteClaude> {
    let mut out = Vec::new();
    recolectar_agentes(&AGENTS_DIR, overrides, &mut out);
    out
}

fn recolectar_agentes(
    dir: &Dir,
    overrides: &BTreeMap<String, String>,
    out: &mut Vec<SubagenteClaude>,
) {
    for f in dir.files() {
        if f.path().file_name().and_then(|n| n.to_str()) == Some("AGENT.md") {
            if let Some(sa) = construir_subagente(f, overrides) {
                out.push(sa);
            }
        }
    }
    for sub in dir.dirs() {
        recolectar_agentes(sub, overrides, out);
    }
}

fn construir_subagente(f: &File, overrides: &BTreeMap<String, String>) -> Option<SubagenteClaude> {
    let raw = f.contents_utf8()?;
    let path = f.path();
    let slug = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())?
        .to_string();
    // Reusa el parser para nombre/descripción; el `model` y el cuerpo se extraen aparte.
    let ns = scan::parsear_contenido("AGENT", Some(&slug), raw, true, "", "bundle")?;
    let descripcion = ns
        .when_to_use
        .unwrap_or_default()
        .replace('"', "'")
        .replace('\n', " ");
    // El override del usuario gana; si no, el `model` del frontmatter; si no, `inherit`.
    let modelo = overrides
        .get(&slug)
        .cloned()
        .or_else(|| scan::campo_anidado(raw, "model"))
        .unwrap_or_else(|| "inherit".to_string());
    let cuerpo = cuerpo_md(raw);
    let contenido = format!(
        "---\nname: {slug}\ndescription: \"{descripcion}\"\nmodel: {modelo}\n---\n{MARCA_SUBAGENTE}\n\n{cuerpo}\n\nSos parte del equipo de Turtle: usá el MCP `turtle` para memoria (memory_search/memory_save) y skills (skills_search/skill_get), y coordiná por rótulo con message_send/inbox.\n"
    );
    Some(SubagenteClaude { slug, contenido })
}

/// Cuerpo markdown de un archivo (lo que sigue al frontmatter `--- … ---`).
fn cuerpo_md(raw: &str) -> String {
    let Some(cuerpo) = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
    else {
        return raw.to_string();
    };
    let Some(i) = cuerpo.find("\n---") else {
        return raw.to_string();
    };
    let resto = &cuerpo[i + 1..]; // arranca en la línea de cierre `---`
    match resto.find('\n') {
        Some(j) => resto[j + 1..].trim_start().to_string(),
        None => String::new(),
    }
}

/// Modelo declarado por una persona embebida (frontmatter `metadata.model`), buscado por slug.
/// Lo usa el hook de actividad para mostrar "→ donatello (sonnet)".
pub fn modelo_persona(slug: &str) -> Option<String> {
    let mut encontrado = None;
    buscar_modelo(&AGENTS_DIR, slug, &mut encontrado);
    encontrado
}

fn buscar_modelo(dir: &Dir, slug: &str, out: &mut Option<String>) {
    if out.is_some() {
        return;
    }
    for f in dir.files() {
        if f.path().file_name().and_then(|n| n.to_str()) == Some("AGENT.md") {
            let es_slug = f
                .path()
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                == Some(slug);
            if es_slug {
                if let Some(raw) = f.contents_utf8() {
                    *out = scan::campo_anidado(raw, "model");
                }
                return;
            }
        }
    }
    for sub in dir.dirs() {
        buscar_modelo(sub, slug, out);
    }
}

// ─── Catálogo de modelos frontera (Claude Code, por subscripción) ───
//
// Lo que el usuario puede asignar a una persona con `turtle modelos`. El `token` es lo que va al
// campo `model:` del subagente: los alias (opus/sonnet/haiku) los resuelve Claude Code al modelo
// vigente de ese nivel, y los ids fijan una versión. Mantener al día con los lanzamientos de
// Anthropic. `inherit` hereda el modelo de la sesión principal. (Codex y otros CLIs tendrán su
// propio catálogo cuando se sume su adaptador.)

/// Un modelo elegible para una persona en Claude Code.
pub struct ModeloInfo {
    /// Lo que se escribe en `model:` del subagente (alias o id exacto).
    pub token: &'static str,
    /// Para qué sirve / qué resuelve.
    pub nota: &'static str,
}

/// Catálogo de modelos de Anthropic usables por subscripción en Claude Code.
pub const MODELOS_CLAUDE: &[ModeloInfo] = &[
    ModeloInfo {
        token: "inherit",
        nota: "Hereda el modelo de la sesión principal",
    },
    ModeloInfo {
        token: "opus",
        nota: "Alias → Opus vigente (hoy 4.8): codear, arquitectura, razonar",
    },
    ModeloInfo {
        token: "sonnet",
        nota: "Alias → Sonnet vigente (4.6): equilibrio velocidad/inteligencia",
    },
    ModeloInfo {
        token: "haiku",
        nota: "Alias → Haiku vigente (4.5): el más rápido y barato",
    },
    ModeloInfo {
        token: "claude-fable-5",
        nota: "Fable 5 — el más capaz, trabajo agéntico de largo aliento",
    },
    ModeloInfo {
        token: "claude-opus-4-8",
        nota: "Opus 4.8 (id fijo)",
    },
    ModeloInfo {
        token: "claude-opus-4-7",
        nota: "Opus 4.7 (id fijo)",
    },
    ModeloInfo {
        token: "claude-sonnet-4-6",
        nota: "Sonnet 4.6 (id fijo)",
    },
    ModeloInfo {
        token: "claude-haiku-4-5",
        nota: "Haiku 4.5 (id fijo)",
    },
];

/// `true` si `token` es un modelo que una persona puede usar (alias o id del catálogo).
pub fn modelo_valido(token: &str) -> bool {
    MODELOS_CLAUDE.iter().any(|m| m.token == token)
}

/// Una persona embebida con su modelo por defecto, para el configurador `turtle modelos`.
pub struct Persona {
    pub slug: String,
    pub rol: String,
    pub modelo_default: String,
}

/// Lista las personas del bundle (slug, rol y modelo por defecto del frontmatter), ordenadas.
pub fn personas() -> Vec<Persona> {
    let mut out = Vec::new();
    recolectar_personas(&AGENTS_DIR, &mut out);
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    out
}

fn recolectar_personas(dir: &Dir, out: &mut Vec<Persona>) {
    for f in dir.files() {
        if f.path().file_name().and_then(|n| n.to_str()) == Some("AGENT.md") {
            if let (Some(slug), Some(raw)) = (
                f.path()
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str()),
                f.contents_utf8(),
            ) {
                let rol = scan::campo_anidado(raw, "role")
                    .or_else(|| scan::campo_anidado(raw, "domain"))
                    .unwrap_or_default();
                let modelo_default =
                    scan::campo_anidado(raw, "model").unwrap_or_else(|| "inherit".to_string());
                out.push(Persona {
                    slug: slug.to_string(),
                    rol,
                    modelo_default,
                });
            }
        }
    }
    for sub in dir.dirs() {
        recolectar_personas(sub, out);
    }
}
