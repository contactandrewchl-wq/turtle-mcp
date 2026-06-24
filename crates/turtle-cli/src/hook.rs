//! `turtle hook <evento>` — adaptadores de los hooks de Claude Code (Capa 1 de la integración).
//!
//! Claude Code invoca estos comandos en eventos de sesión y les pasa un JSON por stdin (con
//! `cwd`, `prompt`, …). El comando imprime, en el formato de hook, **contexto para inyectar**:
//! memorias relevantes del proyecto. Así Turtle queda "presente" sin que el agente lo pida.
//!
//! El formato exacto del hook puede variar entre versiones de Claude Code. Los subcomandos
//! también se pueden cablear a mano en `~/.claude/settings.json` bajo `hooks`.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use turtle_core::event::EventKind;
use turtle_service::MemoryService;

/// Presupuesto de tokens del contexto que inyecta un hook.
const PRESUPUESTO_HOOK: usize = 1_500;

/// Tope de bytes que se leen del JSON de stdin que manda Claude Code. Aunque la entrada es local,
/// no está acotada por contrato: un payload patológico (p. ej. un `prompt` enorme) haría que un
/// `read_to_string` sin límite consuma memoria sin techo en un hook que corre por cada tool-call.
/// 4 MiB cubre con holgura cualquier prompt/transcripts real y corta el caso degenerado
/// (secure-by-default: límites por defecto). El JSON que excede el tope se descarta silenciosamente.
const MAX_STDIN_BYTES: u64 = 4 * 1024 * 1024;

/// Punto de entrada del subcomando `hook`.
pub fn ejecutar(evento: &str, servicio: &MemoryService) -> Result<(), String> {
    let entrada = leer_stdin_json();
    let cwd = entrada
        .as_ref()
        .and_then(cwd_de)
        .or_else(|| std::env::current_dir().ok());
    // Proyecto: igual que el resto de la CLI, `$TURTLE_PROJECT` tiene prioridad; si no, se deriva del
    // cwd que pasó Claude Code (o el actual). Así el hook y los comandos resuelven el mismo proyecto.
    let proyecto = std::env::var("TURTLE_PROJECT")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| cwd.as_deref().map(crate::proyecto_en))
        .unwrap_or_else(|| "default".to_string());

    // Feed de actividad (hook PreToolUse): registra la herramienta/dispatch; sin contexto.
    if evento == "activity" {
        return registrar_actividad(servicio, entrada.as_ref(), &proyecto);
    }

    let (nombre_evento, contexto) = match evento {
        "session-start" => {
            let ctx = contexto_session_start(servicio, &proyecto)?;
            // Auto-mantenimiento y auto-sync: best-effort, throttled, silencioso. Corre DESPUÉS de
            // calcular el contexto a inyectar (que es el contrato visible del hook) y NO falla el
            // hook si algo sale mal — así el usuario nunca tiene que correr `escalonar`/`podar`/`sync`.
            auto_mantenimiento(servicio, &proyecto);
            ("SessionStart", ctx)
        }
        "prompt-submit" | "user-prompt-submit" => {
            let prompt = entrada
                .as_ref()
                .and_then(|v| v.get("prompt"))
                .and_then(|p| p.as_str())
                .unwrap_or("");
            // Cierra el círculo automático (paridad funcional): registra el prompt para que un
            // memory_save posterior sin prompt explícito lo adjunte solo. Best-effort: si falla la
            // BD, no rompe el hook (el contrato visible es el contexto a inyectar).
            if !prompt.trim().is_empty() {
                let sesion = entrada
                    .as_ref()
                    .and_then(|v| v.get("session_id"))
                    .and_then(|s| s.as_str());
                let _ = servicio.record_prompt(&proyecto, sesion, prompt);
            }
            let mut ctx = contexto_prompt(servicio, &proyecto, prompt)?.unwrap_or_default();
            // Nudge de guardado (paridad funcional): si hace mucho que no se guarda, recordarlo.
            if let Some(n) = nudge_guardado(servicio, &proyecto)? {
                if !ctx.is_empty() {
                    ctx.push('\n');
                }
                ctx.push_str(&n);
            }
            (
                "UserPromptSubmit",
                if ctx.is_empty() { None } else { Some(ctx) },
            )
        }
        otro => {
            return Err(format!(
                "evento de hook desconocido: {otro}. Use session-start o prompt-submit."
            ))
        }
    };

    if let Some(texto) = contexto {
        println!("{}", envolver(nombre_evento, &texto));
    }
    Ok(())
}

