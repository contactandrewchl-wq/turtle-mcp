//! Ingesta de skills y agentes del ecosistema (RF-SKL-01/02/06): recorre `skills/` y `agents/`,
//! parsea el frontmatter (`name`, `description`, `type`, `tags`) y devuelve `NewSkill`s listos
//! para persistir. Sin dependencias nuevas: parser de frontmatter mínimo, sin serde_yaml.

use std::path::{Path, PathBuf};

use turtle_core::skill::{NewSkill, SkillKind};

/// Profundidad máxima al recorrer un directorio de skills (cubre `skills/<n>/SKILL.md`).
const PROF_MAX: usize = 4;

/// Resultado de un escaneo: las skills encontradas y los directorios que existían y se leyeron.
pub struct Escaneo {
    pub skills: Vec<NewSkill>,
    pub fuentes: Vec<PathBuf>,
}

/// Directorios de skills/agentes locales al proyecto (relativos al `cwd`). Cubre los de Claude Code
/// (`.claude/`) y los de OpenCode (`.opencode/`), además de las carpetas planas `skills`/`agents`.
pub fn rutas_proyecto(cwd: &Path) -> Vec<PathBuf> {
    vec![
        cwd.join("skills"),
        cwd.join("agents"),
        cwd.join(".claude").join("skills"),
        cwd.join(".claude").join("agents"),
        cwd.join(".opencode").join("skills"),
        cwd.join(".opencode").join("agents"),
    ]
}

/// Directorios de skills/agentes globales del usuario: `~/.claude/...` (Claude Code) y
/// `~/.config/opencode/...` (OpenCode, anclado a home en todas las plataformas).
pub fn rutas_globales() -> Vec<PathBuf> {
    match home_dir() {
        Some(h) => vec![
            h.join(".claude").join("skills"),
            h.join(".claude").join("agents"),
            h.join(".config").join("opencode").join("skills"),
            h.join(".config").join("opencode").join("agents"),
        ],
        None => Vec::new(),
    }
}

/// Escanea las rutas dadas. Un directorio llamado `agents` se trata como agentes; el resto, como
/// skills. A las skills halladas se les asigna `project` (vacío = global).
pub fn escanear(rutas: &[PathBuf], project: &str) -> Escaneo {
    let mut skills = Vec::new();
    let mut fuentes = Vec::new();
    for raiz in rutas {
        if !raiz.is_dir() {
            continue;
        }
        let es_agente = raiz.file_name().and_then(|n| n.to_str()) == Some("agents");
        let archivos = archivos_md(raiz);
        if archivos.is_empty() {
            continue;
        }
        fuentes.push(raiz.clone());
        for arch in archivos {
            if let Some(s) = leer_skill(&arch, es_agente, project) {
                skills.push(s);
            }
        }
    }
    Escaneo { skills, fuentes }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn archivos_md(raiz: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    recolectar(raiz, 0, &mut out);
    out
}

fn recolectar(dir: &Path, prof: usize, out: &mut Vec<PathBuf>) {
    if prof > PROF_MAX {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            recolectar(&p, prof + 1, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("md") {
            out.push(p);
        }
    }
}

fn leer_skill(path: &Path, es_agente: bool, project: &str) -> Option<NewSkill> {
    let raw = std::fs::read_to_string(path).ok()?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("skill");
    let dir_padre = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());
    parsear_contenido(
        stem,
        dir_padre,
        &raw,
        es_agente,
        project,
        &path.to_string_lossy(),
    )
}

/// Parsea el contenido de un archivo de skill/agente (frontmatter + cuerpo) a un `NewSkill`.
/// Lo usan tanto la ingesta de disco como el sembrado de los assets embebidos en el binario.
pub fn parsear_contenido(
    stem: &str,
    dir_padre: Option<&str>,
    raw: &str,
    es_agente: bool,
    project: &str,
    source: &str,
) -> Option<NewSkill> {
    let fm = parsear_frontmatter(raw);
    // Nombre: del frontmatter; si no, el del directorio cuando el archivo es `SKILL.md`/`AGENT.md`.
    let nombre = campo(&fm, "name").unwrap_or_else(|| {
        if stem.eq_ignore_ascii_case("skill") || stem.eq_ignore_ascii_case("agent") {
            dir_padre.unwrap_or(stem).to_string()
        } else {
            stem.to_string()
        }
    });
    if nombre.is_empty() {
        return None;
    }
    let kind = if es_agente {
        SkillKind::Agent
    } else {
        // El `type` puede venir al tope del frontmatter o anidado bajo `metadata:` (convención del
        // ecosistema, ver catalog.md). Si no está arriba, lo buscamos anidado antes de caer en el
        // valor por defecto: así las skills de comportamiento/herramienta no se degradan a conocimiento.
        campo(&fm, "type")
            .or_else(|| campo(&fm, "kind"))
            .or_else(|| campo_anidado(raw, "type"))
            .or_else(|| campo_anidado(raw, "kind"))
            .and_then(|t| SkillKind::parse(&t))
            .unwrap_or(SkillKind::Knowledge)
    };
    Some(NewSkill {
        project: project.to_string(),
        name: nombre,
        kind,
        when_to_use: campo(&fm, "description").or_else(|| campo(&fm, "when_to_use")),
        content: raw.to_string(),
        tags: campo(&fm, "tags"),
        source: Some(source.to_string()),
    })
}

/// Pares `clave: valor` del frontmatter YAML simple entre `---`. Vacío si no hay uno válido.
/// Soporta los block scalars `>` y `|`: pliega las líneas indentadas que siguen (frecuentes en
/// las `description:` de skills y agentes del ecosistema).
fn parsear_frontmatter(raw: &str) -> Vec<(String, String)> {
    let cuerpo = match raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))
    {
        Some(c) => c,
        None => return Vec::new(),
    };
    // Requiere un cierre `---` en su propia línea.
    let fin = match cuerpo.find("\n---") {
        Some(f) => f,
        None => return Vec::new(),
    };
    let lineas: Vec<&str> = cuerpo[..fin].lines().collect();
    let mut pares = Vec::new();
    let mut i = 0;
    while i < lineas.len() {
        let linea = lineas[i];
        i += 1;
        // Solo claves de nivel superior (sin sangría).
        if linea.is_empty() || linea.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((k, v)) = linea.split_once(':') else {
            continue;
        };
        let clave = k.trim().to_lowercase();
        let v = v.trim();
        let valor = if es_block_scalar(v) {
            // Junta las líneas siguientes más indentadas (o vacías) hasta la próxima clave.
            let mut partes = Vec::new();
            while i < lineas.len()
                && (lineas[i].trim().is_empty() || lineas[i].starts_with(char::is_whitespace))
            {
                let t = lineas[i].trim();
                if !t.is_empty() {
                    partes.push(t);
                }
                i += 1;
            }
            partes.join(" ")
        } else {
            limpiar_valor(v)
        };
        if !valor.is_empty() {
            pares.push((clave, valor));
        }
    }
    pares
}

