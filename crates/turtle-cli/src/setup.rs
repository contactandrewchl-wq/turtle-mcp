//! `turtle setup` — registra el servidor MCP de Turtle en la configuración de un cliente
//! (Claude Code, Cursor, etc.), para que lo levante solo en cada sesión (RF-IS-05, TF2-36).
//!
//! eliges el cliente (por argumento o por un menú) y
//! Turtle escribe la entrada MCP en su config, fusionándola con lo que ya haya. Resuelve la
//! ruta **absoluta** del binario (`current_exe`) para que el cliente lo encuentre aunque el
//! comando `turtle` no esté en el PATH del proceso que lo lanza.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Formato de archivo de configuración del cliente.
#[derive(Debug)]
enum Formato {
    /// JSON con un objeto `mcpServers` (Claude, Cursor, Windsurf, Gemini, …).
    JsonMcpServers,
    /// TOML con una tabla `[mcp_servers.<nombre>]` (Codex).
    Toml,
    /// JSON con un objeto `mcp` y entradas `{"type":"local","command":[…],"enabled":true}`
    /// (OpenCode). NO usa el `mcpServers` estándar ni `command` como string: el comando va como
    /// array de argumentos. Ver <https://opencode.ai/docs/mcp-servers/>.
    JsonMcpLocal,
}

/// Un cliente MCP que Turtle sabe configurar.
#[derive(Debug)]
struct Cliente {
    /// Identificador para `turtle setup <id>`.
    id: &'static str,
    /// Nombre legible.
    nombre: &'static str,
    formato: Formato,
}

const CLIENTES: &[Cliente] = &[
    Cliente {
        id: "claude-code",
        nombre: "Claude Code",
        formato: Formato::JsonMcpServers,
    },
    Cliente {
        id: "claude-desktop",
        nombre: "Claude Desktop",
        formato: Formato::JsonMcpServers,
    },
    Cliente {
        id: "cursor",
        nombre: "Cursor",
        formato: Formato::JsonMcpServers,
    },
    Cliente {
        id: "windsurf",
        nombre: "Windsurf",
        formato: Formato::JsonMcpServers,
    },
    Cliente {
        id: "gemini",
        nombre: "Gemini CLI",
        formato: Formato::JsonMcpServers,
    },
    Cliente {
        id: "codex",
        nombre: "Codex",
        formato: Formato::Toml,
    },
    Cliente {
        id: "opencode",
        nombre: "OpenCode",
        formato: Formato::JsonMcpLocal,
    },
];

/// Punto de entrada del subcomando. `agente` elige el cliente; si es `None`, se muestra un
/// menú. `config_override` permite apuntar a otro archivo (pruebas o rutas no estándar).
pub fn ejecutar(agente: Option<String>, config_override: Option<PathBuf>) -> Result<(), String> {
    let binario = std::env::current_exe()
        .map_err(|e| format!("no se pudo resolver la ruta del binario turtle: {e}"))?;

    let cliente = match agente {
        Some(id) => buscar_cliente(&id)?,
        None => elegir_interactivo()?,
    };

    let ruta = match config_override {
        Some(p) => p,
        None => ruta_config(cliente.id)
            .ok_or("no se pudo determinar la carpeta de configuración del usuario.")?,
    };

    match cliente.formato {
        Formato::JsonMcpServers => aplicar_json(&ruta, &binario)?,
        Formato::Toml => aplicar_toml(&ruta, &binario)?,
        Formato::JsonMcpLocal => aplicar_json_local(&ruta, &binario)?,
    }

    println!(
        "Turtle registrado para {} en {}.",
        cliente.nombre,
        ruta.display()
    );
    println!("Reinicia {} para que tome el servidor MCP.", cliente.nombre);

    // Configurador: además del MCP, inyecta el protocolo de Turtle en las instrucciones del cliente.
    if let Some(md) = ruta_instrucciones(cliente.id) {
        inyectar_protocolo(&md, cliente.id)?;
        println!(
            "Protocolo de Turtle inyectado en {} (bloque marcado e idempotente).",
            md.display()
        );
    }

    // Personas como subagentes nativos (Claude Code los muestra en su menú y respeta su `model`).
    if cliente.id == "claude-code" {
        // Respeta el modelo que el usuario haya elegido por persona (`turtle modelos`).
        let overrides = crate::modelos::leer_overrides();
        let n = instalar_subagentes(&overrides)?;
        println!("Personas instaladas como subagentes de Claude Code: {n} (en ~/.claude/agents/).");
        println!(
            "Modelo por persona: `turtle modelos` (menú interactivo) · `turtle modelos set <persona>=<modelo>` (directo)."
        );
        if let Some(s) = ruta_settings(cliente.id) {
            registrar_hooks(&s, &binario)?;
            println!(
                "Hooks de Turtle registrados en {} (actividad, contexto al iniciar y memorias al pedir; automático).",
                s.display()
            );
        }
    }
    Ok(())
}

fn buscar_cliente(id: &str) -> Result<&'static Cliente, String> {
    CLIENTES.iter().find(|c| c.id == id).ok_or_else(|| {
        let ids: Vec<&str> = CLIENTES.iter().map(|c| c.id).collect();
        format!("agente desconocido: {id}. Opciones: {}.", ids.join(", "))
    })
}

fn elegir_interactivo() -> Result<&'static Cliente, String> {
    println!("Clientes MCP que Turtle puede configurar:");
    for (i, c) in CLIENTES.iter().enumerate() {
        let marca = if config_presente(c.id) {
            " (detectado)"
        } else {
            ""
        };
        println!("  {}) {}{}", i + 1, c.nombre, marca);
    }
    print!("Elige un número (Enter para cancelar): ");
    std::io::stdout().flush().ok();

    let mut linea = String::new();
    std::io::stdin()
        .read_line(&mut linea)
        .map_err(|e| format!("no se pudo leer la elección: {e}"))?;
    let linea = linea.trim();
    if linea.is_empty() {
        return Err("operación cancelada.".to_string());
    }
    let n: usize = linea
        .parse()
        .map_err(|_| format!("entrada inválida: {linea}"))?;
    CLIENTES
        .get(n.wrapping_sub(1))
        .ok_or_else(|| format!("opción fuera de rango: {n}"))
}

/// `true` si el archivo de config del cliente, o su carpeta, ya existe (señal de que está instalado).
fn config_presente(id: &str) -> bool {
    ruta_config(id).is_some_and(|p| p.exists() || p.parent().map(Path::exists).unwrap_or(false))
}

