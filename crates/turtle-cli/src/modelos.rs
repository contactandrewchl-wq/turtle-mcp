//! `turtle modelos` — configurador de modelo por persona para Claude Code (por subscripción).
//!
//! Turtle es un MCP provider-agnóstico: cada persona se instala como subagente de Claude Code con
//! un campo `model:` que Claude Code respeta. Aquí el usuario elige ese modelo por persona; la
//! elección se guarda en `~/.turtle/models.conf` (formato `slug = modelo`, una línea por persona) y
//! se aplica reescribiendo los subagentes en `~/.claude/agents/`. No toca la base de Turtle ni
//! lanza procesos (RF observa-no-lanza): es pura configuración de lo que se escribe en los archivos.
//!
//! El modelo en uso lo determina qué CLI corres (Claude Code → Claude). El catálogo de modelos vive
//! en `turtle_service::MODELOS_CLAUDE`. Codex y otros CLIs tendrán su propio adaptador más adelante.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::AccionModelos;

/// Ruta del archivo de overrides (`~/.turtle/models.conf`).
fn ruta_overrides() -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    Some(home.join(".turtle").join("models.conf"))
}

/// El contenido estructurado de `~/.turtle/models.conf`. Dos capas en un mismo archivo:
/// - `overrides` (líneas `slug = modelo`): el **estado autoritativo** que consumen el hook
///   `PreToolUse` e `instalar_subagentes`. Lo edita `turtle modelos`.
/// - la *receta* de perfil (`perfil` + `fases`, escritas como directivas-comentario `#:perfil`/
///   `#:fase`): metadato que solo lee la capa de `turtle perfil`, para re-resolver e idempotencia.
///
/// Un único parser/serializador para ambas capas: así `turtle modelos set/reset` hace **round-trip**
/// de las directivas y no borra la receta de perfil (must-fix de QA del diseño). Las directivas
/// empiezan con `#`, por lo que un binario viejo (que solo conoce `slug = modelo`) las ignora:
/// el formato es 100% compatible hacia atrás.
#[derive(Default, Clone, PartialEq, Debug)]
pub(crate) struct Config {
    /// Overrides por persona: `slug → modelo` (estado autoritativo).
    pub overrides: BTreeMap<String, String>,
    /// Perfil activo declarado (`#:perfil = <nombre>`), si lo hay.
    pub perfil: Option<String>,
    /// Overrides explícitos por fase (`#:fase <fase> = <modelo>`).
    pub fases: BTreeMap<String, String>,
}

/// Lee la config completa de `~/.turtle/models.conf`. Si no existe o no se puede leer, vacía.
pub(crate) fn leer_config() -> Config {
    ruta_overrides()
        .and_then(|r| std::fs::read_to_string(r).ok())
        .map(|c| parsear_config(&c))
        .unwrap_or_default()
}

/// Lee solo los overrides `slug → modelo` (lo que consumen el hook y `instalar_subagentes`).
pub(crate) fn leer_overrides() -> BTreeMap<String, String> {
    leer_config().overrides
}

/// Parsea el contenido del archivo en sus dos capas. Las líneas `slug = modelo` van a `overrides`;
/// las directivas `#:perfil`/`#:fase` arman la receta. Ignora líneas vacías, comentarios comunes
/// (`#` que no son `#:`) y líneas mal formadas (sin `=`, o con clave/valor vacío). Tolera sangría
/// y CRLF. Una directiva desconocida (`#:` con otra clave) se ignora sin romper.
pub(crate) fn parsear_config(contenido: &str) -> Config {
    let mut cfg = Config::default();
    for linea in contenido.lines() {
        let linea = linea.trim();
        if linea.is_empty() {
            continue;
        }
        if let Some(directiva) = linea.strip_prefix("#:") {
            let Some((clave, valor)) = directiva.split_once('=') else {
                continue;
            };
            let valor = valor.trim();
            if valor.is_empty() {
                continue;
            }
            let mut campos = clave.split_whitespace();
            match (campos.next(), campos.next()) {
                (Some("perfil"), None) => cfg.perfil = Some(valor.to_string()),
                (Some("fase"), Some(fase)) => {
                    cfg.fases.insert(fase.to_string(), valor.to_string());
                }
                _ => {} // directiva desconocida: ignorar
            }
            continue;
        }
        if linea.starts_with('#') {
            continue; // comentario común
        }
        if let Some((slug, modelo)) = linea.split_once('=') {
            let slug = slug.trim();
            let modelo = modelo.trim();
            if !slug.is_empty() && !modelo.is_empty() {
                cfg.overrides.insert(slug.to_string(), modelo.to_string());
            }
        }
    }
    cfg
}

