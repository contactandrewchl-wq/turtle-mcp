//! Binario `turtle` — punto de composición y entrada por subcomandos.
//!
//! Un solo ejecutable autocontenido (RNF-INS-02) con comandos cortos y ergonómicos
//! (RF-UI-01): el dato principal va como argumento posicional y el proyecto se autodetecta
//! del repo/carpeta actual (override con `-p`/`--proyecto` o `$TURTLE_PROJECT`). Toda la
//! salida va dirigida a la persona en español latino neutro (RNF-LOC-01). Ver arquitectura §2.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use turtle_core::agent::Agent;
use turtle_core::event::Event;
use turtle_core::memory::{
    Importance, Memory, MemoryIndexRow, MemoryKind, NewMemory, Tier, Verbosidad,
};
use turtle_core::message::Message;
use turtle_core::relation::{Relation, RelationKind};
use turtle_core::session::Session;
use turtle_core::skill::{Intensidad, NewSkill, Skill, SkillIndexRow, SkillKind};
use turtle_data::{default_db_path, Db};
use turtle_service::{Diagnostico, Estadisticas, EstadoChequeo, MemoryService, SearchOutcome};

/// Cuántos agentes lista `turtle agentes` como máximo.
const MAX_AGENTES: u32 = 100;

/// Cuántos eventos lista `turtle actividad` como máximo.
const MAX_EVENTOS: u32 = 100;

/// Cuántos mensajes lista `turtle bandeja` como máximo.
const MAX_MENSAJES: u32 = 100;

/// Cuántas skills listan `turtle skills buscar`/`listar` como máximo.
const MAX_SKILLS: u32 = 50;

/// Cuántas sesiones lista `turtle sesion listar` como máximo.
const MAX_SESIONES: u32 = 50;

/// Cuántas memorias por revisar lista `turtle revisar listar` como máximo.
const MAX_REVISION: u32 = 100;

mod hook;
mod modelos;
mod setup;
mod statusline;
mod sync;

/// Presupuesto de tokens por defecto para búsquedas y contexto de sesión.
const PRESUPUESTO_POR_DEFECTO: usize = 2_000;