/// Ruta del archivo de configuración de cada cliente, por convención de su proyecto.
fn ruta_config(id: &str) -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    let home = dirs.home_dir();
    Some(match id {
        "claude-code" => home.join(".claude.json"),
        "claude-desktop" => dirs
            .config_dir()
            .join("Claude")
            .join("claude_desktop_config.json"),
        "cursor" => home.join(".cursor").join("mcp.json"),
        "windsurf" => home
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json"),
        "gemini" => home.join(".gemini").join("settings.json"),
        "codex" => home.join(".codex").join("config.toml"),
        // OpenCode usa `~/.config/opencode/` literal en macOS, Linux y Windows nativo (no `%APPDATA%`),
        // así que se arma desde el home y no desde `config_dir()`. Ver https://opencode.ai/docs/config/.
        "opencode" => home.join(".config").join("opencode").join("opencode.json"),
        _ => return None,
    })
}

// ─── Inyección del protocolo en las instrucciones del cliente ───

/// Marcadores del bloque inyectado: permiten reemplazarlo sin duplicar ni pisar lo demás.
const MARCA_INI: &str =
    "<!-- TURTLE:BEGIN — generado por `turtle setup`; se reemplaza al re-ejecutar, no editar -->";
const MARCA_FIN: &str = "<!-- TURTLE:END -->";

/// Archivo de instrucciones globales donde inyectar el protocolo, por cliente. `None` = no soportado
/// aún. Codex lee `~/.codex/AGENTS.md` como instrucciones globales (o `AGENTS.override.md` si existe;
/// inyectamos en `AGENTS.md`, el caso por defecto). Gemini CLI lee `~/.gemini/GEMINI.md` como contexto
/// global (carga jerárquica global→workspace→JIT; el nombre se puede cambiar con `context.fileName`,
/// pero `GEMINI.md` es el valor por defecto).
fn ruta_instrucciones(id: &str) -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    match id {
        "claude-code" => Some(home.join(".claude").join("CLAUDE.md")),
        "codex" => Some(home.join(".codex").join("AGENTS.md")),
        "gemini" => Some(home.join(".gemini").join("GEMINI.md")),
        // OpenCode lee reglas globales de `~/.config/opencode/AGENTS.md`. Ver https://opencode.ai/docs/rules/.
        "opencode" => Some(home.join(".config").join("opencode").join("AGENTS.md")),
        _ => None,
    }
}

/// Núcleo del protocolo de Turtle (uso del MCP), neutral respecto del cliente: aplica a cualquier
/// CLI que consuma el MCP `turtle` (Claude Code, Codex, …).
fn protocolo_core() -> &'static str {
    r#"## Turtle — memoria persistente y coordinación (vía MCP)

Tienes disponible el servidor MCP `turtle`. Protocolo de uso:

- Al iniciar: llamá `session_start` con tu rótulo (rol/dominio) y la tarea; te entrega contexto y relevos pendientes.
- Antes de re-derivar algo: buscá con `memory_search`; traé el detalle con `memory_get` solo si hace falta (cuidá los tokens).
- Cuando decidas algo no obvio: guardalo con `memory_save` (tipo decision/architecture) con What/Why/Where/Learned.
- Skills: descubrí con `skills_search` y carga con `skill_get` (comportamiento always-on, conocimiento y herramienta).
- Coordinación/relevos: hablá con otros por rótulo con `message_send`; revisa tu `inbox`. Personas: backend, frontend, seguridad, arquitectura, revision, orquestador, sdd, api, seo.
- Al cerrar: `session_close` con un resumen.
- Respondé en español latino neutro."#
}

/// Sección de delegación a subagentes, específica de Claude Code (tool `Task`, árbol main/sub-agente,
/// modelos opus/sonnet). NO aplica a CLIs single-agent como Codex, así que no se inyecta ahí.
fn protocolo_delegacion_claude() -> &'static str {
    r#"## Delegación a sub-agentes (se ve en el árbol main/sub-agente de Claude Code)
Cuando la tarea tenga dueño claro, delegá con el tool Task en vez de hacer todo en el hilo principal:
- **Investigar, leer o recapitular** ("¿qué hicimos?", "leé/revisa X", "resumime Y") → sub-agente en **sonnet** (rápido y barato). Es el ÚNICO uso de sonnet.
- **Codear, diseñar arquitectura, razonar o decidir** → modelo frontera **opus 4.8** (todas las personas corren en opus): backend brunelleschi · frontend michelangelo · API pacioli · arquitectura donatello · plan/SDD alberti · seguridad raphael · revisión de PR vasari · SEO botticelli · coordinación leonardo.
- Lo trivial resolvelo en el hilo principal; no delegues por delegar."#
}

/// Protocolo a inyectar según el cliente: el núcleo siempre; la delegación a subagentes solo en
/// Claude Code (es lo único Claude-específico; el resto es provider-agnóstico).
fn texto_protocolo(id: &str) -> String {
    if id == "claude-code" {
        format!("{}\n\n{}", protocolo_core(), protocolo_delegacion_claude())
    } else {
        protocolo_core().to_string()
    }
}

/// Inserta o reemplaza el bloque marcado del protocolo en `ruta`, preservando el resto del archivo.
fn inyectar_protocolo(ruta: &Path, id: &str) -> Result<(), String> {
    let actual = if ruta.exists() {
        leer(ruta)?
    } else {
        String::new()
    };
    let bloque = format!("{}\n{}\n{}", MARCA_INI, texto_protocolo(id), MARCA_FIN);

    let nuevo = match (actual.find(MARCA_INI), actual.find(MARCA_FIN)) {
        (Some(i), Some(j)) if j > i => {
            let fin = j + MARCA_FIN.len();
            format!("{}{}{}", &actual[..i], bloque, &actual[fin..])
        }
        _ if actual.trim().is_empty() => format!("{bloque}\n"),
        _ => format!("{}\n\n{}\n", actual.trim_end(), bloque),
    };

    crear_padre(ruta)?;
    escribir(ruta, &nuevo)
}

/// Escribe las personas de Turtle como subagentes nativos de Claude Code (`~/.claude/agents/`).
/// El `model` de cada persona se respeta (asignación de modelo por subagente). No pisa archivos
/// ajenos: solo crea nuevos o reemplaza los que llevan el marcador de Turtle.
pub(crate) fn instalar_subagentes(overrides: &BTreeMap<String, String>) -> Result<usize, String> {
    let home = directories::BaseDirs::new()
        .ok_or("no se pudo determinar la carpeta del usuario.")?
        .home_dir()
        .to_path_buf();
    escribir_subagentes_en(&home.join(".claude").join("agents"), overrides)
}