/// `true` si el feed de actividad está desactivado por entorno (`$TURTLE_NO_ACTIVITY` con un valor
/// no vacío). Permite al usuario apagar el hook PreToolUse y dejar su costo en el piso del proceso,
/// sin tocar la base. Se consulta desde `main` antes de abrir la base.
pub fn actividad_desactivada() -> bool {
    std::env::var_os("TURTLE_NO_ACTIVITY").is_some_and(|v| !v.is_empty())
}

/// Registra en el feed de actividad lo que claude hizo (hook PreToolUse): la herramienta usada,
/// o el dispatch a un subagente con su modelo. Silencioso (no inyecta contexto).
fn registrar_actividad(
    servicio: &MemoryService,
    entrada: Option<&serde_json::Value>,
    proyecto: &str,
) -> Result<(), String> {
    let Some(v) = entrada else {
        return Ok(());
    };
    let agente = v.get("agent_type").and_then(|x| x.as_str());
    let tool = v.get("tool_name").and_then(|x| x.as_str());
    let (kind, resumen, aviso) = match tool {
        Some("Task") => {
            let sub_opt = v
                .get("tool_input")
                .and_then(|t| t.get("subagent_type"))
                .and_then(|x| x.as_str());
            // Modelo EFECTIVO: el override que el usuario eligió con `turtle modelos` gana sobre el
            // modelo por defecto del bundle. Así el feed (y el aviso) muestran el modelo real en uso.
            let modelo = sub_opt
                .and_then(|s| {
                    crate::modelos::leer_overrides()
                        .get(s)
                        .cloned()
                        .or_else(|| turtle_service::modelo_persona(s))
                })
                .unwrap_or_else(|| "modelo?".to_string());
            let sub = sub_opt.unwrap_or("agente");
            // El aviso visible solo cuando hay un subagente real (evita "→ agente · modelo?" si el
            // payload del hook viene degenerado). El feed registra el dispatch igual.
            let aviso = sub_opt.map(|s| format!("🐢 delega → {s} · {modelo}"));
            (
                EventKind::AgentDispatched,
                format!("→ {sub} ({modelo})"),
                aviso,
            )
        }
        Some(t) => (EventKind::ToolUsed, t.to_string(), None),
        None => return Ok(()),
    };
    servicio
        .record_activity(proyecto, agente, kind, Some(&resumen))
        .map_err(|e| e.to_string())?;
    // Aviso visible al usuario en la terminal de Claude Code al delegar (canal `systemMessage` del
    // hook). Solo en dispatches a subagente; el resto de tool-calls no imprime nada (hot path).
    if let Some(aviso) = aviso {
        println!(
            "{}",
            serde_json::json!({
                "hookSpecificOutput": { "hookEventName": "PreToolUse" },
                "systemMessage": aviso,
            })
        );
    }
    Ok(())
}