#[derive(Parser)]
#[command(
    name = "turtle",
    version,
    about = "Turtle: memoria persistente y coordinación para agentes de IA."
)]
struct Cli {
    /// Ruta de la base. Por defecto: $TURTLE_DB o la carpeta de datos del usuario.
    #[arg(long, global = true, value_name = "ARCHIVO")]
    db: Option<PathBuf>,
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Guarda una memoria. El contenido va como argumento o por la entrada estándar.
    Guardar {
        /// Título breve y descriptivo.
        titulo: String,
        /// Contenido. Si se omite, se lee de la entrada estándar.
        contenido: Option<String>,
        /// Proyecto (por defecto: el repo/carpeta actual o $TURTLE_PROJECT).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Tipo: decision, architecture, correction, convention o note.
        #[arg(short = 't', long, default_value = "note")]
        tipo: String,
        /// Resumen de una línea para el índice de búsqueda.
        #[arg(short = 's', long)]
        resumen: Option<String>,
        /// Qué se decidió o describió.
        #[arg(long)]
        que: Option<String>,
        /// Por qué: la justificación.
        #[arg(long)]
        porque: Option<String>,
        /// Dónde aplica (archivo, módulo, área).
        #[arg(long)]
        donde: Option<String>,
        /// Qué se aprendió.
        #[arg(long)]
        aprendido: Option<String>,
        /// Importancia: pinned (fijada), normal o ephemeral (efímera).
        #[arg(short = 'i', long)]
        importancia: Option<String>,
        /// Alcance: project (por defecto) o personal (visible en todos los proyectos).
        #[arg(long)]
        scope: Option<String>,
        /// Clave de tema evolutivo (area/sub): si ya existe, actualiza en vez de duplicar (UPSERT).
        #[arg(long)]
        topic: Option<String>,
        /// Prompt del usuario que originó la memoria (best-effort si se omite).
        #[arg(long)]
        prompt: Option<String>,
    },
    /// Cambia la importancia de una memoria: pinned, normal o ephemeral.
    Importancia {
        /// Identificador de la memoria.
        id: String,
        /// Nivel: pinned, normal o ephemeral.
        nivel: String,
    },
    /// Cambia el nivel de escalonamiento de una memoria: hot, warm o cold.
    Nivel {
        /// Identificador de la memoria.
        id: String,
        /// Nivel: hot, warm o cold.
        nivel: String,
    },
    /// Escalona memorias por antigüedad: a tibio y a frío las que no se usan hace tiempo.
    Escalonar {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Días sin acceso para pasar de caliente a tibio.
        #[arg(long, default_value_t = 14)]
        dias_tibio: i64,
        /// Días sin acceso para pasar de tibio a frío.
        #[arg(long, default_value_t = 60)]
        dias_frio: i64,
    },
    /// Poda las memorias efímeras sin acceso hace más de N días.
    Podar {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Días de antigüedad de acceso para podar.
        #[arg(long, default_value_t = 30)]
        dias: i64,
    },
    /// Gestiona memorias por revisar (contexto añejo): listarlas o marcar una como revisada.
    Revisar {
        #[command(subcommand)]
        accion: AccionRevisar,
    },
    /// Línea de tiempo de una memoria y las relacionadas con ella (cronológico).
    Timeline {
        /// Identificador de la memoria de partida.
        id: String,
    },
    /// Historial de versiones de una memoria de tema evolutivo (de la más reciente a la más antigua).
    Historial {
        /// Identificador de la memoria.
        id: String,
    },
    /// Propone memorias probablemente duplicadas para consolidar (por solapamiento de título y contenido, sin IA).
    Duplicados {
        /// Proyecto a escanear (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Cuántas memorias recientes escanear (cota de latencia).
        #[arg(long, default_value_t = 100)]
        limite: u32,
    },
    /// Consolida (mueve) las memorias de un proyecto en otro.
    Consolidar {
        /// Proyecto de origen.
        de: String,
        /// Proyecto de destino.
        a: String,
    },
    /// Sincronización por fragmentos git (un archivo JSON por memoria, sin conflictos de fusión).
    Sync {
        #[command(subcommand)]
        accion: AccionSync,
    },
    /// Busca memorias por relevancia (índice barato). Por defecto, en el proyecto actual.
    Buscar {
        /// Términos de búsqueda (se pueden escribir sin comillas).
        #[arg(required = true, num_args = 1..)]
        consulta: Vec<String>,
        /// Proyecto a consultar (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Busca en todos los proyectos, no solo en el actual.
        #[arg(short = 'g', long)]
        global: bool,
        /// Presupuesto de tokens del índice devuelto.
        #[arg(long, default_value_t = PRESUPUESTO_POR_DEFECTO)]
        presupuesto: usize,
        /// Verbosidad: indice (por defecto), compacto (con extracto) o completo (con contenido).
        #[arg(short = 'v', long, default_value = "indice")]
        verbosidad: String,
    },
    /// Muestra el contenido completo de una memoria.
    #[command(alias = "ver")]
    Recuperar {
        /// Identificador de la memoria.
        id: String,
    },
    /// Arma el contexto inicial: memorias relevantes al proyecto y la tarea.
    #[command(alias = "ctx")]
    Contexto {
        /// Descripción de la tarea (opcional, sin comillas).
        #[arg(num_args = 0..)]
        tarea: Vec<String>,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Presupuesto de tokens del contexto.
        #[arg(long, default_value_t = PRESUPUESTO_POR_DEFECTO)]
        presupuesto: usize,
    },
    /// Inicia el servidor MCP por entrada/salida estándar (stdio).
    #[command(visible_alias = "mcp")]
    Servir {
        /// Perfil de herramientas: completo (por defecto) o minimo. También: $TURTLE_MCP_PROFILE.
        #[arg(long)]
        perfil: Option<String>,
    },
    /// Registra el servidor MCP de Turtle en un cliente (Claude Code, Cursor, …).
    Setup {
        /// Cliente a configurar. Si se omite, se muestra un menú para elegir.
        #[arg(value_name = "AGENTE")]
        agente: Option<String>,
        /// Archivo de configuración a escribir (por defecto, el del cliente).
        #[arg(long, value_name = "ARCHIVO")]
        config: Option<PathBuf>,
    },
    /// Configurador todo-en-uno: siembra skills+personas, registra el MCP e inyecta el protocolo.
    Install {
        /// Cliente a configurar. Si se omite, se muestra un menú para elegir.
        #[arg(value_name = "AGENTE")]
        agente: Option<String>,
        /// Archivo de configuración a escribir (por defecto, el del cliente).
        #[arg(long, value_name = "ARCHIVO")]
        config: Option<PathBuf>,
    },
    /// Quita Turtle de un cliente: entrada MCP, protocolo y subagentes (no borra la memoria).
    Uninstall {
        /// Cliente a limpiar. Si se omite, se muestra un menú para elegir.
        #[arg(value_name = "AGENTE")]
        agente: Option<String>,
        /// Archivo de configuración a editar (por defecto, el del cliente).
        #[arg(long, value_name = "ARCHIVO")]
        config: Option<PathBuf>,
    },
    /// Imprime una línea de estado para la statusLine de Claude Code (rama + modelo + tokens).
    Statusline,
    /// Elige el modelo de cada persona en Claude Code (por subscripción). Sin acción: abre un menú.
    Modelos {
        #[command(subcommand)]
        accion: Option<AccionModelos>,
    },
    /// Adaptador de hooks de Claude Code: inyecta contexto de Turtle (uso interno del plugin).
    /// El feed de actividad (`hook activity`, por cada tool-call) se puede apagar con
    /// `$TURTLE_NO_ACTIVITY` (cualquier valor no vacío): así el hook no abre siquiera la base.
    Hook {
        /// Evento: session-start o prompt-submit.
        evento: String,
    },
    /// Lista los agentes registrados (rótulo, estado, rama y tarea).
    Agentes {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Lista los agentes de todos los proyectos.
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Muestra el feed de actividad (operaciones recientes atribuidas a agentes).
    Actividad {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Muestra la actividad de todos los proyectos.
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Envía un mensaje a otro agente (o por difusión si se omite el destinatario).
    Mensaje {
        /// Texto del mensaje (se puede escribir sin comillas).
        #[arg(required = true, num_args = 1..)]
        texto: Vec<String>,
        /// Destinatario (rótulo/rol). Si se omite, es difusión a todo el proyecto.
        #[arg(short = 'a', long)]
        para: Option<String>,
        /// Remitente (tu rótulo/rol).
        #[arg(long)]
        de: Option<String>,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Muestra la bandeja de un agente (mensajes a su rol o por difusión).
    Bandeja {
        /// Rótulo del agente cuya bandeja se lee.
        agente: String,
        /// Incluye también los mensajes ya entregados.
        #[arg(long)]
        todas: bool,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Registra una relación entre dos memorias (reemplaza/conflicto/relaciona/duplicado).
    Relacionar {
        /// Memoria origen (id).
        de: String,
        /// Memoria destino (id).
        a: String,
        /// Tipo: replaces, conflicts, relates o duplicate (acepta alias en español).
        tipo: String,
        /// Nota opcional que explica el porqué.
        #[arg(long)]
        nota: Option<String>,
    },
    /// Lista las relaciones de una memoria.
    Relaciones {
        /// Identificador de la memoria.
        id: String,
    },
    /// Compara dos memorias mostrando su contenido completo.
    Comparar {
        /// Primera memoria (id).
        id_a: String,
        /// Segunda memoria (id).
        id_b: String,
    },
    /// Gestiona sesiones de trabajo.
    Sesion {
        #[command(subcommand)]
        accion: AccionSesion,
    },
    /// Gestiona la capa de skills: ingesta de `skills/` y `agents/`, búsqueda y carga.
    Skills {
        #[command(subcommand)]
        accion: AccionSkill,
    },
    /// Diagnóstico de salud de la base (esquema, integridad, índices FTS, duplicados).
    Doctor {
        /// Aplica las reparaciones seguras sugeridas (reconstruye índices FTS desincronizados).
        #[arg(long)]
        reparar: bool,
    },
    /// Muestra estadísticas de la base (totales y conteos por proyecto y por tipo).
    Stats,
    /// Guarda o muestra el checkpoint de trabajo en curso (sobrevive a la compactación).
    Checkpoint {
        /// Texto del trabajo en curso. Si se omite, muestra el último.
        #[arg(num_args = 0..)]
        texto: Vec<String>,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Exporta memorias a JSON (a un archivo con --salida, o a la salida estándar).
    Exportar {
        /// Proyecto a exportar (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Exporta todos los proyectos, no solo el actual.
        #[arg(short = 'g', long)]
        global: bool,
        /// Archivo de salida. Si se omite, se imprime por la salida estándar.
        #[arg(short = 's', long, value_name = "ARCHIVO")]
        salida: Option<PathBuf>,
    },
    /// Importa memorias desde un archivo JSON (o desde la entrada estándar).
    Importar {
        /// Archivo JSON. Si se omite, se lee de la entrada estándar.
        archivo: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AccionSkill {
    /// Escanea `skills/` y `agents/` (del proyecto y de `~/.claude`) y los indexa.
    Importar {
        /// Rutas a escanear (por defecto: los directorios estándar).
        #[arg(num_args = 0..)]
        rutas: Vec<String>,
        /// Proyecto a asignar a las skills locales (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Busca skills por palabras (índice barato).
    Buscar {
        /// Términos de búsqueda (sin comillas).
        #[arg(required = true, num_args = 1..)]
        consulta: Vec<String>,
        /// Proyecto (por defecto: el actual; siempre incluye las globales).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Muestra el contenido completo de una skill.
    Ver {
        /// Identificador de la skill.
        id: String,
    },
    /// Guarda una skill capturada manualmente (contenido por argumento o entrada estándar).
    Guardar {
        /// Nombre de la skill.
        nombre: String,
        /// Contenido (o por la entrada estándar si se omite).
        contenido: Option<String>,
        /// Tipo: behavior, knowledge, tool o agent.
        #[arg(short = 't', long, default_value = "knowledge")]
        tipo: String,
        /// Cuándo usarla.
        #[arg(short = 'c', long)]
        cuando: Option<String>,
        /// Etiquetas separadas por coma.
        #[arg(long)]
        etiquetas: Option<String>,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Lista las skills (más recientes).
    Listar {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Activa/desactiva una skill de comportamiento con una intensidad.
    Activar {
        /// Identificador de la skill.
        id: String,
        /// Intensidad: off, lite, full o ultra.
        intensidad: String,
    },
    /// Lista las skills de comportamiento activas.
    Activas {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Siembra las skills y personas embebidas en el binario (bundle completo: skills/ + agents/).
    Seed,
}

#[derive(Subcommand)]
enum AccionSync {
    /// Exporta cada memoria a un archivo JSON en un directorio (versionable en git).
    Exportar {
        /// Directorio destino (se crea si no existe).
        dir: PathBuf,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Exporta todos los proyectos.
        #[arg(short = 'g', long)]
        global: bool,
    },
    /// Importa los fragmentos JSON de un directorio.
    Importar {
        /// Directorio con los fragmentos `*.json`.
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum AccionRevisar {
    /// Lista las memorias marcadas para revisión (contexto añejo) del proyecto.
    Listar {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
    },
    /// Marca una memoria como revisada (vuelve a vigente y refresca su acceso).
    Marcar {
        /// Identificador de la memoria.
        id: String,
    },
}

#[derive(Subcommand)]
enum AccionModelos {
    /// Abre el menú interactivo para elegir persona y modelo (igual que `turtle modelos` a secas).
    Menu,
    /// Muestra cada persona con su modelo efectivo y el catálogo de modelos disponibles.
    Listar,
    /// Fija el modelo de una o más personas: turtle modelos set donatello=opus brunelleschi=claude-fable-5
    Set {
        /// Pares persona=modelo (sin comillas).
        #[arg(required = true, num_args = 1.., value_name = "PERSONA=MODELO")]
        pares: Vec<String>,
    },
    /// Quita el override de una o más personas (vuelven a su modelo por defecto). Sin args: todas.
    Reset {
        /// Slugs de persona (vacío = todas).
        #[arg(num_args = 0..)]
        personas: Vec<String>,
    },
    /// Reescribe los subagentes de Claude Code aplicando los overrides actuales.
    Aplicar,
}

#[derive(Subcommand)]
enum AccionSesion {
    /// Inicia una sesión y muestra el contexto inicial relevante.
    Iniciar {
        /// Descripción de la tarea (opcional, sin comillas).
        #[arg(num_args = 0..)]
        tarea: Vec<String>,
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Rótulo del agente (rol o dominio).
        #[arg(short = 'a', long)]
        agente: Option<String>,
        /// Presupuesto de tokens del contexto inicial.
        #[arg(long, default_value_t = PRESUPUESTO_POR_DEFECTO)]
        presupuesto: usize,
    },
    /// Cierra una sesión y registra un resumen de lo realizado.
    Cerrar {
        /// Identificador de la sesión.
        id: String,
        /// Resumen de lo realizado. Si se omite, se genera uno local.
        resumen: Option<String>,
    },
    /// Lista las sesiones anteriores con su resumen.
    Listar {
        /// Proyecto (por defecto: el actual).
        #[arg(short = 'p', long)]
        proyecto: Option<String>,
        /// Lista las sesiones de todos los proyectos.
        #[arg(short = 'g', long)]
        global: bool,
    },
}

fn main() -> ExitCode {
    match ejecutar(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("turtle: {e}");
            ExitCode::FAILURE
        }
    }
}

fn ejecutar(cli: Cli) -> Result<(), String> {
    match cli.comando {
        // `setup`, `uninstall` y `statusline` no tocan la base de Turtle.
        Comando::Setup { agente, config } => setup::ejecutar(agente, config),
        Comando::Uninstall { agente, config } => setup::desinstalar(agente, config),
        Comando::Statusline => statusline::ejecutar(),
        // `modelos` solo toca ~/.turtle y ~/.claude/agents; no necesita la base de Turtle.
        Comando::Modelos { accion } => modelos::ejecutar(accion),
        // Apagado opcional del feed de actividad (hot path del hook PreToolUse): si el usuario
        // setea $TURTLE_NO_ACTIVITY, salimos sin abrir siquiera la base. Así el costo del hook
        // baja al piso del proceso para quien no quiera el feed.
        Comando::Hook { evento } if evento == "activity" && hook::actividad_desactivada() => Ok(()),
        comando => despachar(comando, abrir_servicio(cli.db)?),
    }
}

fn despachar(comando: Comando, servicio: MemoryService) -> Result<(), String> {
    match comando {
        Comando::Guardar {
            titulo,
            contenido,
            proyecto,
            tipo,
            resumen,
            que,
            porque,
            donde,
            aprendido,
            importancia,
            scope,
            topic,
            prompt,
        } => {
            let proyecto = resolver_proyecto(proyecto);
            let kind = MemoryKind::parse(&tipo).ok_or_else(|| {
                format!(
                    "tipo desconocido: {tipo}. Use decision, architecture, correction, \
                     convention o note."
                )
            })?;
            let scope = match scope.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                None => turtle_core::memory::Scope::Project,
                Some(s) => turtle_core::memory::Scope::parse(s)
                    .ok_or_else(|| format!("alcance desconocido: {s}. Use project o personal."))?,
            };
            let contenido = match contenido {
                Some(c) => c,
                None => leer_entrada_estandar()?,
            };
            let limpiar =
                |o: Option<String>| o.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
            let nueva = NewMemory {
                project: proyecto.clone(),
                kind,
                title: titulo,
                what: que,
                why: porque,
                where_: donde,
                learned: aprendido,
                content: contenido,
                summary: resumen,
                scope,
                topic_key: limpiar(topic),
                prompt: limpiar(prompt),
            };
            let id = servicio.save(&nueva).map_err(|e| e.to_string())?;
            println!("Memoria guardada con id {id} (proyecto: {proyecto}).");
            if let Some(nivel) = importancia {
                let imp = Importance::parse(nivel.trim()).ok_or_else(|| {
                    format!("importancia desconocida: {nivel}. Use pinned, normal o ephemeral.")
                })?;
                servicio
                    .set_importance(&id, imp)
                    .map_err(|e| e.to_string())?;
            }
            let candidatos = servicio
                .detectar_candidatos(&nueva, &id)
                .map_err(|e| e.to_string())?;
            if !candidatos.is_empty() {
                println!();
                println!("Posibles duplicados/conflictos (si aplica, registralo con «turtle relacionar»):");
                imprimir_filas(&candidatos);
            }
        }
        Comando::Importancia { id, nivel } => {
            let imp = Importance::parse(nivel.trim()).ok_or_else(|| {
                format!("importancia desconocida: {nivel}. Use pinned, normal o ephemeral.")
            })?;
            if servicio
                .set_importance(&id, imp)
                .map_err(|e| e.to_string())?
            {
                println!("Importancia de {id} cambiada a {}.", imp.as_str());
            } else {
                return Err(format!("no existe una memoria con id {id}."));
            }
        }
        Comando::Nivel { id, nivel } => {
            let tier = Tier::parse(nivel.trim())
                .ok_or_else(|| format!("nivel desconocido: {nivel}. Use hot, warm o cold."))?;
            if servicio.set_tier(&id, tier).map_err(|e| e.to_string())? {
                println!("Nivel de {id} cambiado a {}.", tier.as_str());
            } else {
                return Err(format!("no existe una memoria con id {id}."));
            }
        }
        Comando::Escalonar {
            proyecto,
            dias_tibio,
            dias_frio,
        } => {
            let proyecto = resolver_proyecto(proyecto);
            let (t, f) = servicio
                .escalonar(&proyecto, dias_tibio, dias_frio)
                .map_err(|e| e.to_string())?;
            println!("Escalonadas en «{proyecto}»: {t} a tibio, {f} a frío.");
        }
        Comando::Podar { proyecto, dias } => {
            let proyecto = resolver_proyecto(proyecto);
            let n = servicio
                .podar_efimeras(&proyecto, dias)
                .map_err(|e| e.to_string())?;
            println!("Podadas {n} memorias efímeras en «{proyecto}».");
        }
        Comando::Revisar { accion } => match accion {
            AccionRevisar::Listar { proyecto } => {
                let proyecto = resolver_proyecto(proyecto);
                let filas = servicio
                    .needs_review_list(&proyecto, MAX_REVISION)
                    .map_err(|e| e.to_string())?;
                if filas.is_empty() {
                    println!("Sin memorias por revisar en «{proyecto}».");
                } else {
                    println!("Memorias por revisar (verificá antes de confiar):");
                    imprimir_filas(&filas);
                }
            }
            AccionRevisar::Marcar { id } => {
                if servicio.mark_reviewed(&id).map_err(|e| e.to_string())? {
                    println!("Memoria {id} marcada como revisada (vigente).");
                } else {
                    return Err(format!("no existe una memoria con id {id}."));
                }
            }
        },
        Comando::Timeline { id } => {
            let rows = servicio.memory_timeline(&id).map_err(|e| e.to_string())?;
            if rows.is_empty() {
                return Err(format!("no existe una memoria con id {id}."));
            }
            imprimir_filas(&rows);
        }
        Comando::Historial { id } => {
            let versiones = servicio.memory_history(&id).map_err(|e| e.to_string())?;
            if versiones.is_empty() {
                println!(
                    "La memoria {id} no tiene versiones anteriores (no se actualizó por tema)."
                );
            } else {
                let n = versiones.len();
                println!(
                    "Historial de {id}: {n} versión(es) anterior(es), de la más reciente a la más antigua:"
                );
                for (i, v) in versiones.iter().enumerate() {
                    let resumen = v
                        .summary
                        .as_deref()
                        .unwrap_or_else(|| v.content.lines().next().unwrap_or(""));
                    println!("  {}) {}  —  {}", i + 1, v.title, resumen);
                }
            }
        }
        Comando::Duplicados { proyecto, limite } => {
            let proyecto = resolver_proyecto(proyecto);
            let pares = servicio
                .consolidation_candidates(&proyecto, limite)
                .map_err(|e| e.to_string())?;
            if pares.is_empty() {
                println!("No se encontraron candidatos a duplicado en «{proyecto}».");
            } else {
                println!(
                    "Candidatos a consolidar en «{proyecto}» ({}, más fuerte primero):",
                    pares.len()
                );
                for p in &pares {
                    println!("  • {}  ↔  {}", p.a_titulo, p.b_titulo);
                    println!("      {}   {}", p.a_id, p.b_id);
                }
            }
        }
        Comando::Consolidar { de, a } => {
            let n = servicio
                .consolidate_projects(&de, &a)
                .map_err(|e| e.to_string())?;
            println!("Consolidadas {n} memorias de «{de}» en «{a}».");
        }
        Comando::Sync { accion } => match accion {
            AccionSync::Exportar {
                dir,
                proyecto,
                global,
            } => {
                let filtro = if global {
                    None
                } else {
                    Some(resolver_proyecto(proyecto))
                };
                let n = sync::exportar_fragmentos(&servicio, filtro.as_deref(), &dir)?;
                println!("Exportados {n} fragmentos a {}.", dir.display());
            }
            AccionSync::Importar { dir } => {
                let (nuevas, actualizadas) = sync::importar_fragmentos(&servicio, &dir)?;
                println!("Importación completa: {nuevas} nuevas, {actualizadas} actualizadas.");
            }
        },
        Comando::Buscar {
            consulta,
            proyecto,
            global,
            presupuesto,
            verbosidad,
        } => {
            let consulta = consulta.join(" ");
            let filtro = if global {
                None
            } else {
                Some(resolver_proyecto(proyecto))
            };
            let verb = Verbosidad::parse(&verbosidad).ok_or_else(|| {
                format!("verbosidad desconocida: {verbosidad}. Use indice, compacto o completo.")
            })?;
            let resultado = servicio
                .search(&consulta, filtro.as_deref(), presupuesto, verb)
                .map_err(|e| e.to_string())?;
            imprimir_indice(&resultado);
        }
        Comando::Recuperar { id } => match servicio.get(&id).map_err(|e| e.to_string())? {
            Some(m) => imprimir_memoria(&m),
            None => return Err(format!("no existe una memoria con id {id}.")),
        },
        Comando::Contexto {
            tarea,
            proyecto,
            presupuesto,
        } => {
            let proyecto = resolver_proyecto(proyecto);
            let tarea = tarea.join(" ");
            let resultado = servicio
                .session_context(&proyecto, &tarea, presupuesto)
                .map_err(|e| e.to_string())?;
            imprimir_indice(&resultado);
        }
        Comando::Servir { perfil } => {
            let perfil = resolver_perfil(perfil)?;
            turtle_mcp::serve_stdio_blocking_con_perfil(servicio, perfil)
                .map_err(|e| format!("el servidor MCP terminó con error: {e}"))?;
        }
        Comando::Hook { evento } => return hook::ejecutar(&evento, &servicio),
        Comando::Agentes { proyecto, global } => {
            let filtro = if global {
                None
            } else {
                Some(resolver_proyecto(proyecto))
            };
            let agentes = servicio
                .list_agents(filtro.as_deref(), MAX_AGENTES)
                .map_err(|e| e.to_string())?;
            imprimir_agentes(&agentes);
        }
        Comando::Actividad { proyecto, global } => {
            let filtro = if global {
                None
            } else {
                Some(resolver_proyecto(proyecto))
            };
            let eventos = servicio
                .list_events(filtro.as_deref(), MAX_EVENTOS)
                .map_err(|e| e.to_string())?;
            imprimir_actividad(&eventos);
        }
        Comando::Mensaje {
            texto,
            para,
            de,
            proyecto,
        } => {
            let proyecto = resolver_proyecto(proyecto);
            let cuerpo = texto.join(" ");
            servicio
                .send_message(&proyecto, de.as_deref(), para.as_deref(), &cuerpo)
                .map_err(|e| e.to_string())?;
            match para.as_deref() {
                Some(p) => println!("Mensaje enviado a {p}."),
                None => println!("Mensaje enviado por difusión."),
            }
        }
        Comando::Bandeja {
            agente,
            todas,
            proyecto,
        } => {
            let proyecto = resolver_proyecto(proyecto);
            let mensajes = servicio
                .inbox(&proyecto, &agente, !todas, MAX_MENSAJES)
                .map_err(|e| e.to_string())?;
            imprimir_bandeja(&mensajes);
        }
        Comando::Relacionar { de, a, tipo, nota } => {
            let kind = RelationKind::parse(&tipo).ok_or_else(|| {
                format!("tipo de relación desconocido: {tipo}. Use replaces, conflicts, relates o duplicate.")
            })?;
            // Integridad referencial: solo se relacionan memorias que existen (paridad con la tool
            // MCP relation_add y con «turtle comparar»); evita filas huérfanas que ninguna lectura
            // (relaciones/timeline hacen JOIN con memories) vuelve a mostrar.
            if servicio.get(&de).map_err(|e| e.to_string())?.is_none() {
                return Err(format!("no existe la memoria origen {de}."));
            }
            if servicio.get(&a).map_err(|e| e.to_string())?.is_none() {
                return Err(format!("no existe la memoria destino {a}."));
            }
            servicio
                .add_relation(&de, &a, kind, nota.as_deref())
                .map_err(|e| e.to_string())?;
            println!("Relación registrada: {de} {} {a}.", kind.etiqueta());
        }
        Comando::Relaciones { id } => {
            let rels = servicio.list_relations(&id).map_err(|e| e.to_string())?;
            imprimir_relaciones(&rels);
        }
        Comando::Comparar { id_a, id_b } => {
            let a = servicio
                .get(&id_a)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("no existe la memoria {id_a}."))?;
            let b = servicio
                .get(&id_b)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("no existe la memoria {id_b}."))?;
            println!("===== A =====");
            imprimir_memoria(&a);
            println!();
            println!("===== B =====");
            imprimir_memoria(&b);
        }
        Comando::Sesion { accion } => match accion {
            AccionSesion::Iniciar {
                tarea,
                proyecto,
                agente,
                presupuesto,
            } => {
                let proyecto = resolver_proyecto(proyecto);
                let tarea = tarea.join(" ");
                let tarea_opt = if tarea.is_empty() {
                    None
                } else {
                    Some(tarea.as_str())
                };
                let rama = detectar_rama();
                // RF-TOK-04: deltas desde la última sesión (se mide antes de abrir la nueva).
                let desde = servicio
                    .previous_session_start(&proyecto)
                    .map_err(|e| e.to_string())?;
                let id = servicio
                    .start_session(&proyecto, tarea_opt, agente.as_deref(), rama.as_deref())
                    .map_err(|e| e.to_string())?;
                println!("Sesión iniciada con id {id} (proyecto: {proyecto}).");
                // Relevos: entrega los mensajes pendientes para este agente (RF-COM-06).
                if let Some(label) = agente.as_deref() {
                    let mensajes = servicio
                        .deliver_inbox(&proyecto, label)
                        .map_err(|e| e.to_string())?;
                    if !mensajes.is_empty() {
                        println!();
                        println!("Mensajes pendientes:");
                        imprimir_bandeja(&mensajes);
                    }
                }
                let contexto = servicio
                    .session_deltas(&proyecto, &tarea, presupuesto, desde)
                    .map_err(|e| e.to_string())?;
                if !contexto.rows.is_empty() {
                    println!();
                    println!("Contexto inicial (fijadas + cambios + relevantes):");
                    imprimir_indice(&contexto);
                }
            }
            AccionSesion::Cerrar { id, resumen } => {
                match servicio
                    .close_session(&id, resumen.as_deref())
                    .map_err(|e| e.to_string())?
                {
                    Some(sesion) => {
                        println!("Sesión {} cerrada.", sesion.id);
                        if let Some(r) = sesion.summary.as_deref() {
                            println!("Resumen: {r}");
                        }
                    }
                    None => return Err(format!("no existe una sesión abierta con id {id}.")),
                }
            }
            AccionSesion::Listar { proyecto, global } => {
                let filtro = if global {
                    None
                } else {
                    Some(resolver_proyecto(proyecto))
                };
                let sesiones = servicio
                    .recent_sessions(filtro.as_deref(), MAX_SESIONES)
                    .map_err(|e| e.to_string())?;
                imprimir_sesiones(&sesiones);
            }
        },
        Comando::Skills { accion } => match accion {
            AccionSkill::Importar { rutas, proyecto } => {
                let proyecto = resolver_proyecto(proyecto);
                let reporte = if rutas.is_empty() {
                    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                    servicio.import_skills_default(&cwd, &proyecto)
                } else {
                    let rutas: Vec<PathBuf> = rutas.iter().map(PathBuf::from).collect();
                    servicio.import_skills(&rutas, &proyecto)
                }
                .map_err(|e| e.to_string())?;
                println!("Skills importadas: {}.", reporte.importadas);
                if reporte.fuentes.is_empty() {
                    println!("(No se encontraron directorios skills/ ni agents/.)");
                } else {
                    println!("Directorios leídos:");
                    for f in &reporte.fuentes {
                        println!("  {}", f.display());
                    }
                }
                for av in &reporte.avisos {
                    println!("⚠ {av}");
                }
            }
            AccionSkill::Buscar { consulta, proyecto } => {
                let proyecto = resolver_proyecto(proyecto);
                let filas = servicio
                    .search_skills(&consulta.join(" "), Some(&proyecto), MAX_SKILLS)
                    .map_err(|e| e.to_string())?;
                imprimir_skills(&filas);
            }
            AccionSkill::Ver { id } => {
                let s = servicio
                    .get_skill(&id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("no existe la skill {id}."))?;
                imprimir_skill(&s);
            }
            AccionSkill::Guardar {
                nombre,
                contenido,
                tipo,
                cuando,
                etiquetas,
                proyecto,
            } => {
                let proyecto = resolver_proyecto(proyecto);
                let kind = SkillKind::parse(&tipo).ok_or_else(|| {
                    format!(
                        "tipo de skill desconocido: {tipo}. Use behavior, knowledge, tool o agent."
                    )
                })?;
                let contenido = match contenido {
                    Some(c) => c,
                    None => leer_entrada_estandar()?,
                };
                let id = servicio
                    .save_skill(&NewSkill {
                        project: proyecto,
                        name: nombre,
                        kind,
                        when_to_use: cuando,
                        content: contenido,
                        tags: etiquetas,
                        source: None,
                    })
                    .map_err(|e| e.to_string())?;
                println!("Skill guardada con id {id}.");
            }
            AccionSkill::Listar { proyecto } => {
                let proyecto = resolver_proyecto(proyecto);
                let filas = servicio
                    .search_skills("", Some(&proyecto), MAX_SKILLS)
                    .map_err(|e| e.to_string())?;
                imprimir_skills(&filas);
            }
            AccionSkill::Activar { id, intensidad } => {
                let nivel = Intensidad::parse(&intensidad).ok_or_else(|| {
                    format!("intensidad desconocida: {intensidad}. Use off, lite, full o ultra.")
                })?;
                if servicio
                    .set_skill_intensity(&id, nivel)
                    .map_err(|e| e.to_string())?
                {
                    println!("Skill {id} → intensidad {}.", nivel.as_str());
                } else {
                    return Err(format!("no existe la skill {id}."));
                }
            }
            AccionSkill::Activas { proyecto } => {
                let proyecto = resolver_proyecto(proyecto);
                let activas = servicio
                    .active_skills(&proyecto)
                    .map_err(|e| e.to_string())?;
                if activas.is_empty() {
                    println!("Sin skills de comportamiento activas.");
                } else {
                    for s in &activas {
                        println!("{}  [{}]  {}", s.id, s.intensity.as_str(), s.name);
                    }
                }
            }
            AccionSkill::Seed => {
                let n = servicio.seed_bundled().map_err(|e| e.to_string())?;
                println!("Skills y personas sembradas: {n}.");
            }
        },
        Comando::Doctor { reparar } => {
            if reparar {
                let acciones = servicio.reparar().map_err(|e| e.to_string())?;
                if acciones.is_empty() {
                    println!("No había reparaciones automáticas pendientes.");
                } else {
                    for a in &acciones {
                        println!("✓ {a}");
                    }
                }
                println!();
            }
            let d = servicio.diagnosticar().map_err(|e| e.to_string())?;
            imprimir_diagnostico(&d);
            if d.hay_errores() {
                return Err("el diagnóstico encontró errores.".to_string());
            }
        }
        Comando::Stats => {
            let e = servicio.estadisticas().map_err(|e| e.to_string())?;
            imprimir_estadisticas(&e);
        }
        Comando::Checkpoint { texto, proyecto } => {
            let proyecto = resolver_proyecto(proyecto);
            let texto = texto.join(" ");
            if texto.trim().is_empty() {
                match servicio
                    .latest_checkpoint(&proyecto)
                    .map_err(|e| e.to_string())?
                {
                    Some(c) => println!("{}", c.content),
                    None => println!("Sin checkpoint para «{proyecto}»."),
                }
            } else {
                let id = servicio
                    .save_checkpoint(&proyecto, &texto)
                    .map_err(|e| e.to_string())?;
                println!("Checkpoint guardado ({id}).");
            }
        }
        Comando::Exportar {
            proyecto,
            global,
            salida,
        } => {
            let proyecto = if global {
                None
            } else {
                Some(resolver_proyecto(proyecto))
            };
            let json = sync::exportar(&servicio, proyecto.as_deref())?;
            match salida {
                Some(ruta) => {
                    std::fs::write(&ruta, json).map_err(|e| e.to_string())?;
                    println!("Memorias exportadas a {}.", ruta.display());
                }
                None => println!("{json}"),
            }
        }
        Comando::Importar { archivo } => {
            let json = match archivo {
                Some(ruta) => std::fs::read_to_string(&ruta).map_err(|e| e.to_string())?,
                None => leer_entrada_estandar()?,
            };
            let (nuevas, actualizadas) = sync::importar(&servicio, &json)?;
            println!("Importación completa: {nuevas} nuevas, {actualizadas} actualizadas.");
        }
        Comando::Install { agente, config } => {
            println!("Configurando Turtle (todo-en-uno)…");
            let n = servicio.seed_bundled().map_err(|e| e.to_string())?;
            println!("Skills y personas sembradas: {n}.");
            setup::ejecutar(agente, config)?;
            println!("Listo. Reiniciá tu cliente para que tome el servidor MCP de Turtle.");
        }
        Comando::Setup { .. }
        | Comando::Uninstall { .. }
        | Comando::Statusline
        | Comando::Modelos { .. } => {
            unreachable!("setup, uninstall, statusline y modelos se manejan sin abrir la base")
        }
    }
    Ok(())
}

/// Abre el servicio de memorias sobre la base indicada, `$TURTLE_DB` o la de por defecto,
/// creando la carpeta de datos si hace falta (RNF-INS-05).
fn abrir_servicio(db: Option<PathBuf>) -> Result<MemoryService, String> {
    let ruta = match db {
        Some(p) => p,
        None => match std::env::var_os("TURTLE_DB").filter(|v| !v.is_empty()) {
            Some(v) => PathBuf::from(v),
            None => {
                default_db_path().ok_or("no se pudo determinar la carpeta de datos del usuario.")?
            }
        },
    };
    if let Some(dir) = ruta.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("no se pudo crear la carpeta {}: {e}", dir.display()))?;
        }
    }
    let db = Db::open(&ruta)
        .map_err(|e| format!("no se pudo abrir la base en {}: {e}", ruta.display()))?;
    Ok(MemoryService::new(db))
}

/// Resuelve el proyecto: bandera explícita > `$TURTLE_PROJECT` > repo git / carpeta actual.
fn resolver_proyecto(flag: Option<String>) -> String {
    if let Some(p) = flag {
        return p;
    }
    if let Some(p) = std::env::var("TURTLE_PROJECT")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        return p;
    }
    detectar_proyecto_local()
}

/// Perfil de herramientas del servidor MCP: el flag, si no `$TURTLE_MCP_PROFILE`, si no completo.
fn resolver_perfil(flag: Option<String>) -> Result<turtle_mcp::Perfil, String> {
    let valor = flag.or_else(|| {
        std::env::var("TURTLE_MCP_PROFILE")
            .ok()
            .filter(|s| !s.trim().is_empty())
    });
    match valor {
        None => Ok(turtle_mcp::Perfil::default()),
        Some(v) => turtle_mcp::Perfil::parse(&v)
            .ok_or_else(|| format!("perfil desconocido: {v}. Use completo o minimo.")),
    }
}

/// Detecta el proyecto desde el directorio actual (raíz del repo git o carpeta actual).
fn detectar_proyecto_local() -> String {
    match std::env::current_dir() {
        Ok(d) => proyecto_en(&d),
        Err(_) => "default".to_string(),
    }
}

/// Detecta el proyecto a partir de `cwd`: nombre de la raíz del repo git si la hay; si no, el
/// nombre de la carpeta; `default` como último recurso.
pub(crate) fn proyecto_en(cwd: &Path) -> String {
    turtle_service::proyecto_en(cwd)
}

/// Detecta la rama de git del directorio actual (RF-AGN-02).
fn detectar_rama() -> Option<String> {
    rama_en(&std::env::current_dir().ok()?)
}

/// Detecta la rama de git a partir de `inicio`, leyendo `.git/HEAD` (RF-AGN-02). Soporta
/// worktrees (donde `.git` es un archivo que apunta al gitdir real). `None` fuera de un repo.
pub(crate) fn rama_en(inicio: &Path) -> Option<String> {
    let mut dir: &Path = inicio;
    let git = loop {
        let candidato = dir.join(".git");
        if candidato.exists() {
            break candidato;
        }
        dir = dir.parent()?;
    };
    let head_path = if git.is_dir() {
        git.join("HEAD")
    } else {
        // Worktree: `.git` es un archivo "gitdir: <ruta>".
        let contenido = std::fs::read_to_string(&git).ok()?;
        let gitdir = contenido.strip_prefix("gitdir:")?.trim();
        Path::new(gitdir).join("HEAD")
    };
    let head = std::fs::read_to_string(head_path).ok()?;
    let head = head.trim();
    if let Some(rama) = head.strip_prefix("ref: refs/heads/") {
        Some(rama.to_string())
    } else if head.is_empty() {
        None
    } else {
        // HEAD desacoplado: un sha; mostramos su forma corta.
        Some(head.chars().take(8).collect())
    }
}

fn leer_entrada_estandar() -> Result<String, String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| format!("no se pudo leer la entrada estándar: {e}"))?;
    Ok(buffer)
}

fn imprimir_indice(resultado: &SearchOutcome) {
    if resultado.rows.is_empty() {
        println!("Sin resultados.");
        return;
    }
    for fila in &resultado.rows {
        let revisar = if fila.needs_review {
            "  ⚠ por revisar"
        } else {
            ""
        };
        println!(
            "{}  [{}]  {}{revisar}",
            fila.id,
            fila.kind.as_str(),
            fila.title
        );
        if let Some(resumen) = fila.summary.as_deref() {
            if !resumen.is_empty() {
                println!("    {resumen}");
            }
        }
        if let Some(cuerpo) = fila.cuerpo.as_deref() {
            for linea in cuerpo.lines() {
                println!("    │ {linea}");
            }
        }
    }
    println!();
    let recorte = if resultado.truncated {
        " (recortado por presupuesto)"
    } else {
        ""
    };
    println!(
        "{} resultado(s), {} tokens estimados{recorte}.",
        resultado.rows.len(),
        resultado.total_tokens
    );
}

fn imprimir_memoria(m: &Memory) {
    println!("id:        {}", m.id);
    println!("proyecto:  {}", m.project);
    println!("tipo:      {}", m.kind.as_str());
    if m.importance != Importance::Normal {
        println!("importancia: {}", m.importance.as_str());
    }
    if m.scope != turtle_core::memory::Scope::Project {
        println!("alcance:   {}", m.scope.as_str());
    }
    if let Some(t) = m.topic_key.as_deref() {
        println!("tema:      {t}");
    }
    if m.review_state != turtle_core::memory::ReviewState::Active {
        println!(
            "estado:    {} (verificá antes de confiar)",
            m.review_state.as_str()
        );
    }
    println!("título:    {}", m.title);
    if let Some(s) = m.summary.as_deref() {
        println!("resumen:   {s}");
    }
    if let Some(s) = m.what.as_deref() {
        println!("qué:       {s}");
    }
    if let Some(s) = m.why.as_deref() {
        println!("porqué:    {s}");
    }
    if let Some(s) = m.where_.as_deref() {
        println!("dónde:     {s}");
    }
    if let Some(s) = m.learned.as_deref() {
        println!("aprendido: {s}");
    }
    println!();
    println!("{}", m.content);
}

fn imprimir_agentes(agentes: &[Agent]) {
    if agentes.is_empty() {
        println!("Sin agentes registrados.");
        return;
    }
    for a in agentes {
        let rama = a.branch.as_deref().unwrap_or("—");
        println!(
            "{}  [{}]  proyecto: {}  ·  rama: {}",
            a.label,
            a.status.as_str(),
            a.project,
            rama
        );
        if let Some(tarea) = a.task.as_deref().filter(|t| !t.is_empty()) {
            println!("    {tarea}");
        }
    }
}

fn imprimir_actividad(eventos: &[Event]) {
    if eventos.is_empty() {
        println!("Sin actividad registrada.");
        return;
    }
    for e in eventos {
        let agente = e.agent.as_deref().unwrap_or("alguien");
        let resumen = e
            .summary
            .as_deref()
            .map(|s| format!(": {s}"))
            .unwrap_or_default();
        println!("{}  {}{}", agente, e.kind.etiqueta(), resumen);
    }
}

fn imprimir_bandeja(mensajes: &[Message]) {
    if mensajes.is_empty() {
        println!("Bandeja vacía.");
        return;
    }
    for m in mensajes {
        let de = m.from_agent.as_deref().unwrap_or("alguien");
        let para = match m.to_agent.as_deref() {
            Some(p) => format!(" → {p}"),
            None => " (difusión)".to_string(),
        };
        let pendiente = if m.read_at.is_none() { "• " } else { "  " };
        println!("{pendiente}{de}{para}: {}", m.body);
    }
}

fn imprimir_filas(filas: &[MemoryIndexRow]) {
    for f in filas {
        let revisar = if f.needs_review {
            "  ⚠ por revisar"
        } else {
            ""
        };
        println!("{}  [{}]  {}{revisar}", f.id, f.kind.as_str(), f.title);
        if let Some(r) = f.summary.as_deref().filter(|s| !s.is_empty()) {
            println!("    {r}");
        }
    }
}

fn imprimir_relaciones(rels: &[Relation]) {
    if rels.is_empty() {
        println!("Sin relaciones.");
        return;
    }
    for r in rels {
        let nota = r
            .note
            .as_deref()
            .map(|n| format!("  ({n})"))
            .unwrap_or_default();
        println!("{} {} {}{nota}", r.from_id, r.kind.etiqueta(), r.to_id);
    }
}

fn imprimir_sesiones(sesiones: &[Session]) {
    if sesiones.is_empty() {
        println!("Sin sesiones.");
        return;
    }
    for s in sesiones {
        let tarea = s.task.as_deref().unwrap_or("—");
        println!("{}  [{}]  {}", s.id, s.status.as_str(), tarea);
        if let Some(r) = s.summary.as_deref().filter(|x| !x.is_empty()) {
            println!("    {r}");
        }
    }
}

fn imprimir_skills(filas: &[SkillIndexRow]) {
    if filas.is_empty() {
        println!("Sin skills.");
        return;
    }
    for f in filas {
        println!("{}  [{}]  {}", f.id, f.kind.as_str(), f.name);
        if let Some(c) = f.when_to_use.as_deref().filter(|s| !s.is_empty()) {
            println!("    {c}");
        }
    }
}

fn imprimir_skill(s: &Skill) {
    println!("id:       {}", s.id);
    if !s.project.is_empty() {
        println!("proyecto: {}", s.project);
    }
    println!("nombre:   {}", s.name);
    println!("tipo:     {}", s.kind.as_str());
    if s.intensity.activa() {
        println!("activa:   {}", s.intensity.as_str());
    }
    if let Some(c) = s.when_to_use.as_deref() {
        println!("cuándo:   {c}");
    }
    if let Some(t) = s.tags.as_deref().filter(|x| !x.is_empty()) {
        println!("etiqueta: {t}");
    }
    if let Some(o) = s.source.as_deref() {
        println!("origen:   {o}");
    }
    println!();
    println!("{}", s.content);
}

fn imprimir_diagnostico(d: &Diagnostico) {
    println!("Diagnóstico de Turtle");
    println!();
    for c in &d.chequeos {
        let marca = match c.estado {
            EstadoChequeo::Ok => "✓",
            EstadoChequeo::Aviso => "!",
            EstadoChequeo::Error => "✗",
        };
        println!("  {marca} {}: {}", c.nombre, c.detalle);
        if let Some(rep) = c.reparacion.as_deref() {
            println!("      ↳ sugerencia: {rep}");
        }
    }
    println!();
    println!("Conteos:");
    for (nombre, n) in &d.stats {
        println!("  {nombre}: {n}");
    }
    println!();
    println!(
        "Resultado: {}.",
        if d.hay_errores() {
            "con errores"
        } else {
            "sano"
        }
    );
}

fn imprimir_estadisticas(e: &Estadisticas) {
    let bloque = |titulo: &str, datos: &[(String, i64)]| {
        println!("{titulo}:");
        if datos.is_empty() {
            println!("  (sin datos)");
        } else {
            for (k, n) in datos {
                println!("  {k}: {n}");
            }
        }
    };
    bloque("Totales", &e.totales);
    println!();
    bloque("Memorias por proyecto", &e.por_proyecto);
    println!();
    bloque("Memorias por tipo", &e.por_tipo);
}

#[cfg(test)]
mod tests {
    use super::resolver_proyecto;

    #[test]
    fn la_bandera_de_proyecto_tiene_prioridad() {
        assert_eq!(resolver_proyecto(Some("explicito".into())), "explicito");
    }

    #[test]
    fn sin_bandera_resuelve_algo_no_vacio() {
        // Sin bandera cae en $TURTLE_PROJECT o la detección local; nunca vacío.
        assert!(!resolver_proyecto(None).trim().is_empty());
    }
}