/// Escribe los subagentes en `dir` (lo crea si falta), aplicando `overrides` (slug→modelo). No pisa
/// archivos ajenos (los que no llevan el marcador `TURTLE-AGENT`). Devuelve cuántos escribió.
/// Separada de la ruta del home para poder probarla con un directorio temporal.
fn escribir_subagentes_en(
    dir: &Path,
    overrides: &BTreeMap<String, String>,
) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
    let subagentes = turtle_service::subagentes_claude(overrides);
    let vigentes: std::collections::HashSet<String> =
        subagentes.iter().map(|sa| sa.slug.clone()).collect();
    let mut n = 0;
    for sa in &subagentes {
        let ruta = dir.join(format!("{}.md", sa.slug));
        if ruta.exists() {
            let actual = std::fs::read_to_string(&ruta).unwrap_or_default();
            if !actual.contains("TURTLE-AGENT") {
                continue; // respetar un subagente ajeno con el mismo nombre
            }
        }
        escribir(&ruta, &sa.contenido)?;
        n += 1;
    }
    // Poda: elimina los subagentes que escribió Turtle (llevan el marcador `TURTLE-AGENT`) cuyo slug
    // ya no corresponde a una persona vigente (p. ej. renombradas). Nunca toca archivos ajenos.
    if let Ok(entradas) = std::fs::read_dir(dir) {
        for entrada in entradas.flatten() {
            let ruta = entrada.path();
            if ruta.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let vigente = ruta
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|slug| vigentes.contains(slug));
            if vigente {
                continue;
            }
            if std::fs::read_to_string(&ruta)
                .unwrap_or_default()
                .contains("TURTLE-AGENT")
            {
                let _ = std::fs::remove_file(&ruta);
            }
        }
    }
    Ok(n)
}

// ─── Hook de actividad: registra lo que claude va haciendo (visible con «turtle actividad») ───

/// Archivo de settings de Claude Code donde van los hooks.
fn ruta_settings(id: &str) -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    match id {
        "claude-code" => Some(home.join(".claude").join("settings.json")),
        _ => None,
    }
}

/// Eventos de hook de Claude Code que Turtle registra, con el evento de `turtle hook <evento>` que
/// dispara cada uno y el `matcher` que lleva en settings.json. El esquema sale de la doc oficial de
/// hooks de Claude Code (verificado, junio 2026):
///   - `PreToolUse` admite matcher por nombre de tool; `"*"` cubre todas (feed de actividad).
///   - `SessionStart` admite matchers `startup`/`resume`/`clear`/`compact`; usamos `startup|resume`
///     para inyectar contexto al abrir y al reanudar, sin duplicar en `/clear` ni en compactaciones.
///   - `UserPromptSubmit` NO admite matcher (se ignora silenciosamente): se registra sin él.
///
/// El formato de salida que emite `hook.rs` (`hookSpecificOutput.additionalContext` con
/// `hookEventName` por evento) es el correcto para los tres según esa misma doc.
struct HookSpec {
    /// Clave bajo `hooks` en settings.json.
    evento_cc: &'static str,
    /// Subcomando de Turtle (`turtle hook <sub>`).
    sub: &'static str,
    /// Matcher a escribir, o `None` para eventos que no lo admiten (UserPromptSubmit).
    matcher: Option<&'static str>,
}

const HOOKS: &[HookSpec] = &[
    HookSpec {
        evento_cc: "PreToolUse",
        sub: "activity",
        matcher: Some("*"),
    },
    HookSpec {
        evento_cc: "SessionStart",
        sub: "session-start",
        matcher: Some("startup|resume"),
    },
    HookSpec {
        evento_cc: "UserPromptSubmit",
        sub: "prompt-submit",
        matcher: None,
    },
];

/// `true` si una entrada de hook es de Turtle: su comando invoca `... hook <evento>` a nuestro
/// binario. Generaliza la detección a cualquier subcomando `hook` (no solo `activity`), para que la
/// re-instalación y la desinstalación reconozcan los tres hooks que registramos y no dupliquen.
fn es_hook_turtle(entrada: &serde_json::Value) -> bool {
    entrada
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter().any(|h| {
                h.get("command")
                    .and_then(|c| c.as_str())
                    .map(comando_es_hook_turtle)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// `true` si un comando de hook es de Turtle: contiene la secuencia `hook <evento>` para alguno de
/// nuestros subcomandos. Reconoce el formato actual (`"<bin>" hook session-start`) sin depender del
/// matcher, así desinstala incluso hooks de versiones previas (que solo registraban `hook activity`).
fn comando_es_hook_turtle(cmd: &str) -> bool {
    HOOKS
        .iter()
        .any(|h| cmd.contains(&format!("hook {}", h.sub)))
}

/// Registra (idempotente) los tres hooks de Turtle en settings.json: `PreToolUse` (feed de
/// actividad), `SessionStart` (inyección de contexto + auto-mantenimiento) y `UserPromptSubmit`
/// (inyección de memorias relevantes + nudge de guardado). No pisa hooks ajenos: solo reemplaza los
/// que ya son de Turtle (detectados por su comando), preservando el resto del archivo.
fn registrar_hooks(ruta: &Path, binario: &Path) -> Result<(), String> {
    let mut raiz: serde_json::Value = if ruta.exists() {
        let txt = leer(ruta)?;
        if txt.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&txt)
                .map_err(|e| format!("settings.json inválido ({}): {e}", ruta.display()))?
        }
    } else {
        serde_json::json!({})
    };
    let obj = raiz
        .as_object_mut()
        .ok_or_else(|| format!("settings.json no es un objeto ({}).", ruta.display()))?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("`hooks` no es un objeto en settings.json.")?;

    for spec in HOOKS {
        let lista = hooks
            .entry(spec.evento_cc)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                format!(
                    "`hooks.{}` no es una lista en settings.json.",
                    spec.evento_cc
                )
            })?;
        lista.retain(|e| !es_hook_turtle(e)); // idempotencia: quitar la nuestra anterior
        let cmd = format!("\"{}\" hook {}", binario.display(), spec.sub);
        let mut entrada = serde_json::Map::new();
        if let Some(m) = spec.matcher {
            entrada.insert("matcher".to_string(), serde_json::json!(m));
        }
        entrada.insert(
            "hooks".to_string(),
            serde_json::json!([ { "type": "command", "command": cmd } ]),
        );
        lista.push(serde_json::Value::Object(entrada));
    }

    crear_padre(ruta)?;
    let pretty = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
    escribir(ruta, &(pretty + "\n"))
}

