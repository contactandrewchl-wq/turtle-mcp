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

/// Lee los overrides `slug → modelo`. Si el archivo no existe o no se puede leer, devuelve vacío.
pub(crate) fn leer_overrides() -> BTreeMap<String, String> {
    ruta_overrides()
        .and_then(|r| std::fs::read_to_string(r).ok())
        .map(|c| parsear_overrides(&c))
        .unwrap_or_default()
}

/// Parsea el contenido del archivo: una línea `slug = modelo` por persona; ignora líneas vacías,
/// comentarios (`#`) y mal formadas (sin `=`, o con slug/modelo vacío). Tolera sangría y CRLF.
fn parsear_overrides(contenido: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for linea in contenido.lines() {
        let linea = linea.trim();
        if linea.is_empty() || linea.starts_with('#') {
            continue;
        }
        if let Some((slug, modelo)) = linea.split_once('=') {
            let slug = slug.trim();
            let modelo = modelo.trim();
            if !slug.is_empty() && !modelo.is_empty() {
                out.insert(slug.to_string(), modelo.to_string());
            }
        }
    }
    out
}

/// Serializa los overrides (ordenados por slug) con un encabezado explicativo. Round-trip con
/// `parsear_overrides`.
fn serializar_overrides(overrides: &BTreeMap<String, String>) -> String {
    let mut texto = String::from(
        "# Turtle — modelo por persona (Claude Code). Edita con: turtle modelos set <persona>=<modelo>\n",
    );
    for (slug, modelo) in overrides {
        texto.push_str(&format!("{slug} = {modelo}\n"));
    }
    texto
}

/// Escribe los overrides en `~/.turtle/models.conf` (crea la carpeta si falta).
fn escribir_overrides(overrides: &BTreeMap<String, String>) -> Result<(), String> {
    let ruta = ruta_overrides().ok_or("no se pudo determinar la carpeta del usuario.")?;
    if let Some(padre) = ruta.parent() {
        std::fs::create_dir_all(padre)
            .map_err(|e| format!("no se pudo crear {}: {e}", padre.display()))?;
    }
    std::fs::write(&ruta, serializar_overrides(overrides))
        .map_err(|e| format!("no se pudo escribir {}: {e}", ruta.display()))
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

    #[test]
    fn round_trip_de_overrides() {
        let mut m = BTreeMap::new();
        m.insert("donatello".to_string(), "opus".to_string());
        m.insert("brunelleschi".to_string(), "claude-fable-5".to_string());
        let parseado = parsear_overrides(&serializar_overrides(&m));
        assert_eq!(parseado, m);
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
        let m = parsear_overrides(contenido);
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
        let m = parsear_overrides("donatello = opus\r\nbrunelleschi = sonnet\r\n");
        assert_eq!(m.get("donatello").map(String::as_str), Some("opus"));
        assert_eq!(m.get("brunelleschi").map(String::as_str), Some("sonnet"));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn vacio_round_trip_a_vacio() {
        let vacio: BTreeMap<String, String> = BTreeMap::new();
        // El serializado solo tiene el encabezado comentado; parsear vuelve a vacío.
        assert!(parsear_overrides(&serializar_overrides(&vacio)).is_empty());
        assert!(parsear_overrides("").is_empty());
    }
}