/// Serializa la config (round-trip con `parsear_config`): encabezado, la receta de perfil como
/// directivas `#:` (solo si hay perfil u overrides de fase) y las líneas `slug = modelo` ordenadas.
pub(crate) fn serializar_config(cfg: &Config) -> String {
    let mut texto = String::from(
        "# Turtle — modelo por persona (Claude Code). Edita con: turtle modelos set <persona>=<modelo>\n",
    );
    if cfg.perfil.is_some() || !cfg.fases.is_empty() {
        texto.push_str("# Perfil de modelo por fase — turtle perfil <cheap|balanced|premium>\n");
    }
    if let Some(p) = &cfg.perfil {
        texto.push_str(&format!("#:perfil = {p}\n"));
    }
    for (fase, modelo) in &cfg.fases {
        texto.push_str(&format!("#:fase {fase} = {modelo}\n"));
    }
    for (slug, modelo) in &cfg.overrides {
        texto.push_str(&format!("{slug} = {modelo}\n"));
    }
    texto
}

/// Escribe `contenido` en `ruta` de forma **atómica**: primero lo vuelca en `<ruta>.tmp` y luego
/// hace `rename` sobre el destino. Así un crash a mitad de escritura no deja un `models.conf`
/// corrupto a medio escribir: o queda la versión vieja entera o la nueva entera. En Windows
/// `fs::rename` reemplaza el destino existente (MoveFileEx con REPLACE_EXISTING). Helper local:
/// `setup.rs` aún escribe los sub-agentes con `fs::write` directo (fuera del alcance de este lote).
fn escribir_atomico(ruta: &std::path::Path, contenido: &str) -> std::io::Result<()> {
    let mut tmp = ruta.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contenido)?;
    std::fs::rename(&tmp, ruta)
}

/// Escribe la config completa en `~/.turtle/models.conf` (crea la carpeta si falta).
pub(crate) fn escribir_config(cfg: &Config) -> Result<(), String> {
    let ruta = ruta_overrides().ok_or("no se pudo determinar la carpeta del usuario.")?;
    if let Some(padre) = ruta.parent() {
        std::fs::create_dir_all(padre)
            .map_err(|e| format!("no se pudo crear {}: {e}", padre.display()))?;
    }
    escribir_atomico(&ruta, &serializar_config(cfg))
        .map_err(|e| format!("no se pudo escribir {}: {e}", ruta.display()))
}

/// Escribe los overrides por persona **preservando** la receta de perfil ya guardada (must-fix):
/// un `turtle modelos set/reset` no debe borrar `#:perfil`/`#:fase`. Para descartar también la
/// receta, usá `turtle perfil reset`.
fn escribir_overrides(overrides: &BTreeMap<String, String>) -> Result<(), String> {
    let mut cfg = leer_config();
    cfg.overrides = overrides.clone();
    escribir_config(&cfg)
}

/// Despacha `turtle modelos [acción]`. Sin acción abre el menú interactivo (o cae a `listar` si no
/// hay terminal interactiva, p. ej. cuando la salida está redirigida o se corre desde un hook).
pub(crate) fn ejecutar(accion: Option<AccionModelos>) -> Result<(), String> {
    match accion {
        None | Some(AccionModelos::Menu) => menu(),
        Some(AccionModelos::Listar) => listar(),
        Some(AccionModelos::Set { pares }) => set(pares),
        Some(AccionModelos::Reset { personas }) => reset(personas),
        Some(AccionModelos::Aplicar) => {
            let n = aplicar()?;
            println!("Subagentes reescritos: {n} (en ~/.claude/agents/).");
            Ok(())
        }
    }
}

