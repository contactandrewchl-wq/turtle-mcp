//! `turtle statusline` — línea de estado para la statusLine de Claude Code (Capa 2 de la
//! integración). Combina la **rama** de git, el **modelo** (del JSON que Claude Code manda por
//! stdin) y el **consumo de tokens** (vía `turtle-usage`, que lee los logs locales en Rust).
//!
//! El porcentaje es una **estimación**: el dato oficial es `/usage` dentro de Claude Code
//! (RF-T-04). Por defecto se muestran conteos; con `TURTLE_LIMIT_5H` / `TURTLE_LIMIT_WEEK` se
//! muestra una barra con `%`.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

/// Tope de bytes que se leen del JSON de sesión de stdin (mismo motivo que en `hook.rs`): acota la
/// lectura para no consumir memoria sin techo ante un stdin patológico. 4 MiB cubre el JSON real.
const MAX_STDIN_BYTES: u64 = 4 * 1024 * 1024;

/// Imprime una línea de estado. Pensado para `statusLine.command` de Claude Code.
pub fn ejecutar() -> Result<(), String> {
    let sesion = leer_stdin_si_hay()
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

    let modelo = sesion
        .as_ref()
        .and_then(modelo_de_sesion)
        .unwrap_or_else(|| "claude".to_string());

    let cwd = sesion
        .as_ref()
        .and_then(cwd_de_sesion)
        .or_else(|| std::env::current_dir().ok());
    let rama = cwd
        .as_deref()
        .and_then(crate::rama_en)
        .unwrap_or_else(|| "—".to_string());

    // La statusLine se redibuja al teclear: usamos la lectura cacheada para no leer y parsear
    // todos los transcripts en cada redibujo (lo que lagea el tipeo).
    let uso = turtle_usage::leer_uso_cacheado();
    println!(
        "🐢 {rama} · {modelo} · 5h {} · sem {}",
        turtle_usage::medidor("TURTLE_LIMIT_5H", uso.cinco_horas),
        turtle_usage::medidor("TURTLE_LIMIT_WEEK", uso.semana),
    );
    Ok(())
}

/// Lee el JSON de sesión de stdin si Claude Code lo está enviando (no en ejecución interactiva).
fn leer_stdin_si_hay() -> Option<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    let mut s = String::new();
    // Lectura acotada al tope, por la misma razón que en el hook: evitar un consumo de memoria sin
    // techo ante un stdin patológico. Si se trunca, el JSON parcial no parsea y caemos en defaults.
    stdin
        .lock()
        .take(MAX_STDIN_BYTES)
        .read_to_string(&mut s)
        .ok()?;
    Some(s).filter(|s| !s.trim().is_empty())
}

fn modelo_de_sesion(v: &serde_json::Value) -> Option<String> {
    let m = v.get("model")?;
    m.get("display_name")
        .and_then(|x| x.as_str())
        .or_else(|| m.get("id").and_then(|x| x.as_str()))
        .or_else(|| m.as_str())
        .map(str::to_string)
}

fn cwd_de_sesion(v: &serde_json::Value) -> Option<PathBuf> {
    let s = v.get("cwd").and_then(|x| x.as_str()).or_else(|| {
        v.get("workspace")
            .and_then(|w| w.get("current_dir"))
            .and_then(|x| x.as_str())
    })?;
    Some(PathBuf::from(s))
}
