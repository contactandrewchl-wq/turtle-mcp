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
// Permite que `turtle skills seed` (y el configurador) siembren las skills y personas embebidas en
// cualquier máquina, sin necesitar el repositorio presente.

use std::collections::{BTreeMap, BTreeSet};

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
/// del frontmatter. Pasa un mapa vacío para usar los modelos por defecto del bundle.
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

// ─── Perfiles de modelo por fase ───
//
// Algunos flujos por fases exponen dos ejes (perfil de modelo + tier de razonamiento por fase) que
// Turtle no puede tener: Claude Code da UN solo knob por sub-agente (`model:`). Colapsamos los dos
// ejes en uno: cada fase del flujo SDD tiene un *tier* (strong/mid/cheap) que un *perfil* nombrado
// traduce a un alias de modelo (opus/sonnet/haiku). Como una persona puede ser dueña de fases de
// distinto tier, su modelo efectivo se resuelve con la regla "el más fuerte gana". Provider-agnóstico:
// esto solo decide qué alias se escribe en `model:`; el modelo real lo fija el CLI/subscripción.

/// Tier de razonamiento de una fase del SDD (tres niveles: strong/mid/cheap).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TierFase {
    /// Razonamiento máximo (init, propose, design, contracts, verify, judge).
    Strong,
    /// Equilibrio (spec, tasks, apply).
    Mid,
    /// Rápido/barato (explore, archive).
    Cheap,
}

/// Una fase del flujo SDD de marca: su nombre, su tier y las personas (slugs) dueñas. `duenos`
/// vacío = fase *advisory* (sin persona con `model:` que reescribir: `explore`, `archive`).
pub struct Fase {
    /// Nombre de la fase (clave estable para overrides por fase).
    pub nombre: &'static str,
    /// Tier de razonamiento de la fase.
    pub tier: TierFase,
    /// Slugs de las personas dueñas (deben existir en `personas()`; ver test de cobertura).
    pub duenos: &'static [&'static str],
}

/// Las 11 fases del flujo SDD de Turtle con su tier y dueños reales del roster
/// (`agents/roster.md`). El orden es estable: la resolución y la serialización son deterministas.
pub const FASES: &[Fase] = &[
    Fase {
        nombre: "init",
        tier: TierFase::Strong,
        duenos: &["leonardo"],
    },
    Fase {
        nombre: "explore",
        tier: TierFase::Cheap,
        duenos: &[],
    },
    Fase {
        nombre: "propose",
        tier: TierFase::Strong,
        duenos: &["alberti"],
    },
    Fase {
        nombre: "spec",
        tier: TierFase::Mid,
        duenos: &["alberti"],
    },
    Fase {
        nombre: "design",
        tier: TierFase::Strong,
        duenos: &["donatello"],
    },
    Fase {
        nombre: "contracts",
        tier: TierFase::Strong,
        duenos: &["pacioli"],
    },
    Fase {
        nombre: "tasks",
        tier: TierFase::Mid,
        duenos: &["alberti", "leonardo"],
    },
    Fase {
        nombre: "apply",
        tier: TierFase::Mid,
        duenos: &["brunelleschi", "michelangelo"],
    },
    Fase {
        nombre: "verify",
        tier: TierFase::Strong,
        duenos: &["vasari", "raphael"],
    },
    Fase {
        nombre: "judge",
        tier: TierFase::Strong,
        duenos: &["galileo"],
    },
    Fase {
        nombre: "archive",
        tier: TierFase::Cheap,
        duenos: &[],
    },
];

/// `true` si `nombre` es una de las fases conocidas.
pub fn fase_existe(nombre: &str) -> bool {
    FASES.iter().any(|f| f.nombre == nombre)
}

/// Slugs de las personas que participan del flujo SDD (unión de los dueños de todas las fases).
/// Las personas fuera de este conjunto (p. ej. `botticelli`/seo) nunca las toca un perfil.
pub fn slugs_flujo() -> BTreeSet<String> {
    FASES
        .iter()
        .flat_map(|f| f.duenos.iter().copied())
        .map(String::from)
        .collect()
}