/// Menú interactivo: elige una persona, luego un modelo; se aplica al instante. Repite hasta salir.
/// Si la entrada no es una terminal (redirección/pipe/hook), cae a la vista estática `listar`.
fn menu() -> Result<(), String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return listar();
    }
    let personas = turtle_service::personas();
    let modelos = turtle_service::MODELOS_CLAUDE;
    loop {
        let overrides = leer_overrides();
        println!("\n── Modelo por persona (Claude Code, por subscripción) ──");
        for (i, p) in personas.iter().enumerate() {
            let elegido = overrides.contains_key(&p.slug);
            let efectivo = overrides
                .get(&p.slug)
                .cloned()
                .unwrap_or_else(|| p.modelo_default.clone());
            let marca = if elegido { "elegido" } else { "default" };
            println!(
                "  {:>2}) {:<10} {:<14} {:<18} ({marca})",
                i + 1,
                p.slug,
                p.rol,
                efectivo
            );
        }
        println!("   r) restaurar todas a su modelo por defecto");
        println!("   0) salir");
        let sel = leer_linea("Persona (número): ")?;
        let sel = sel.trim();
        if sel.is_empty() || sel == "0" {
            break;
        }
        if sel.eq_ignore_ascii_case("r") {
            escribir_overrides(&BTreeMap::new())?;
            let n = aplicar()?;
            println!("Listo: todas a su modelo por defecto (subagentes reescritos: {n}).");
            continue;
        }
        let Some(persona) = sel
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 1 && *n <= personas.len())
            .map(|n| &personas[n - 1])
        else {
            println!("Opción inválida.");
            continue;
        };
        println!("\nModelo para {} ({}):", persona.slug, persona.rol);
        for (i, m) in modelos.iter().enumerate() {
            println!("  {:>2}) {:<18} {}", i + 1, m.token, m.nota);
        }
        println!("   0) cancelar");
        let selm = leer_linea("Modelo (número): ")?;
        let selm = selm.trim();
        if selm.is_empty() || selm == "0" {
            continue;
        }
        let Some(token) = selm
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 1 && *n <= modelos.len())
            .map(|n| modelos[n - 1].token.to_string())
        else {
            println!("Opción inválida.");
            continue;
        };
        let mut overrides = leer_overrides();
        overrides.insert(persona.slug.clone(), token.clone());
        escribir_overrides(&overrides)?;
        let n = aplicar()?;
        println!(
            "✓ {} → {}  (subagentes reescritos: {n})",
            persona.slug, token
        );
    }
    println!("Listo.");
    Ok(())
}

/// Imprime un prompt y lee una línea de la entrada estándar.
fn leer_linea(prompt: &str) -> Result<String, String> {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut linea = String::new();
    std::io::stdin()
        .read_line(&mut linea)
        .map_err(|e| format!("no se pudo leer la entrada: {e}"))?;
    Ok(linea)
}

/// Muestra cada persona con su modelo efectivo y el catálogo de modelos disponibles.
fn listar() -> Result<(), String> {
    let overrides = leer_overrides();
    let personas = turtle_service::personas();
    println!("Personas (modelo efectivo en Claude Code):");
    for p in &personas {
        match overrides.get(&p.slug) {
            Some(m) => println!("  {:<10} {:<14} {}  (elegido)", p.slug, p.rol, m),
            None => println!(
                "  {:<10} {:<14} {}  (por defecto)",
                p.slug, p.rol, p.modelo_default
            ),
        }
    }
    println!("\nModelos disponibles (por subscripción):");
    for m in turtle_service::MODELOS_CLAUDE {
        println!("  {:<18} {}", m.token, m.nota);
    }
    println!("\nMenú interactivo: turtle modelos        (elegir desde una lista)");
    println!("Cambiar directo:  turtle modelos set <persona>=<modelo> [<persona>=<modelo> ...]");
    println!("Restaurar:        turtle modelos reset [<persona> ...]   (sin args = todas)");
    Ok(())
}

/// Fija el modelo de una o más personas: `set donatello=opus brunelleschi=claude-fable-5`.
fn set(pares: Vec<String>) -> Result<(), String> {
    let personas = turtle_service::personas();
    let conocidas: std::collections::BTreeSet<&str> =
        personas.iter().map(|p| p.slug.as_str()).collect();
    let mut overrides = leer_overrides();
    let mut cambios = Vec::new();
    for par in &pares {
        let (slug, modelo) = par
            .split_once('=')
            .ok_or_else(|| format!("formato inválido: '{par}'. Use persona=modelo."))?;
        let slug = slug.trim();
        let modelo = modelo.trim();
        if !conocidas.contains(slug) {
            let nombres: Vec<&str> = personas.iter().map(|p| p.slug.as_str()).collect();
            return Err(format!(
                "persona desconocida: '{slug}'. Opciones: {}.",
                nombres.join(", ")
            ));
        }
        if !turtle_service::modelo_valido(modelo) {
            let tokens: Vec<&str> = turtle_service::MODELOS_CLAUDE
                .iter()
                .map(|m| m.token)
                .collect();
            return Err(format!(
                "modelo desconocido: '{modelo}'. Opciones: {}.",
                tokens.join(", ")
            ));
        }
        overrides.insert(slug.to_string(), modelo.to_string());
        cambios.push((slug.to_string(), modelo.to_string()));
    }
    escribir_overrides(&overrides)?;
    let n = aplicar()?;
    for (slug, modelo) in &cambios {
        println!("{slug} → {modelo}");
    }
    println!("Listo. Subagentes reescritos: {n}.");
    Ok(())
}