/// Quita los tres hooks de Turtle de settings.json, preservando el resto. Devuelve `true` si quitó
/// alguno. Limpia las listas de evento que queden vacías para no dejar `"PreToolUse": []` huérfanos.
fn quitar_hooks(ruta: &Path) -> Result<bool, String> {
    if !ruta.exists() {
        return Ok(false);
    }
    let txt = leer(ruta)?;
    if txt.trim().is_empty() {
        return Ok(false);
    }
    let mut raiz: serde_json::Value = serde_json::from_str(&txt)
        .map_err(|e| format!("settings.json inválido ({}): {e}", ruta.display()))?;
    let Some(hooks) = raiz.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return Ok(false);
    };
    let mut quitado = false;
    for spec in HOOKS {
        let Some(lista) = hooks.get_mut(spec.evento_cc).and_then(|p| p.as_array_mut()) else {
            continue;
        };
        let antes = lista.len();
        lista.retain(|e| !es_hook_turtle(e));
        if lista.len() != antes {
            quitado = true;
        }
        if lista.is_empty() {
            hooks.remove(spec.evento_cc);
        }
    }
    if quitado {
        let pretty = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
        escribir(ruta, &(pretty + "\n"))?;
    }
    Ok(quitado)
}

// ─── Rollback: revierte lo que el configurador agregó, usando los marcadores ───

/// Quita la configuración de Turtle de un cliente: la entrada MCP, el bloque del protocolo y los
/// subagentes generados. No toca nada que no lleve los marcadores de Turtle. No borra la memoria.
pub fn desinstalar(agente: Option<String>, config_override: Option<PathBuf>) -> Result<(), String> {
    let cliente = match agente {
        Some(id) => buscar_cliente(&id)?,
        None => elegir_interactivo()?,
    };
    let ruta = match config_override {
        Some(p) => p,
        None => ruta_config(cliente.id)
            .ok_or("no se pudo determinar la carpeta de configuración del usuario.")?,
    };

    let quitado_mcp = match cliente.formato {
        Formato::JsonMcpServers => quitar_mcp_json(&ruta)?,
        Formato::Toml => quitar_mcp_toml(&ruta)?,
        Formato::JsonMcpLocal => quitar_mcp_json_local(&ruta)?,
    };
    println!(
        "MCP de Turtle {} en {}.",
        if quitado_mcp { "quitado" } else { "no estaba" },
        ruta.display()
    );

    if let Some(md) = ruta_instrucciones(cliente.id) {
        let q = quitar_protocolo(&md)?;
        println!(
            "Protocolo de Turtle {} en {}.",
            if q { "quitado" } else { "no estaba" },
            md.display()
        );
    }
    if cliente.id == "claude-code" {
        let n = quitar_subagentes()?;
        println!("Subagentes de Turtle quitados: {n}.");
        if let Some(s) = ruta_settings(cliente.id) {
            let q = quitar_hooks(&s)?;
            println!(
                "Hooks de Turtle {} en {}.",
                if q { "quitados" } else { "no estaban" },
                s.display()
            );
        }
    }
    println!("Listo. La memoria sembrada en la base se conserva (borrala aparte si quieres).");
    Ok(())
}

/// Quita el bloque del protocolo (entre marcadores) preservando el resto del archivo.
fn quitar_protocolo(ruta: &Path) -> Result<bool, String> {
    if !ruta.exists() {
        return Ok(false);
    }
    let actual = leer(ruta)?;
    match (actual.find(MARCA_INI), actual.find(MARCA_FIN)) {
        (Some(i), Some(j)) if j > i => {
            let fin = j + MARCA_FIN.len();
            let cabeza = actual[..i].trim_end();
            let cola = actual[fin..].trim_start();
            let mut nuevo = cabeza.to_string();
            if !cola.is_empty() {
                if !nuevo.is_empty() {
                    nuevo.push_str("\n\n");
                }
                nuevo.push_str(cola);
            }
            nuevo.push('\n');
            escribir(ruta, &nuevo)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Quita la clave `turtle` de `mcpServers` en un JSON, preservando el resto.
fn quitar_mcp_json(ruta: &Path) -> Result<bool, String> {
    if !ruta.exists() {
        return Ok(false);
    }
    let txt = leer(ruta)?;
    if txt.trim().is_empty() {
        return Ok(false);
    }
    let mut raiz: serde_json::Value = serde_json::from_str(&txt)
        .map_err(|e| format!("JSON inválido en {}: {e}", ruta.display()))?;
    let quitado = raiz
        .get_mut("mcpServers")
        .and_then(|s| s.as_object_mut())
        .map(|o| o.remove("turtle").is_some())
        .unwrap_or(false);
    if quitado {
        let pretty = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
        escribir(ruta, &(pretty + "\n"))?;
    }
    Ok(quitado)
}

/// Quita la clave `turtle` del objeto `mcp` en un JSON (OpenCode), preservando el resto.
fn quitar_mcp_json_local(ruta: &Path) -> Result<bool, String> {
    if !ruta.exists() {
        return Ok(false);
    }
    let txt = leer(ruta)?;
    if txt.trim().is_empty() {
        return Ok(false);
    }
    let mut raiz: serde_json::Value = serde_json::from_str(&txt)
        .map_err(|e| format!("JSON inválido en {}: {e}", ruta.display()))?;
    let quitado = raiz
        .get_mut("mcp")
        .and_then(|s| s.as_object_mut())
        .map(|o| o.remove("turtle").is_some())
        .unwrap_or(false);
    if quitado {
        let pretty = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
        escribir(ruta, &(pretty + "\n"))?;
    }
    Ok(quitado)
}

/// Quita la tabla `[mcp_servers.turtle]` de un TOML (Codex), preservando el resto.
fn quitar_mcp_toml(ruta: &Path) -> Result<bool, String> {
    if !ruta.exists() {
        return Ok(false);
    }
    let txt = leer(ruta)?;
    let marca = "[mcp_servers.turtle]";
    let Some(ini) = txt.find(marca) else {
        return Ok(false);
    };
    // El bloque termina en el próximo encabezado `[` o al final del archivo.
    let resto = &txt[ini + marca.len()..];
    let fin = resto
        .find("\n[")
        .map(|p| ini + marca.len() + p + 1)
        .unwrap_or(txt.len());
    let cabeza = txt[..ini].trim_end();
    let cola = txt[fin..].trim_start();
    let mut nuevo = cabeza.to_string();
    if !cola.is_empty() {
        if !nuevo.is_empty() {
            nuevo.push_str("\n\n");
        }
        nuevo.push_str(cola);
    }
    if !nuevo.ends_with('\n') {
        nuevo.push('\n');
    }
    escribir(ruta, &nuevo)?;
    Ok(true)
}

/// Borra los subagentes generados por Turtle (`~/.claude/agents/*.md` con el marcador). Devuelve cuántos.
fn quitar_subagentes() -> Result<usize, String> {
    let home = directories::BaseDirs::new()
        .ok_or("no se pudo determinar la carpeta del usuario.")?
        .home_dir()
        .to_path_buf();
    let dir = home.join(".claude").join("agents");
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut n = 0;
    let rd =
        std::fs::read_dir(&dir).map_err(|e| format!("no se pudo leer {}: {e}", dir.display()))?;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("md") {
            let contenido = std::fs::read_to_string(&p).unwrap_or_default();
            if contenido.contains("TURTLE-AGENT") {
                std::fs::remove_file(&p)
                    .map_err(|e| format!("no se pudo borrar {}: {e}", p.display()))?;
                n += 1;
            }
        }
    }
    Ok(n)
}

/// Fusiona la entrada MCP de Turtle dentro de un JSON con `mcpServers`, preservando el resto.
fn aplicar_json(ruta: &Path, binario: &Path) -> Result<(), String> {
    let mut raiz: serde_json::Value = if ruta.exists() {
        let txt = leer(ruta)?;
        if txt.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&txt).map_err(|e| {
                format!(
                    "la config existente no es JSON válido ({}): {e}",
                    ruta.display()
                )
            })?
        }
    } else {
        serde_json::json!({})
    };

    let obj = raiz.as_object_mut().ok_or_else(|| {
        format!(
            "la config existente no es un objeto JSON ({}).",
            ruta.display()
        )
    })?;
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers.as_object_mut().ok_or_else(|| {
        format!(
            "`mcpServers` no es un objeto en la config existente ({}).",
            ruta.display()
        )
    })?;
    servers.insert(
        "turtle".to_string(),
        serde_json::json!({
            "command": binario.display().to_string(),
            "args": ["mcp"],
        }),
    );

    crear_padre(ruta)?;
    let pretty = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
    escribir(ruta, &(pretty + "\n"))
}