/// Un perfil nombrado: qué alias de modelo recibe cada tier.
pub struct Perfil {
    /// Nombre del perfil (`cheap`/`balanced`/`premium`).
    pub nombre: &'static str,
    /// Alias para las fases `strong`.
    pub strong: &'static str,
    /// Alias para las fases `mid`.
    pub mid: &'static str,
    /// Alias para las fases `cheap`.
    pub cheap: &'static str,
    /// Descripción corta para la ayuda.
    pub nota: &'static str,
}

impl Perfil {
    /// Alias de modelo que este perfil asigna a un tier.
    pub fn modelo_de_tier(&self, tier: TierFase) -> &'static str {
        match tier {
            TierFase::Strong => self.strong,
            TierFase::Mid => self.mid,
            TierFase::Cheap => self.cheap,
        }
    }
}

/// Los tres perfiles nombrados. `cheap` = todo haiku; `balanced` = strong→opus, mid→sonnet,
/// cheap→haiku; `premium` = todo opus.
pub const PERFILES: &[Perfil] = &[
    Perfil {
        nombre: "cheap",
        strong: "haiku",
        mid: "haiku",
        cheap: "haiku",
        nota: "todo haiku (el más barato de la subscripción, no gratis)",
    },
    Perfil {
        nombre: "balanced",
        strong: "opus",
        mid: "sonnet",
        cheap: "haiku",
        nota: "orquestación y diseño en opus, implementación en sonnet, exploración en haiku",
    },
    Perfil {
        nombre: "premium",
        strong: "opus",
        mid: "opus",
        cheap: "opus",
        nota: "todo opus (razonamiento máximo en cada fase)",
    },
];

/// Busca un perfil por su nombre.
pub fn perfil_por_nombre(nombre: &str) -> Option<&'static Perfil> {
    PERFILES.iter().find(|p| p.nombre == nombre)
}

/// Rango de capacidad de un modelo, para la regla "el más fuerte gana" cuando una persona toca
/// fases de distinto tier. Orden total: desconocido/inherit(0) < haiku(1) < sonnet(2) < opus(3) <
/// fable(4). Cubre los alias y los ids fijos del catálogo. `inherit` (capacidad indeterminada)
/// pierde ante cualquier modelo concreto, así un override por fase con un modelo concreto manda.
pub fn rango_modelo(token: &str) -> u8 {
    match token {
        "haiku" | "claude-haiku-4-5" => 1,
        "sonnet" | "claude-sonnet-4-6" => 2,
        "opus" | "claude-opus-4-7" | "claude-opus-4-8" => 3,
        "claude-fable-5" => 4,
        _ => 0,
    }
}

