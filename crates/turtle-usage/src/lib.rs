//! `turtle-usage` — lectura best-effort del consumo de tokens desde los logs locales de Claude
//! Code (`~/.claude/projects/**/*.jsonl`), en Rust (sin npm). Lo usa la statusLine (`turtle-cli`),
//! que se redibuja al teclear: por eso ofrece [`leer_uso_cacheado`] con un caché de corta vida.
//!
//! El porcentaje es una **estimación**: el dato oficial es `/usage` dentro de Claude Code
//! (RF-T-04). Se suman `input + output + cache_creation` (se excluye `cache_read`, que domina el
//! total y no representa el consumo real). Si no hay logs, devuelve `0` (degradación elegante).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ventana de 5 horas, en ms.
const VENTANA_5H_MS: i64 = 5 * 60 * 60 * 1000;
/// Ventana semanal (7 días), en ms.
const VENTANA_SEM_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Ancho de la barra del medidor.
const ANCHO_BARRA: usize = 10;
/// Vida por defecto del caché de uso (ms). Configurable con `TURTLE_USAGE_TTL_MS` (0 lo desactiva).
const TTL_CACHE_MS_DEFECTO: i64 = 15_000;

/// Consumo estimado de tokens por ventana.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Uso {
    pub cinco_horas: u64,
    pub semana: u64,
}

/// Lee el consumo de las últimas 5 h y 7 días desde los logs de Claude Code.
///
/// Hace un barrido completo: relee y parsea todos los transcripts (`*.jsonl`) tocados en la última
/// semana. Es caro; para llamadas muy frecuentes usá [`leer_uso_cacheado`].
pub fn leer_uso() -> Uso {
    leer_uso_en(now_ms())
}

/// Igual que [`leer_uso`] pero con un caché de corta vida en disco.
///
/// La statusLine de Claude Code se redibuja al teclear; recalcular en cada redibujo implica releer
/// y parsear todos los transcripts, lo que **lagea el tipeo**. Si el caché es más nuevo que el TTL
/// (`TURTLE_USAGE_TTL_MS`, por defecto 15 s; `0` lo desactiva), se reutiliza; si no, se recalcula y
/// se reescribe. Si no hay carpeta de caché, cae con elegancia en un barrido completo.
pub fn leer_uso_cacheado() -> Uso {
    let ahora = now_ms();
    let ttl = std::env::var("TURTLE_USAGE_TTL_MS")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(TTL_CACHE_MS_DEFECTO);
    let ruta = ruta_cache();
    if ttl > 0 {
        if let Some(p) = &ruta {
            if let Some((ts, uso)) = leer_cache(p) {
                // Caché vigente (y con marca de tiempo no futura): se reutiliza sin tocar disco.
                if (0..ttl).contains(&(ahora - ts)) {
                    return uso;
                }
            }
        }
    }
    let uso = leer_uso_en(ahora);
    if let Some(p) = &ruta {
        let _ = escribir_cache(p, ahora, &uso);
    }
    uso
}

/// Render de una ventana: barra + % si `env_limite` trae un número; si no, el conteo humano.
pub fn medidor(env_limite: &str, tokens: u64) -> String {
    match std::env::var(env_limite)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|l| *l > 0)
    {
        Some(limite) => {
            let (barra, pct) = barra_pct(tokens, limite, ANCHO_BARRA);
            format!("{barra} {pct}% (est.)")
        }
        None => format!("{} tok", formato_tokens(tokens)),
    }
}

/// Formato humano compacto de una cantidad de tokens (500, 1.5k, 2.3M).
pub fn formato_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Barra de bloques y porcentaje (recortado a 0..100) de `usados` sobre `limite`.
fn barra_pct(usados: u64, limite: u64, ancho: usize) -> (String, u8) {
    let frac = (usados as f64 / limite as f64).clamp(0.0, 1.0);
    let llenas = (frac * ancho as f64).round() as usize;
    let barra = format!("▕{}{}▏", "█".repeat(llenas), "░".repeat(ancho - llenas));
    (barra, (frac * 100.0).round() as u8)
}

fn leer_uso_en(ahora_ms: i64) -> Uso {
    let Some(base) = directories::BaseDirs::new() else {
        return Uso::default();
    };
    let dir = base.home_dir().join(".claude").join("projects");
    let limite_sem = ahora_ms - VENTANA_SEM_MS;
    let mut uso = Uso::default();
    for proyecto in leer_dir(&dir) {
        for archivo in leer_dir(&proyecto) {
            if archivo.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if !modificado_desde(&archivo, limite_sem) {
                continue;
            }
            acumular_archivo(&archivo, ahora_ms, &mut uso);
        }
    }
    uso
}