/// Fusiona la entrada MCP de Turtle dentro de un JSON con un objeto `mcp` (OpenCode), preservando
/// el resto. A diferencia de `mcpServers`, la entrada lleva `type: "local"`, el comando va como
/// array (`["<bin>", "mcp"]`) y se marca `enabled: true`. Ver https://opencode.ai/docs/mcp-servers/.
fn aplicar_json_local(ruta: &Path, binario: &Path) -> Result<(), String> {
    let mut raiz: serde_json::Value = if ruta.exists() {
        let txt = leer(ruta)?;
        if txt.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&txt).map_err(|e| {
                format!(
                    "la config existente no es JSON válido ({}): {e}",
                    ruta.display()
                )
            })?
        }
    } else {
        serde_json::json!({})
    };

    let obj = raiz.as_object_mut().ok_or_else(|| {
        format!(
            "la config existente no es un objeto JSON ({}).",
            ruta.display()
        )
    })?;
    // Deja el `$schema` de OpenCode si el archivo es nuevo (ayuda al autocompletado del editor).
    obj.entry("$schema")
        .or_insert_with(|| serde_json::json!("https://opencode.ai/config.json"));
    let servers = obj.entry("mcp").or_insert_with(|| serde_json::json!({}));
    let servers = servers.as_object_mut().ok_or_else(|| {
        format!(
            "`mcp` no es un objeto en la config existente ({}).",
            ruta.display()
        )
    })?;
    servers.insert(
        "turtle".to_string(),
        serde_json::json!({
            "type": "local",
            "command": [binario.display().to_string(), "mcp"],
            "enabled": true,
        }),
    );

    crear_padre(ruta)?;
    let pretty = serde_json::to_string_pretty(&raiz).map_err(|e| e.to_string())?;
    escribir(ruta, &(pretty + "\n"))
}

/// Agrega la tabla `[mcp_servers.turtle]` a un TOML (Codex), sin duplicarla.
fn aplicar_toml(ruta: &Path, binario: &Path) -> Result<(), String> {
    let actual = if ruta.exists() {
        leer(ruta)?
    } else {
        String::new()
    };
    if actual.contains("[mcp_servers.turtle]") {
        return Err(format!(
            "{} ya tiene [mcp_servers.turtle]; editalo a mano si quieres cambiar la ruta.",
            ruta.display()
        ));
    }
    let ruta_bin = binario.display().to_string().replace('\\', "\\\\");
    let bloque = format!("[mcp_servers.turtle]\ncommand = \"{ruta_bin}\"\nargs = [\"mcp\"]\n");

    crear_padre(ruta)?;
    let nuevo = if actual.trim().is_empty() {
        bloque
    } else {
        format!("{}\n\n{}", actual.trim_end(), bloque)
    };
    escribir(ruta, &nuevo)
}