/// Resuelve el modelo de cada persona del flujo SDD a partir de un perfil + overrides por fase.
///
/// Para cada fase, su modelo efectivo es el override explícito (`overrides_fase[fase]`) si lo hay,
/// o el alias del tier según el perfil. Luego, regla "el más fuerte gana": cada persona recibe el
/// modelo de mayor rango (`rango_modelo`) entre las fases que posee. En empate de rango, gana la
/// primera fase en el orden canónico de `FASES` (determinista). Las personas fuera del flujo (p.
/// ej. `botticelli`) NO aparecen en el resultado: el perfil no las toca. Las fases advisory (sin
/// dueños) no contribuyen a ninguna persona, pero su override igual puede registrarse en la receta.
pub fn perfil_resolver(
    perfil: &Perfil,
    overrides_fase: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for fase in FASES {
        let modelo: &str = overrides_fase
            .get(fase.nombre)
            .map(String::as_str)
            .unwrap_or_else(|| perfil.modelo_de_tier(fase.tier));
        for &slug in fase.duenos {
            let reemplazar = match out.get(slug) {
                None => true,
                Some(actual) => rango_modelo(modelo) > rango_modelo(actual),
            };
            if reemplazar {
                out.insert(slug.to_string(), modelo.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests_perfiles {
    use super::*;

    /// Atajo para armar el mapa esperado persona→modelo.
    fn mapa(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(s, m)| (s.to_string(), m.to_string()))
            .collect()
    }

    #[test]
    fn resolucion_por_perfil_table_driven() {
        let sin_overrides = BTreeMap::new();
        let casos: &[(&str, &[(&str, &str)])] = &[
            (
                "cheap",
                &[
                    ("leonardo", "haiku"),
                    ("alberti", "haiku"),
                    ("donatello", "haiku"),
                    ("pacioli", "haiku"),
                    ("brunelleschi", "haiku"),
                    ("michelangelo", "haiku"),
                    ("vasari", "haiku"),
                    ("raphael", "haiku"),
                    ("galileo", "haiku"),
                ],
            ),
            (
                "balanced",
                &[
                    ("leonardo", "opus"),
                    ("alberti", "opus"),
                    ("donatello", "opus"),
                    ("pacioli", "opus"),
                    ("brunelleschi", "sonnet"),
                    ("michelangelo", "sonnet"),
                    ("vasari", "opus"),
                    ("raphael", "opus"),
                    ("galileo", "opus"),
                ],
            ),
            (
                "premium",
                &[
                    ("leonardo", "opus"),
                    ("alberti", "opus"),
                    ("donatello", "opus"),
                    ("pacioli", "opus"),
                    ("brunelleschi", "opus"),
                    ("michelangelo", "opus"),
                    ("vasari", "opus"),
                    ("raphael", "opus"),
                    ("galileo", "opus"),
                ],
            ),
        ];
        for (nombre, esperado) in casos {
            let perfil = perfil_por_nombre(nombre).expect("perfil conocido");
            let got = perfil_resolver(perfil, &sin_overrides);
            assert_eq!(got, mapa(esperado), "perfil {nombre}");
        }
    }

    #[test]
    fn el_mas_fuerte_gana_en_personas_multifase() {
        // leonardo (init=strong, tasks=mid) y alberti (propose=strong, spec/tasks=mid) tienen una
        // fase strong: en balanced quedan en opus, no en sonnet (no se sub-potencian).
        let perfil = perfil_por_nombre("balanced").unwrap();
        let r = perfil_resolver(perfil, &BTreeMap::new());
        assert_eq!(r.get("leonardo").map(String::as_str), Some("opus"));
        assert_eq!(r.get("alberti").map(String::as_str), Some("opus"));
        // brunelleschi/michelangelo solo tienen apply (mid) → sonnet, más liviano que las multifase.
        assert_eq!(r.get("brunelleschi").map(String::as_str), Some("sonnet"));
        assert_eq!(r.get("michelangelo").map(String::as_str), Some("sonnet"));
    }

    #[test]
    fn botticelli_y_personas_fuera_del_flujo_no_se_tocan() {
        for nombre in ["cheap", "balanced", "premium"] {
            let perfil = perfil_por_nombre(nombre).unwrap();
            let r = perfil_resolver(perfil, &BTreeMap::new());
            assert!(
                !r.contains_key("botticelli"),
                "botticelli no participa del flujo ({nombre})"
            );
        }
    }

    #[test]
    fn override_por_fase_baja_a_la_persona_cuya_fase_no_esta_dominada() {
        // design es la única fase de donatello (strong). Bajar design a sonnet baja a donatello a sonnet.
        let perfil = perfil_por_nombre("balanced").unwrap();
        let overrides = mapa(&[("design", "sonnet")]);
        let r = perfil_resolver(perfil, &overrides);
        assert_eq!(r.get("donatello").map(String::as_str), Some("sonnet"));
    }

    #[test]
    fn override_por_fase_dominada_no_cambia_el_modelo_efectivo() {
        // spec (mid) es de alberti, que TAMBIÉN posee propose (strong→opus). Bajar spec a haiku no
        // cambia a alberti: queda en opus (la fase dominante manda). Degradación documentada.
        let perfil = perfil_por_nombre("balanced").unwrap();
        let overrides = mapa(&[("spec", "haiku")]);
        let r = perfil_resolver(perfil, &overrides);
        assert_eq!(r.get("alberti").map(String::as_str), Some("opus"));
    }

    #[test]
    fn override_puede_subir_una_persona_por_encima_del_perfil() {
        // En cheap (todo haiku), subir apply a opus sube a brunelleschi y michelangelo (única fase: apply).
        let perfil = perfil_por_nombre("cheap").unwrap();
        let overrides = mapa(&[("apply", "opus")]);
        let r = perfil_resolver(perfil, &overrides);
        assert_eq!(r.get("brunelleschi").map(String::as_str), Some("opus"));
        assert_eq!(r.get("michelangelo").map(String::as_str), Some("opus"));
        // Una persona que no toca apply sigue en haiku.
        assert_eq!(r.get("donatello").map(String::as_str), Some("haiku"));
    }

    #[test]
    fn override_inherit_rango_0_baja_a_inherit_en_fase_no_dominada() {
        // `inherit` tiene rango 0 (el más débil): en "el más fuerte gana" solo puede ganar cuando es
        // el ÚNICO modelo en juego para la persona (fase no dominada). design (strong) es la única
        // fase de donatello: bajar design a inherit baja a donatello a inherit.
        let perfil = perfil_por_nombre("balanced").unwrap();
        let overrides = mapa(&[("design", "inherit")]);
        let r = perfil_resolver(perfil, &overrides);
        assert_eq!(r.get("donatello").map(String::as_str), Some("inherit"));
        // Contraste: spec (mid) es de alberti, que también posee propose (strong→opus). Bajar la
        // fase dominada a inherit NO la afecta: el rango 0 pierde ante el opus de la fase dominante.
        let overrides = mapa(&[("spec", "inherit")]);
        let r = perfil_resolver(perfil, &overrides);
        assert_eq!(r.get("alberti").map(String::as_str), Some("opus"));
    }

    #[test]
    fn override_en_fase_advisory_no_agrega_ninguna_persona() {
        // explore y archive no tienen dueños: su override se podrá registrar, pero no toca personas.
        let perfil = perfil_por_nombre("balanced").unwrap();
        let base = perfil_resolver(perfil, &BTreeMap::new());
        for advisory in ["explore", "archive"] {
            let overrides = mapa(&[(advisory, "opus")]);
            let r = perfil_resolver(perfil, &overrides);
            assert_eq!(
                r, base,
                "override en fase advisory {advisory} no debe cambiar el mapa"
            );
        }
    }

    #[test]
    fn cobertura_de_slugs_todo_dueno_existe_en_personas() {
        // Guard contra drift del roster: cada slug dueño en FASES debe ser una persona real.
        let conocidas: BTreeSet<String> = personas().into_iter().map(|p| p.slug).collect();
        for fase in FASES {
            for &slug in fase.duenos {
                assert!(
                    conocidas.contains(slug),
                    "la fase '{}' apunta a un slug inexistente: '{slug}'",
                    fase.nombre
                );
            }
        }
    }

    #[test]
    fn rango_total_ordena_alias_e_ids() {
        assert!(rango_modelo("haiku") < rango_modelo("sonnet"));
        assert!(rango_modelo("sonnet") < rango_modelo("opus"));
        assert!(rango_modelo("opus") < rango_modelo("claude-fable-5"));
        // ids fijos comparten rango con su alias.
        assert_eq!(rango_modelo("claude-opus-4-8"), rango_modelo("opus"));
        assert_eq!(rango_modelo("claude-sonnet-4-6"), rango_modelo("sonnet"));
        // inherit/desconocido es el más débil: pierde ante cualquier concreto.
        assert_eq!(rango_modelo("inherit"), 0);
        assert!(rango_modelo("inherit") < rango_modelo("haiku"));
    }
}