/// `true` si el valor es un indicador de block scalar YAML (`>`, `|`, con cortes opcionales).
fn es_block_scalar(v: &str) -> bool {
    matches!(v, ">" | "|" | ">-" | "|-" | ">+" | "|+")
}

fn limpiar_valor(v: &str) -> String {
    v.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn campo(fm: &[(String, String)], clave: &str) -> Option<String> {
    fm.iter().find(|(k, _)| k == clave).map(|(_, v)| v.clone())
}

/// Busca la primera línea `<clave>: valor` dentro del frontmatter, con cualquier sangría (sirve para
/// campos anidados como `metadata.type` o `metadata.model`, que `parsear_frontmatter` no aplana).
pub(crate) fn campo_anidado(raw: &str, clave: &str) -> Option<String> {
    let cuerpo = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))?;
    let fin = cuerpo.find("\n---")?;
    let prefijo = format!("{clave}:");
    for linea in cuerpo[..fin].lines() {
        if let Some(v) = linea.trim().strip_prefix(&prefijo) {
            let v = v.trim().trim_matches('"').trim_matches('\'').trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_frontmatter_y_clasifica() {
        let dir = std::env::temp_dir().join(format!("turtle_scan_{}", ulid_simple()));
        let skills = dir.join("skills").join("ponytail");
        let agents = dir.join("agents");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::create_dir_all(&agents).unwrap();
        // `description` como block scalar YAML (`>`): debe plegarse a una sola línea.
        std::fs::write(
            skills.join("SKILL.md"),
            "---\nname: ponytail\ndescription: >\n  Maneja ramas\n  de git\ntype: tool\ntags: git\n---\nCuerpo.",
        )
        .unwrap();
        std::fs::write(
            agents.join("revisor.md"),
            "---\nname: revisor\ndescription: Revisa PRs\n---\nInstrucciones del subagente.",
        )
        .unwrap();

        let escaneo = escanear(&rutas_proyecto(&dir), "demo");
        let ponytail = escaneo
            .skills
            .iter()
            .find(|s| s.name == "ponytail")
            .unwrap();
        assert_eq!(ponytail.kind, SkillKind::Tool);
        assert_eq!(ponytail.when_to_use.as_deref(), Some("Maneja ramas de git"));
        assert_eq!(ponytail.project, "demo");
        let revisor = escaneo.skills.iter().find(|s| s.name == "revisor").unwrap();
        assert_eq!(revisor.kind, SkillKind::Agent); // por estar en agents/
        assert_eq!(escaneo.fuentes.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn type_anidado_bajo_metadata_se_respeta() {
        // Convención del ecosistema (catalog.md): el tipo va anidado bajo `metadata:`.
        let raw = "---\nname: x\ndescription: >\n  algo plegado\nlicense: Apache-2.0\nmetadata:\n  type: comportamiento\n  activation: permanente\n---\nCuerpo.";
        let s = parsear_contenido("x", None, raw, false, "demo", "src").unwrap();
        assert_eq!(
            s.kind,
            SkillKind::Behavior,
            "type anidado debe clasificar como comportamiento, no caer en conocimiento"
        );
        let raw_tool = "---\nname: y\nmetadata:\n  type: herramienta\n---\nCuerpo.";
        let t = parsear_contenido("y", None, raw_tool, false, "demo", "src").unwrap();
        assert_eq!(t.kind, SkillKind::Tool);
        // El tope sigue teniendo prioridad sobre lo anidado.
        let raw_top =
            "---\nname: z\ntype: knowledge\nmetadata:\n  type: comportamiento\n---\nCuerpo.";
        let z = parsear_contenido("z", None, raw_top, false, "demo", "src").unwrap();
        assert_eq!(z.kind, SkillKind::Knowledge);
    }

    // Pequeño identificador único para el directorio temporal de la prueba.
    fn ulid_simple() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
}