fn crear_padre(ruta: &Path) -> Result<(), String> {
    if let Some(dir) = ruta.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("no se pudo crear la carpeta {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

fn leer(ruta: &Path) -> Result<String, String> {
    std::fs::read_to_string(ruta).map_err(|e| format!("no se pudo leer {}: {e}", ruta.display()))
}

fn escribir(ruta: &Path, contenido: &str) -> Result<(), String> {
    std::fs::write(ruta, contenido)
        .map_err(|e| format!("no se pudo escribir {}: {e}", ruta.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruta_temp(nombre: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("turtle_setup_{}_{nombre}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn dir_temp(nombre: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("turtle_qa_{}_{nombre}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn subagentes_se_escriben_con_override_y_respetan_ajenos() {
        let dir = dir_temp("agents");
        // Un archivo ajeno (sin el marcador) con el nombre de una persona NO debe pisarse.
        let ajeno = dir.join("donatello.md");
        std::fs::write(&ajeno, "subagente ajeno, no tocar").unwrap();

        let mut overrides = BTreeMap::new();
        overrides.insert("brunelleschi".to_string(), "haiku".to_string());
        let n = escribir_subagentes_en(&dir, &overrides).unwrap();

        // brunelleschi escrito con el modelo elegido y el marcador.
        let brunelleschi = std::fs::read_to_string(dir.join("brunelleschi.md")).unwrap();
        assert!(brunelleschi.contains("model: haiku"), "aplica el override");
        assert!(brunelleschi.contains("TURTLE-AGENT"));
        // El ajeno quedó intacto.
        assert_eq!(
            std::fs::read_to_string(&ajeno).unwrap(),
            "subagente ajeno, no tocar"
        );
        // Se escribieron todas las personas menos la bloqueada por el archivo ajeno.
        let generados = turtle_service::subagentes_claude(&overrides).len();
        assert_eq!(n, generados - 1, "donatello quedó afuera por ser ajeno");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reescribir_es_idempotente_y_actualiza_el_modelo() {
        let dir = dir_temp("agents2");
        // Primera escritura: modelos por defecto del bundle.
        escribir_subagentes_en(&dir, &BTreeMap::new()).unwrap();
        assert!(std::fs::read_to_string(dir.join("brunelleschi.md"))
            .unwrap()
            .contains("model:"));

        // Segunda: brunelleschi → sonnet; reescribe el propio (lleva marcador) sin duplicar.
        let mut overrides = BTreeMap::new();
        overrides.insert("brunelleschi".to_string(), "sonnet".to_string());
        escribir_subagentes_en(&dir, &overrides).unwrap();
        let despues = std::fs::read_to_string(dir.join("brunelleschi.md")).unwrap();
        assert!(despues.contains("model: sonnet"), "se actualizó el modelo");
        assert!(dir.join("brunelleschi.md").is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn json_fusiona_y_preserva_lo_existente() {
        let ruta = ruta_temp("a.json");
        std::fs::write(
            &ruta,
            r#"{"mcpServers":{"otro":{"command":"x"}},"tema":"oscuro"}"#,
        )
        .unwrap();
        aplicar_json(&ruta, Path::new("/usr/local/bin/turtle")).unwrap();

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        assert_eq!(
            v["mcpServers"]["turtle"]["command"],
            "/usr/local/bin/turtle"
        );
        assert_eq!(v["mcpServers"]["turtle"]["args"][0], "mcp");
        assert_eq!(v["mcpServers"]["otro"]["command"], "x");
        assert_eq!(v["tema"], "oscuro");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn json_crea_archivo_si_no_existe() {
        let ruta = ruta_temp("b.json");
        aplicar_json(&ruta, Path::new("/bin/turtle")).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["turtle"]["args"][0], "mcp");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn toml_agrega_bloque_y_no_duplica() {
        let ruta = ruta_temp("c.toml");
        std::fs::write(&ruta, "[otra]\nx = 1\n").unwrap();
        aplicar_toml(&ruta, Path::new("/bin/turtle")).unwrap();
        let txt = std::fs::read_to_string(&ruta).unwrap();
        assert!(txt.contains("[mcp_servers.turtle]"));
        assert!(txt.contains("args = [\"mcp\"]"));
        assert!(txt.contains("[otra]"));
        // La segunda vez no debe duplicar: error claro.
        assert!(aplicar_toml(&ruta, Path::new("/bin/turtle")).is_err());
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn agente_desconocido_es_error_con_opciones() {
        assert!(buscar_cliente("cursor").is_ok());
        let err = buscar_cliente("inexistente").unwrap_err();
        assert!(err.contains("cursor"), "{err}");
    }

    #[test]
    fn opencode_json_local_fusiona_y_usa_el_esquema_correcto() {
        let ruta = ruta_temp("opencode.json");
        // Config existente con otro server bajo `mcp` y una clave ajena: ambos deben preservarse.
        std::fs::write(
            &ruta,
            r#"{"mcp":{"otro":{"type":"local","command":["x"]}},"theme":"dark"}"#,
        )
        .unwrap();
        aplicar_json_local(&ruta, Path::new("/usr/local/bin/turtle")).unwrap();

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        // Entrada de Turtle con el shape de OpenCode: `type` local, `command` array, `enabled` true.
        assert_eq!(v["mcp"]["turtle"]["type"], "local");
        assert_eq!(v["mcp"]["turtle"]["command"][0], "/usr/local/bin/turtle");
        assert_eq!(v["mcp"]["turtle"]["command"][1], "mcp");
        assert_eq!(v["mcp"]["turtle"]["enabled"], true);
        // Preserva el otro server y la clave ajena.
        assert_eq!(v["mcp"]["otro"]["command"][0], "x");
        assert_eq!(v["theme"], "dark");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn opencode_json_local_crea_archivo_con_schema() {
        let ruta = ruta_temp("opencode_nuevo.json");
        let _ = std::fs::remove_file(&ruta);
        aplicar_json_local(&ruta, Path::new("/bin/turtle")).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        // Archivo nuevo: lleva el `$schema` de OpenCode y la entrada de Turtle.
        assert_eq!(v["$schema"], "https://opencode.ai/config.json");
        assert_eq!(v["mcp"]["turtle"]["type"], "local");
        assert_eq!(v["mcp"]["turtle"]["command"][1], "mcp");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn opencode_ruta_config_e_instrucciones_son_las_oficiales() {
        // Config global: ~/.config/opencode/opencode.json (literal en todos los SO).
        let cfg = ruta_config("opencode").unwrap();
        assert!(cfg.ends_with("opencode.json"));
        assert!(cfg
            .to_string_lossy()
            .replace('\\', "/")
            .contains(".config/opencode/"));
        // Reglas globales: ~/.config/opencode/AGENTS.md.
        let md = ruta_instrucciones("opencode").unwrap();
        assert!(md.ends_with("AGENTS.md"));
        assert!(md
            .to_string_lossy()
            .replace('\\', "/")
            .contains(".config/opencode/"));
    }

    #[test]
    fn protocolo_opencode_es_core_sin_delegacion_de_subagentes() {
        // OpenCode (single-agent como Codex): recibe el núcleo del MCP, NO la sección Claude-específica.
        let ruta = ruta_temp("AGENTS_opencode.md");
        inyectar_protocolo(&ruta, "opencode").unwrap();
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(t.contains("session_start"), "lleva el núcleo del MCP");
        assert!(
            !t.contains("Delegación a sub-agentes"),
            "OpenCode no lleva la sección de subagentes"
        );
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn rollback_quita_mcp_json_local_preservando_otros() {
        let ruta = ruta_temp("rb_opencode.json");
        aplicar_json_local(&ruta, Path::new("/bin/turtle")).unwrap();
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        v["mcp"]["otro"] = serde_json::json!({"type": "local", "command": ["x"]});
        std::fs::write(&ruta, serde_json::to_string(&v).unwrap()).unwrap();

        assert!(quitar_mcp_json_local(&ruta).unwrap());
        let v2: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        assert!(v2["mcp"].get("turtle").is_none(), "quitó turtle");
        assert_eq!(v2["mcp"]["otro"]["command"][0], "x", "preservó el otro");
        // Quitar de nuevo: ya no hay nada de Turtle.
        assert!(!quitar_mcp_json_local(&ruta).unwrap());
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn inyecta_protocolo_preserva_y_es_idempotente() {
        let ruta = ruta_temp("CLAUDE.md");
        std::fs::write(&ruta, "# Mi CLAUDE\n\ncontenido propio\n").unwrap();

        inyectar_protocolo(&ruta, "claude-code").unwrap();
        let t1 = std::fs::read_to_string(&ruta).unwrap();
        assert!(t1.contains("contenido propio"), "preserva lo existente");
        assert!(t1.contains("session_start"), "inyecta el protocolo");
        assert_eq!(t1.matches(MARCA_INI).count(), 1);
        assert_eq!(t1.matches(MARCA_FIN).count(), 1);

        // Re-ejecutar reemplaza el bloque sin duplicar.
        inyectar_protocolo(&ruta, "claude-code").unwrap();
        let t2 = std::fs::read_to_string(&ruta).unwrap();
        assert_eq!(t2.matches(MARCA_INI).count(), 1, "no duplica el bloque");
        assert!(t2.contains("contenido propio"));
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn inyecta_protocolo_crea_archivo_si_no_existe() {
        let ruta = ruta_temp("CLAUDE_nuevo.md");
        let _ = std::fs::remove_file(&ruta);
        inyectar_protocolo(&ruta, "claude-code").unwrap();
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(t.contains(MARCA_INI) && t.contains("session_start"));
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn protocolo_codex_es_core_sin_delegacion_de_subagentes() {
        // Codex usa ~/.codex/AGENTS.md como instrucciones globales.
        assert!(ruta_instrucciones("codex").unwrap().ends_with("AGENTS.md"));

        // Codex (single-agent): recibe el núcleo del MCP, NO la sección Claude-específica.
        let ruta = ruta_temp("AGENTS.md");
        inyectar_protocolo(&ruta, "codex").unwrap();
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(t.contains("session_start"), "lleva el núcleo del MCP");
        assert!(
            !t.contains("Delegación a sub-agentes"),
            "Codex no lleva la sección de subagentes"
        );
        assert!(!t.contains("tool Task"), "Codex no tiene el tool Task");
        let _ = std::fs::remove_file(&ruta);

        // Claude Code sí lleva la delegación.
        let ruta2 = ruta_temp("CLAUDE_deleg.md");
        inyectar_protocolo(&ruta2, "claude-code").unwrap();
        let t2 = std::fs::read_to_string(&ruta2).unwrap();
        assert!(t2.contains("Delegación a sub-agentes"));
        let _ = std::fs::remove_file(&ruta2);
    }

    #[test]
    fn gemini_apunta_a_settings_y_a_gemini_md() {
        // El MCP va a ~/.gemini/settings.json (objeto mcpServers, formato JSON).
        let cfg = ruta_config("gemini").unwrap();
        assert!(
            cfg.ends_with("settings.json"),
            "config de Gemini debe ser settings.json, fue: {}",
            cfg.display()
        );
        assert!(
            cfg.parent().unwrap().ends_with(".gemini"),
            "settings.json de Gemini va en ~/.gemini"
        );
        assert!(matches!(
            buscar_cliente("gemini").unwrap().formato,
            Formato::JsonMcpServers
        ));
        // Las instrucciones globales van a ~/.gemini/GEMINI.md.
        assert!(ruta_instrucciones("gemini").unwrap().ends_with("GEMINI.md"));
    }

    #[test]
    fn protocolo_gemini_es_core_sin_delegacion_de_subagentes() {
        // Gemini CLI usa ~/.gemini/GEMINI.md como contexto global.
        assert!(ruta_instrucciones("gemini").unwrap().ends_with("GEMINI.md"));

        // Gemini (single-agent, como Codex/OpenCode): recibe el núcleo del MCP, NO la sección
        // Claude-específica de delegación a subagentes.
        let ruta = ruta_temp("GEMINI.md");
        inyectar_protocolo(&ruta, "gemini").unwrap();
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(t.contains("session_start"), "lleva el núcleo del MCP");
        assert!(
            !t.contains("Delegación a sub-agentes"),
            "Gemini no lleva la sección de subagentes"
        );
        assert!(!t.contains("tool Task"), "Gemini no tiene el tool Task");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn inyecta_protocolo_gemini_preserva_y_es_idempotente() {
        let ruta = ruta_temp("GEMINI_idem.md");
        std::fs::write(&ruta, "# Mi GEMINI\n\ncontenido propio del usuario\n").unwrap();

        inyectar_protocolo(&ruta, "gemini").unwrap();
        let t1 = std::fs::read_to_string(&ruta).unwrap();
        assert!(t1.contains("contenido propio del usuario"), "preserva lo ajeno");
        assert!(t1.contains("session_start"), "inyecta el protocolo");
        assert_eq!(t1.matches(MARCA_INI).count(), 1);
        assert_eq!(t1.matches(MARCA_FIN).count(), 1);

        // Correr dos veces no duplica el bloque.
        inyectar_protocolo(&ruta, "gemini").unwrap();
        let t2 = std::fs::read_to_string(&ruta).unwrap();
        assert_eq!(t2.matches(MARCA_INI).count(), 1, "no duplica el bloque");
        assert!(t2.contains("contenido propio del usuario"));
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn rollback_quita_protocolo_gemini_preservando_lo_demas() {
        let ruta = ruta_temp("GEMINI_rb.md");
        std::fs::write(&ruta, "# Mío\n\ntexto propio del usuario\n").unwrap();
        inyectar_protocolo(&ruta, "gemini").unwrap();
        assert!(quitar_protocolo(&ruta).unwrap());
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(!t.contains(MARCA_INI), "quitó el bloque");
        assert!(t.contains("texto propio del usuario"), "preservó lo demás");
        // Quitar de nuevo: ya no hay bloque.
        assert!(!quitar_protocolo(&ruta).unwrap());
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn rollback_quita_protocolo_preservando_lo_demas() {
        let ruta = ruta_temp("CLAUDE_rb.md");
        std::fs::write(&ruta, "# Mío\n\ntexto propio\n").unwrap();
        inyectar_protocolo(&ruta, "claude-code").unwrap();
        assert!(quitar_protocolo(&ruta).unwrap());
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(!t.contains(MARCA_INI), "quitó el bloque");
        assert!(t.contains("texto propio"), "preservó lo demás");
        // Quitar de nuevo: ya no hay bloque.
        assert!(!quitar_protocolo(&ruta).unwrap());
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn rollback_quita_mcp_json_preservando_otros() {
        let ruta = ruta_temp("rb.json");
        aplicar_json(&ruta, Path::new("/bin/turtle")).unwrap();
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        v["mcpServers"]["otro"] = serde_json::json!({"command": "x"});
        std::fs::write(&ruta, serde_json::to_string(&v).unwrap()).unwrap();

        assert!(quitar_mcp_json(&ruta).unwrap());
        let v2: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        assert!(v2["mcpServers"].get("turtle").is_none(), "quitó turtle");
        assert_eq!(v2["mcpServers"]["otro"]["command"], "x", "preservó el otro");
        assert!(!quitar_mcp_json(&ruta).unwrap());
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn rollback_quita_mcp_toml_preservando_otros() {
        let ruta = ruta_temp("rb.toml");
        std::fs::write(&ruta, "[otra]\nx = 1\n").unwrap();
        aplicar_toml(&ruta, Path::new("/bin/turtle")).unwrap();
        assert!(quitar_mcp_toml(&ruta).unwrap());
        let t = std::fs::read_to_string(&ruta).unwrap();
        assert!(!t.contains("[mcp_servers.turtle]"), "quitó turtle");
        assert!(t.contains("[otra]"), "preservó lo demás");
        let _ = std::fs::remove_file(&ruta);
    }

    /// Cuenta cuántas entradas de hook bajo `hooks.<evento>` son de Turtle.
    fn cuenta_hooks_turtle(v: &serde_json::Value, evento: &str) -> usize {
        v["hooks"][evento]
            .as_array()
            .map(|a| a.iter().filter(|e| es_hook_turtle(e)).count())
            .unwrap_or(0)
    }

    #[test]
    fn registra_los_tres_hooks_con_su_esquema() {
        let ruta = ruta_temp("hooks_tres.json");
        let _ = std::fs::remove_file(&ruta);
        registrar_hooks(&ruta, Path::new("/bin/turtle")).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();

        // Los tres eventos, uno cada uno, con el subcomando correcto.
        for spec in HOOKS {
            assert_eq!(
                cuenta_hooks_turtle(&v, spec.evento_cc),
                1,
                "{} debe tener exactamente un hook de Turtle",
                spec.evento_cc
            );
            let cmd = v["hooks"][spec.evento_cc][0]["hooks"][0]["command"]
                .as_str()
                .unwrap();
            assert!(
                cmd.contains(&format!("hook {}", spec.sub)),
                "{} debe invocar `hook {}`, fue: {cmd}",
                spec.evento_cc,
                spec.sub
            );
        }

        // PreToolUse y SessionStart llevan matcher; UserPromptSubmit NO (la doc lo ignora).
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "*");
        assert_eq!(v["hooks"]["SessionStart"][0]["matcher"], "startup|resume");
        assert!(
            v["hooks"]["UserPromptSubmit"][0].get("matcher").is_none(),
            "UserPromptSubmit no debe llevar matcher"
        );
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn registrar_hooks_es_idempotente() {
        let ruta = ruta_temp("hooks_idem.json");
        let _ = std::fs::remove_file(&ruta);
        registrar_hooks(&ruta, Path::new("/bin/turtle")).unwrap();
        registrar_hooks(&ruta, Path::new("/bin/turtle")).unwrap();
        registrar_hooks(&ruta, Path::new("/bin/turtle")).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        for spec in HOOKS {
            assert_eq!(
                cuenta_hooks_turtle(&v, spec.evento_cc),
                1,
                "re-registrar no debe duplicar {}",
                spec.evento_cc
            );
        }
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn registrar_hooks_preserva_hooks_ajenos() {
        let ruta = ruta_temp("hooks_ajenos.json");
        // settings con un hook ajeno en PreToolUse y otro tope de archivo.
        std::fs::write(
            &ruta,
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"otra-cosa"}]}]},"tema":"oscuro"}"#,
        )
        .unwrap();
        registrar_hooks(&ruta, Path::new("/bin/turtle")).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        // El ajeno sigue ahí, y se agregó el de Turtle: 2 entradas en PreToolUse.
        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2, "ajeno + Turtle");
        assert!(pre.iter().any(|e| !es_hook_turtle(e)), "preserva el ajeno");
        assert_eq!(cuenta_hooks_turtle(&v, "PreToolUse"), 1);
        assert_eq!(v["tema"], "oscuro", "preserva el resto del archivo");
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn quitar_hooks_quita_los_tres_y_es_idempotente() {
        let ruta = ruta_temp("hooks_quitar.json");
        std::fs::write(
            &ruta,
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"otra-cosa"}]}]}}"#,
        )
        .unwrap();
        registrar_hooks(&ruta, Path::new("/bin/turtle")).unwrap();
        assert!(quitar_hooks(&ruta).unwrap(), "primer quitar elimina algo");

        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        // No queda ningún hook de Turtle, y los eventos vacíos se limpiaron.
        for spec in HOOKS {
            assert_eq!(cuenta_hooks_turtle(&v, spec.evento_cc), 0);
        }
        assert!(
            v["hooks"].get("SessionStart").is_none(),
            "SessionStart vacío se limpia"
        );
        assert!(
            v["hooks"].get("UserPromptSubmit").is_none(),
            "UserPromptSubmit vacío se limpia"
        );
        // El hook ajeno de PreToolUse sobrevive (la lista no quedó vacía, no se limpió).
        assert_eq!(v["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);

        // Quitar de nuevo: ya no hay nada de Turtle.
        assert!(
            !quitar_hooks(&ruta).unwrap(),
            "segundo quitar no cambia nada"
        );
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn quitar_hooks_reconoce_formato_viejo_solo_activity() {
        // Una instalación previa registraba solo `hook activity`: debe desinstalarse igual.
        let ruta = ruta_temp("hooks_viejo.json");
        std::fs::write(
            &ruta,
            r#"{"hooks":{"PreToolUse":[{"matcher":"*","hooks":[{"type":"command","command":"\"/bin/turtle\" hook activity"}]}]}}"#,
        )
        .unwrap();
        assert!(quitar_hooks(&ruta).unwrap(), "debe quitar el formato viejo");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&ruta).unwrap()).unwrap();
        assert!(
            v["hooks"].get("PreToolUse").is_none(),
            "lista vacía se limpia"
        );
        let _ = std::fs::remove_file(&ruta);
    }
}