fn acumular_archivo(archivo: &Path, ahora_ms: i64, uso: &mut Uso) {
    let Ok(contenido) = std::fs::read_to_string(archivo) else {
        return;
    };
    for linea in contenido.lines() {
        if let Some((ts, tokens)) = entrada_uso(linea) {
            let edad = ahora_ms - ts;
            if (0..=VENTANA_SEM_MS).contains(&edad) {
                uso.semana += tokens;
                if edad <= VENTANA_5H_MS {
                    uso.cinco_horas += tokens;
                }
            }
        }
    }
}

/// Extrae `(timestamp_ms, tokens)` de una línea del transcript de Claude Code, o `None`.
fn entrada_uso(linea: &str) -> Option<(i64, u64)> {
    let v: serde_json::Value = serde_json::from_str(linea).ok()?;
    let ts = v.get("timestamp")?.as_str()?;
    let ms = chrono::DateTime::parse_from_rfc3339(ts)
        .ok()?
        .timestamp_millis();
    let usage = v.get("message")?.get("usage")?;
    let tokens = [
        "input_tokens",
        "output_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .map(|k| usage.get(*k).and_then(|x| x.as_u64()).unwrap_or(0))
    .sum();
    Some((ms, tokens))
}

/// Ruta del caché de uso: `<config>/turtle/usage-cache.txt`. `None` si no hay carpeta de config.
fn ruta_cache() -> Option<PathBuf> {
    let b = directories::BaseDirs::new()?;
    Some(b.config_dir().join("turtle").join("usage-cache.txt"))
}

/// Lee `(timestamp_ms, Uso)` del caché. Formato: tres enteros separados por espacios. `None` si no
/// existe o está corrupto (se tratará como caché ausente y se recalculará).
fn leer_cache(p: &Path) -> Option<(i64, Uso)> {
    let s = std::fs::read_to_string(p).ok()?;
    let mut it = s.split_whitespace();
    let ts = it.next()?.parse::<i64>().ok()?;
    let cinco_horas = it.next()?.parse::<u64>().ok()?;
    let semana = it.next()?.parse::<u64>().ok()?;
    Some((
        ts,
        Uso {
            cinco_horas,
            semana,
        },
    ))
}

/// Escribe el caché de uso de forma best-effort (crea la carpeta si hace falta).
fn escribir_cache(p: &Path, ts: i64, uso: &Uso) -> std::io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, format!("{ts} {} {}", uso.cinco_horas, uso.semana))
}

fn leer_dir(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

fn modificado_desde(archivo: &Path, limite_ms: i64) -> bool {
    let Ok(meta) = std::fs::metadata(archivo) else {
        return false;
    };
    match meta.modified() {
        Ok(t) => {
            let ms = t
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            ms >= limite_ms
        }
        Err(_) => true,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_humano_de_tokens() {
        assert_eq!(formato_tokens(500), "500");
        assert_eq!(formato_tokens(1_500), "1.5k");
        assert_eq!(formato_tokens(2_300_000), "2.3M");
    }

    #[test]
    fn barra_y_porcentaje() {
        let (barra, pct) = barra_pct(50, 100, 10);
        assert_eq!(pct, 50);
        assert_eq!(barra.matches('█').count(), 5);
        assert_eq!(barra.matches('░').count(), 5);
        let (_, pct) = barra_pct(300, 100, 10);
        assert_eq!(pct, 100);
    }

    #[test]
    fn parsea_una_entrada_de_uso_real() {
        let linea = r#"{"timestamp":"2026-06-19T14:49:24.965Z","message":{"usage":{"input_tokens":10,"cache_creation_input_tokens":2,"cache_read_input_tokens":3,"output_tokens":5}}}"#;
        let (ms, tokens) = entrada_uso(linea).unwrap();
        assert!(ms > 0);
        assert_eq!(tokens, 17); // 10 + 5 + 2 (excluye cache_read)
        assert!(entrada_uso(r#"{"type":"user"}"#).is_none());
        assert!(entrada_uso("no es json").is_none());
    }

    #[test]
    fn medidor_sin_limite_muestra_conteo() {
        assert_eq!(
            medidor("TURTLE_LIMIT_QUE_NO_EXISTE_98765", 1_500),
            "1.5k tok"
        );
    }

    #[test]
    fn cache_hace_round_trip_y_descarta_basura() {
        let mut p = std::env::temp_dir();
        p.push(format!("turtle_usage_cache_{}.txt", std::process::id()));
        let uso = Uso {
            cinco_horas: 1234,
            semana: 56789,
        };
        escribir_cache(&p, 1000, &uso).unwrap();
        assert_eq!(leer_cache(&p), Some((1000, uso)));
        // Contenido corrupto: se trata como caché ausente, no entra en pánico.
        std::fs::write(&p, "no son numeros").unwrap();
        assert_eq!(leer_cache(&p), None);
        let _ = std::fs::remove_file(&p);
    }
}