/// Quita el override de las personas dadas (vacío = todas) y reaplica.
fn reset(personas: Vec<String>) -> Result<(), String> {
    let mut overrides = leer_overrides();
    if personas.is_empty() {
        overrides.clear();
        println!("Se quitaron todos los overrides (todas vuelven a su modelo por defecto).");
    } else {
        for slug in &personas {
            if overrides.remove(slug.trim()).is_some() {
                println!("{} → por defecto", slug.trim());
            } else {
                println!("{} no tenía override.", slug.trim());
            }
        }
    }
    escribir_overrides(&overrides)?;
    let n = aplicar()?;
    println!("Listo. Subagentes reescritos: {n}.");
    Ok(())
}

/// Reescribe los subagentes de Claude Code aplicando los overrides actuales. Devuelve cuántos.
fn aplicar() -> Result<usize, String> {
    let overrides = leer_overrides();
    crate::setup::instalar_subagentes(&overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(s, m)| (s.to_string(), m.to_string()))
            .collect()
    }

    #[test]
    fn round_trip_de_overrides() {
        let cfg = Config {
            overrides: overrides(&[("donatello", "opus"), ("brunelleschi", "claude-fable-5")]),
            ..Config::default()
        };
        assert_eq!(parsear_config(&serializar_config(&cfg)), cfg);
    }

    #[test]
    fn parser_ignora_comentarios_vacias_y_mal_formadas() {
        let contenido = "\
# encabezado de comentario
donatello = opus

   # otra nota indentada
brunelleschi = haiku
linea_sin_igual
=opus
raphael =
  michelangelo  =  sonnet
";
        let m = parsear_config(contenido).overrides;
        assert_eq!(m.get("donatello").map(String::as_str), Some("opus"));
        assert_eq!(m.get("brunelleschi").map(String::as_str), Some("haiku"));
        // Sangría y espacios alrededor del `=` se recortan.
        assert_eq!(m.get("michelangelo").map(String::as_str), Some("sonnet"));
        // Mal formadas / vacías no entran.
        assert!(!m.contains_key("linea_sin_igual"));
        assert!(!m.contains_key("")); // "=opus" tiene slug vacío
        assert!(!m.contains_key("raphael")); // "raphael =" tiene modelo vacío
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn parser_tolera_crlf() {
        let m = parsear_config("donatello = opus\r\nbrunelleschi = sonnet\r\n").overrides;
        assert_eq!(m.get("donatello").map(String::as_str), Some("opus"));
        assert_eq!(m.get("brunelleschi").map(String::as_str), Some("sonnet"));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn vacio_round_trip_a_vacio() {
        let vacio = Config::default();
        // El serializado solo tiene el encabezado comentado; parsear vuelve a vacío.
        assert_eq!(parsear_config(&serializar_config(&vacio)), vacio);
        assert_eq!(parsear_config(""), vacio);
    }

    #[test]
    fn no_regresion_un_models_conf_legacy_se_lee_igual() {
        // Un archivo legacy (solo `slug = modelo`, sin directivas `#:`) se sigue leyendo igual: sin
        // perfil, sin overrides de fase, y con exactamente los mismos overrides por persona.
        let legacy = "\
# Turtle — modelo por persona (Claude Code).
donatello = opus
brunelleschi = sonnet
botticelli = haiku
";
        let cfg = parsear_config(legacy);
        assert_eq!(
            cfg.overrides,
            overrides(&[
                ("donatello", "opus"),
                ("brunelleschi", "sonnet"),
                ("botticelli", "haiku")
            ])
        );
        assert!(cfg.perfil.is_none());
        assert!(cfg.fases.is_empty());
    }

    #[test]
    fn round_trip_de_la_receta_de_perfil() {
        let cfg = Config {
            overrides: overrides(&[("donatello", "sonnet"), ("leonardo", "opus")]),
            perfil: Some("balanced".to_string()),
            fases: overrides(&[("design", "sonnet"), ("spec", "haiku")]),
        };
        // Serializar y volver a parsear preserva ambas capas idénticas.
        assert_eq!(parsear_config(&serializar_config(&cfg)), cfg);
        // Y el formato emitido usa las directivas-comentario esperadas.
        let texto = serializar_config(&cfg);
        assert!(texto.contains("#:perfil = balanced"));
        assert!(texto.contains("#:fase design = sonnet"));
    }

    #[test]
    fn las_directivas_son_invisibles_para_la_capa_de_overrides() {
        // Las directivas `#:` NO crean overrides fantasma: la capa autoritativa solo ve `slug=modelo`.
        let cfg = parsear_config("#:perfil = balanced\n#:fase design = sonnet\ndonatello = opus\n");
        assert_eq!(cfg.overrides, overrides(&[("donatello", "opus")]));
        assert!(!cfg.overrides.contains_key("perfil"));
        assert!(!cfg.overrides.contains_key("design"));
    }

    #[test]
    fn set_de_modelos_preserva_la_receta_de_perfil() {
        // Must-fix: aplicar un `turtle modelos set <persona>` (que solo toca `overrides`) y
        // reescribir no debe borrar `#:perfil`/`#:fase`. Simula el efecto del serializador unificado.
        let inicial = Config {
            overrides: overrides(&[("donatello", "opus"), ("brunelleschi", "sonnet")]),
            perfil: Some("balanced".to_string()),
            fases: overrides(&[("design", "sonnet")]),
        };
        let texto = serializar_config(&inicial);
        // Releer (como hace escribir_overrides) y cambiar SOLO un override.
        let mut cfg = parsear_config(&texto);
        cfg.overrides
            .insert("brunelleschi".to_string(), "opus".to_string());
        let releido = parsear_config(&serializar_config(&cfg));
        assert_eq!(releido.perfil.as_deref(), Some("balanced"));
        assert_eq!(
            releido.fases.get("design").map(String::as_str),
            Some("sonnet")
        );
        assert_eq!(
            releido.overrides.get("brunelleschi").map(String::as_str),
            Some("opus")
        );
    }

    #[test]
    fn reset_de_todos_los_modelos_preserva_la_receta_de_perfil() {
        // No-regresión (complementa el test de `set`): `turtle modelos reset` SIN args vacía TODOS
        // los overrides por persona, pero NO debe borrar la receta `#:perfil`/`#:fase`. Para
        // descartar también la receta está `turtle perfil reset`. Simula el round-trip que hace
        // `escribir_overrides`: releer la config y reemplazar solo la capa de overrides.
        let inicial = Config {
            overrides: overrides(&[
                ("donatello", "opus"),
                ("brunelleschi", "sonnet"),
                ("botticelli", "haiku"),
            ]),
            perfil: Some("balanced".to_string()),
            fases: overrides(&[("design", "sonnet"), ("spec", "haiku")]),
        };
        let mut cfg = parsear_config(&serializar_config(&inicial));
        cfg.overrides.clear(); // efecto de `reset` sin personas (todas a su default)
        let releido = parsear_config(&serializar_config(&cfg));
        // La receta sobrevive entera...
        assert_eq!(releido.perfil.as_deref(), Some("balanced"));
        assert_eq!(
            releido.fases.get("design").map(String::as_str),
            Some("sonnet")
        );
        assert_eq!(
            releido.fases.get("spec").map(String::as_str),
            Some("haiku")
        );
        // ...y no quedó ningún override por persona.
        assert!(releido.overrides.is_empty());
    }

    #[test]
    fn fase_desconocida_round_trip_sin_override_fantasma_y_el_resolver_la_ignora() {
        // Parser tolerante: una directiva `#:fase <fase_desconocida> = x` se conserva como dato (el
        // parser no valida contra FASES), pero NO crea un override fantasma en la capa autoritativa,
        // y el resolver del perfil la ignora (solo itera las FASES conocidas).
        let cfg = parsear_config("#:perfil = balanced\n#:fase noexiste = opus\ndonatello = haiku\n");
        // No se filtró a la capa de overrides.
        assert_eq!(cfg.overrides, overrides(&[("donatello", "haiku")]));
        assert!(!cfg.overrides.contains_key("noexiste"));
        // Round-trip: serializar y volver a parsear preserva la directiva desconocida tal cual.
        let round = parsear_config(&serializar_config(&cfg));
        assert_eq!(round, cfg);
        assert_eq!(round.fases.get("noexiste").map(String::as_str), Some("opus"));
        // El resolver la ignora: el mapa resuelto es idéntico al de balanced sin overrides de fase.
        let perfil = turtle_service::perfil_por_nombre("balanced").unwrap();
        let con_fase_fantasma = turtle_service::perfil_resolver(perfil, &round.fases);
        let sin_overrides = turtle_service::perfil_resolver(perfil, &BTreeMap::new());
        assert_eq!(con_fase_fantasma, sin_overrides);
    }
}