/// Contexto al iniciar una sesión: memorias fijadas + cambios desde la última sesión + recientes
/// del proyecto, como deltas para no reinyectar todo (RF-TOK-04).
fn contexto_session_start(
    servicio: &MemoryService,
    proyecto: &str,
) -> Result<Option<String>, String> {
    let desde = servicio
        .previous_session_start(proyecto)
        .map_err(|e| e.to_string())?;
    let recientes = servicio
        .session_deltas(proyecto, "", PRESUPUESTO_HOOK, desde)
        .map_err(|e| e.to_string())?;
    let activas = servicio
        .active_skills(proyecto)
        .map_err(|e| e.to_string())?;
    // RF-SES-04: si hay trabajo en curso (checkpoint), recuperarlo al reanudar.
    let checkpoint = servicio
        .latest_checkpoint(proyecto)
        .map_err(|e| e.to_string())?;
    if recientes.rows.is_empty() && activas.is_empty() && checkpoint.is_none() {
        return Ok(None);
    }
    let mut texto = format!("Memoria persistente de Turtle — proyecto «{proyecto}»:\n");
    if let Some(c) = &checkpoint {
        texto.push_str(&format!("Trabajo en curso (checkpoint): {}\n", c.content));
    }
    for r in &recientes.rows {
        texto.push_str(&linea_memoria(r));
    }
    if !activas.is_empty() {
        texto.push_str("Skills de comportamiento activas (aplicalas):\n");
        for s in &activas {
            let cuando = s
                .when_to_use
                .as_deref()
                .map(|c| format!(" — {c}"))
                .unwrap_or_default();
            texto.push_str(&format!(
                "- [{}] {}{}  (id:{})\n",
                s.intensity.as_str(),
                s.name,
                cuando,
                s.id
            ));
        }
    }
    texto.push_str(
        "Usá las herramientas MCP de Turtle (memory_search, memory_get, skill_get) para más.",
    );
    Ok(Some(texto))
}

/// Contexto al enviar un prompt: memorias del proyecto relevantes a lo que se pidió.
fn contexto_prompt(
    servicio: &MemoryService,
    proyecto: &str,
    prompt: &str,
) -> Result<Option<String>, String> {
    if prompt.trim().is_empty() {
        return Ok(None);
    }
    // `session_context` busca por relevancia con OR de las palabras del prompt (no AND estricto),
    // que es lo apropiado para un texto en lenguaje natural.
    let res = servicio
        .session_context(proyecto, prompt, PRESUPUESTO_HOOK)
        .map_err(|e| e.to_string())?;
    if res.rows.is_empty() {
        return Ok(None);
    }
    let mut texto = format!("Memorias de Turtle relevantes a tu pedido (proyecto «{proyecto}»):\n");
    for r in &res.rows {
        texto.push_str(&linea_memoria(r));
    }
    texto.push_str("Recuperá el contenido con memory_get(id) si lo necesitás.");
    Ok(Some(texto))
}

fn linea_memoria(r: &turtle_core::memory::MemoryIndexRow) -> String {
    let resumen = r
        .summary
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" — {s}"))
        .unwrap_or_default();
    format!(
        "- [{}] {}{}  (id:{})\n",
        r.kind.as_str(),
        r.title,
        resumen,
        r.id
    )
}

/// Envuelve el contexto en el formato de salida de hook de Claude Code.
fn envolver(evento: &str, contexto: &str) -> serde_json::Value {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": evento,
            "additionalContext": contexto,
        }
    })
}

fn leer_stdin_json() -> Option<serde_json::Value> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }
    parsear_json_acotado(stdin.lock())
}

/// Lee un JSON desde `r` con la lectura acotada a `MAX_STDIN_BYTES` y lo parsea. Función pura
/// (toma cualquier `Read`) para poder probar el tope sin tocar el stdin real. Un payload que
/// supera el tope se trunca: el JSON parcial no parsea y se devuelve `None` (degradación elegante).
fn parsear_json_acotado<R: Read>(r: R) -> Option<serde_json::Value> {
    let mut s = String::new();
    r.take(MAX_STDIN_BYTES).read_to_string(&mut s).ok()?;
    serde_json::from_str(s.trim()).ok()
}

fn cwd_de(v: &serde_json::Value) -> Option<PathBuf> {
    v.get("cwd").and_then(|x| x.as_str()).map(PathBuf::from)
}

// ─── Nudge de guardado (paridad funcional): recuerda guardar si hace rato que no se guarda ───

/// Minutos sin guardar a partir de los cuales el hook sugiere guardar.
const NUDGE_THRESHOLD_MS: i64 = 20 * 60 * 1000;
/// Tiempo mínimo entre nudges, para no repetir el aviso en cada prompt.
const NUDGE_COOLDOWN_MS: i64 = 15 * 60 * 1000;

/// Decide el texto del nudge de guardado. Puro y testeable: depende solo de marcas de tiempo. Solo
/// nudgea si hubo un guardado previo (no molesta en proyectos recién empezados) que ya superó el
/// umbral, y si pasó el enfriamiento desde el último nudge.
fn texto_nudge(last_save: Option<i64>, last_nudge: Option<i64>, ahora: i64) -> Option<String> {
    let last_save = last_save?;
    if ahora - last_save < NUDGE_THRESHOLD_MS {
        return None;
    }
    if let Some(ln) = last_nudge {
        if ahora - ln < NUDGE_COOLDOWN_MS {
            return None;
        }
    }
    let min = (ahora - last_save) / 60_000;
    Some(format!(
        "💾 Hace ~{min} min que no guardas nada en este proyecto. Si decidiste o aprendiste algo no obvio, usá memory_save."
    ))
}

/// Aplica el nudge: lee/escribe el enfriamiento en `<config>/turtle/nudge_<proyecto>.txt` y devuelve
/// el texto si corresponde mostrarlo. Silencioso ante errores de E/S (no debe romper el prompt).
fn nudge_guardado(servicio: &MemoryService, proyecto: &str) -> Result<Option<String>, String> {
    let ahora = ahora_ms();
    let last_save = servicio
        .last_memory_save_time(proyecto)
        .map_err(|e| e.to_string())?;
    let ruta = ruta_nudge(proyecto);
    let last_nudge = ruta
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<i64>().ok());
    let nudge = texto_nudge(last_save, last_nudge, ahora);
    if nudge.is_some() {
        if let Some(p) = &ruta {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, ahora.to_string());
        }
    }
    Ok(nudge)
}

/// Ruta del archivo de enfriamiento del nudge por proyecto. El nombre se sanea para que cualquier
/// rótulo de proyecto sea un nombre de archivo válido.
fn ruta_nudge(proyecto: &str) -> Option<PathBuf> {
    let seguro: String = proyecto
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let b = directories::BaseDirs::new()?;
    Some(
        b.config_dir()
            .join("turtle")
            .join(format!("nudge_{seguro}.txt")),
    )
}

fn ahora_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ─── Auto-mantenimiento: escalonar + podar (y sync opcional) sin que el usuario corra comandos ───

/// Intervalo mínimo entre corridas de auto-mantenimiento por proyecto: ~1 vez por día. El
/// mantenimiento (escalonar por antigüedad, podar efímeras) no es urgente; correrlo en cada
/// session-start sería desperdicio. El throttle se persiste por proyecto en `<config>/turtle/`.
const MANTENIMIENTO_INTERVALO_MS: i64 = 24 * 60 * 60 * 1000;

/// Defaults de escalonamiento/poda, en paridad con los de la CLI (`turtle escalonar`/`podar`):
/// caliente→tibio a los 14 días sin acceso, tibio→frío a los 60, poda de efímeras a los 30.
const DIAS_TIBIO: i64 = 14;
const DIAS_FRIO: i64 = 60;
const DIAS_PODA_EFIMERAS: i64 = 30;

/// Decide si corresponde correr el mantenimiento ahora. Puro y testeable: depende solo de la marca
/// de la última corrida. Si nunca corrió (`None`), corre; si pasó el intervalo, corre.
fn corresponde_mantenimiento(ultima_corrida: Option<i64>, ahora: i64) -> bool {
    match ultima_corrida {
        None => true,
        Some(t) => ahora - t >= MANTENIMIENTO_INTERVALO_MS,
    }
}

/// Ruta del archivo de throttle del mantenimiento por proyecto. Mismo patrón saneado que el nudge.
fn ruta_mantenimiento(proyecto: &str) -> Option<PathBuf> {
    let seguro: String = proyecto
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let b = directories::BaseDirs::new()?;
    Some(
        b.config_dir()
            .join("turtle")
            .join(format!("mantenimiento_{seguro}.txt")),
    )
}

/// Corre el auto-mantenimiento si toca, persistiendo la marca de la última corrida. Best-effort: no
/// devuelve error ni interrumpe el hook si algo falla (E/S del throttle o la propia BD), porque el
/// session-start debe inyectar contexto pase lo que pase. Tras escalonar/podar, intenta el auto-sync
/// opcional. Esto reemplaza la necesidad de que el usuario corra `escalonar`/`podar` a mano.
fn auto_mantenimiento(servicio: &MemoryService, proyecto: &str) {
    let ruta = ruta_mantenimiento(proyecto);
    let ultima = ruta
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<i64>().ok());
    if !corresponde_mantenimiento(ultima, ahora_ms()) {
        return;
    }
    // Escribir la marca ANTES de trabajar evita que dos session-start casi simultáneos corran el
    // mantenimiento dos veces; si el trabajo falla, se reintenta en la próxima ventana diaria.
    if let Some(p) = &ruta {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, ahora_ms().to_string());
    }
    let _ = servicio.escalonar(proyecto, DIAS_TIBIO, DIAS_FRIO);
    let _ = servicio.podar_efimeras(proyecto, DIAS_PODA_EFIMERAS);
    auto_sync(servicio, proyecto);
}

/// Auto-sync OPCIONAL: si `$TURTLE_SYNC_DIR` apunta a un directorio, exporta los fragmentos del
/// proyecto ahí (un archivo JSON por memoria, versionable en git). Best-effort y sin tocar git: solo
/// escribe los fragmentos; commit/push quedan a cargo del usuario o de su tooling. Si la variable no
/// está seteada o está vacía, no hace nada (no se asume un destino por defecto).
fn auto_sync(servicio: &MemoryService, proyecto: &str) {
    let Some(dir) = std::env::var_os("TURTLE_SYNC_DIR").filter(|v| !v.is_empty()) else {
        return;
    };
    let _ = crate::sync::exportar_fragmentos(servicio, Some(proyecto), std::path::Path::new(&dir));
}

#[cfg(test)]
mod tests {
    use super::*;
    use turtle_core::memory::{MemoryKind, NewMemory};
    use turtle_data::Db;

    fn servicio() -> MemoryService {
        let s = MemoryService::new(Db::open_in_memory().unwrap());
        s.save(&NewMemory {
            summary: Some("MCP con rmcp".into()),
            ..NewMemory::nueva(
                "demo".into(),
                MemoryKind::Decision,
                "Usar rmcp para el MCP".into(),
                "El servidor MCP usa rmcp sobre stdio.".into(),
            )
        })
        .unwrap();
        s
    }

    #[test]
    fn session_start_inyecta_memorias_del_proyecto() {
        let s = servicio();
        let ctx = contexto_session_start(&s, "demo").unwrap().unwrap();
        assert!(ctx.contains("Usar rmcp para el MCP"));
        assert!(ctx.contains("proyecto «demo»"));
        // Un proyecto sin memorias no inyecta nada.
        assert!(contexto_session_start(&s, "vacio").unwrap().is_none());
    }

    #[test]
    fn prompt_submit_inyecta_solo_si_hay_coincidencias() {
        let s = servicio();
        let ctx = contexto_prompt(&s, "demo", "cómo funciona rmcp")
            .unwrap()
            .unwrap();
        assert!(ctx.contains("Usar rmcp para el MCP"));
        // Sin coincidencias o sin prompt: no inyecta.
        assert!(contexto_prompt(&s, "demo", "tema inexistente xyzzy")
            .unwrap()
            .is_none());
        assert!(contexto_prompt(&s, "demo", "  ").unwrap().is_none());
    }

    #[test]
    fn actividad_desactivada_respeta_el_entorno() {
        // Usa una clave de entorno real porque la función lee del proceso; se limpia al salir.
        // SAFETY: test de un solo hilo lógico sobre una var propia; se restaura el estado.
        unsafe { std::env::remove_var("TURTLE_NO_ACTIVITY") };
        assert!(!actividad_desactivada());
        unsafe { std::env::set_var("TURTLE_NO_ACTIVITY", "1") };
        assert!(actividad_desactivada());
        // Una cadena vacía no cuenta como activado (paridad con cómo trata el resto de la CLI los env).
        unsafe { std::env::set_var("TURTLE_NO_ACTIVITY", "") };
        assert!(!actividad_desactivada());
        unsafe { std::env::remove_var("TURTLE_NO_ACTIVITY") };
    }

    #[test]
    fn stdin_se_lee_acotado_y_no_revienta_la_memoria() {
        use std::io::Cursor;
        // Un JSON válido y pequeño parsea normalmente.
        let v = parsear_json_acotado(Cursor::new(br#"{"cwd":"/tmp","prompt":"hola"}"#.to_vec()))
            .expect("un JSON chico debe parsear");
        assert_eq!(v.get("prompt").and_then(|p| p.as_str()), Some("hola"));

        // Un payload patológico que supera el tope se trunca: la lectura se corta en
        // MAX_STDIN_BYTES (no se materializa entero en memoria) y el JSON parcial no parsea.
        let gigante = vec![b'a'; (MAX_STDIN_BYTES as usize) + 1_000];
        assert!(
            parsear_json_acotado(Cursor::new(gigante)).is_none(),
            "un stdin que supera el tope no debe parsear (se truncó)"
        );

        // Caso límite: un JSON válido justo bajo el tope (relleno con espacios) sigue parseando.
        let mut casi = br#"{"prompt":"x"}"#.to_vec();
        casi.resize((MAX_STDIN_BYTES as usize) - 1, b' ');
        assert!(
            parsear_json_acotado(Cursor::new(casi)).is_some(),
            "un JSON válido bajo el tope debe parsear"
        );
    }

    #[test]
    fn envoltura_tiene_el_formato_de_hook() {
        let v = envolver("SessionStart", "hola");
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "hola");
    }

    #[test]
    fn mantenimiento_respeta_el_throttle_diario() {
        let dia = MANTENIMIENTO_INTERVALO_MS;
        // Nunca corrió: corresponde correr.
        assert!(corresponde_mantenimiento(None, 5 * dia));
        // Corrió hace medio día (< intervalo): todavía no.
        assert!(!corresponde_mantenimiento(Some(5 * dia), 5 * dia + dia / 2));
        // Corrió hace exactamente un día (== intervalo): corresponde (borde inclusivo).
        assert!(corresponde_mantenimiento(Some(5 * dia), 6 * dia));
        // Corrió hace dos días (> intervalo): corresponde.
        assert!(corresponde_mantenimiento(Some(5 * dia), 7 * dia));
    }

    #[test]
    fn auto_mantenimiento_no_revienta_y_es_silencioso() {
        // Corre contra una BD en memoria y un proyecto vacío: no debe entrar en pánico ni inyectar
        // nada (es silencioso por contrato). Verifica la integración escalonar+podar best-effort.
        let s = servicio();
        auto_mantenimiento(&s, "demo");
    }

    #[test]
    fn nudge_respeta_umbral_y_enfriamiento() {
        let m = 60_000i64; // un minuto en ms
                           // Nunca guardó: no molesta en proyectos recién empezados.
        assert!(texto_nudge(None, None, 100 * m).is_none());
        // Guardó hace 10 min (< umbral 20 min): todavía no.
        assert!(texto_nudge(Some(100 * m), None, 110 * m).is_none());
        // Guardó hace 30 min (> umbral) y sin nudge previo: sí nudgea.
        assert!(texto_nudge(Some(100 * m), None, 130 * m).is_some());
        // Pasó el umbral pero nudgeó hace 5 min (< enfriamiento 15 min): no repite.
        assert!(texto_nudge(Some(100 * m), Some(125 * m), 130 * m).is_none());
        // Pasó el umbral y el último nudge fue hace 20 min (> enfriamiento): vuelve a nudgear.
        assert!(texto_nudge(Some(100 * m), Some(110 * m), 130 * m).is_some());
    }
}
