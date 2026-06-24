//! `turtle-mcp` — Servidor MCP (`rmcp`) por transporte stdio.
//!
//! Expone las operaciones de memoria de Turtle como herramientas MCP consumibles por
//! cualquier cliente compatible (RF-IS-01, RNF-RES-02). Cada herramienta es un adaptador
//! delgado que despacha a `turtle-service`, donde viven las reglas del dominio
//! (recuperación en dos etapas y presupuesto de tokens). Ver arquitectura §4.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use turtle_core::agent::Agent;
use turtle_core::checkpoint::Checkpoint;
use turtle_core::event::Event;
use turtle_core::memory::{
    Importance, Memory, MemoryIndexRow, MemoryKind, MemoryVersion, NewMemory, Scope, Verbosidad,
};
use turtle_core::message::Message;
use turtle_core::relation::{Relation, RelationKind};
use turtle_core::session::Session;
use turtle_core::skill::{Intensidad, NewSkill, Skill, SkillIndexRow, SkillKind};
use turtle_service::{DupCandidato, Estadisticas, MemoryService, SearchOutcome};

/// Cuántos agentes lista la herramienta `agents_list` como máximo.
const MAX_AGENTES: u32 = 100;

/// Cuántos eventos lista la herramienta `events_list` como máximo.
const MAX_EVENTOS: u32 = 100;

/// Cuántos mensajes lista la herramienta `inbox` como máximo.
const MAX_MENSAJES: u32 = 100;

/// Cuántas skills devuelve `skills_search` como máximo.
const MAX_SKILLS: u32 = 50;

/// Cuántas memorias recientes escanea `memory_duplicates` por defecto (cota de latencia).
const MAX_DUP_SCAN: u32 = 100;

/// Cuántas sesiones lista `sessions_list` como máximo.
const MAX_SESIONES: u32 = 50;

/// Cuántas memorias por revisar devuelve `memory_review list` como máximo.
const MAX_REVISION: u32 = 100;

/// Presupuesto de tokens por defecto cuando el cliente no especifica uno (RF-REC-03).
const PRESUPUESTO_POR_DEFECTO: usize = 2_000;

/// Perfil de herramientas que expone el servidor MCP (RF-TOK-01): un subconjunto de las
/// herramientas reduce los tokens de definición que el cliente carga por turno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perfil {
    /// Todas las herramientas (por defecto).
    #[default]
    Completo,
    /// Núcleo: guardar/buscar/recuperar memorias, contexto y descubrir skills.
    Minimo,
}

impl Perfil {
    /// Interpreta el nombre del perfil (acepta variantes con acento); `None` si no se reconoce.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "completo" | "full" | "all" => Perfil::Completo,
            "minimo" | "mínimo" | "min" => Perfil::Minimo,
            _ => return None,
        })
    }

    /// Herramientas que conserva el perfil mínimo.
    fn herramientas(self) -> Option<&'static [&'static str]> {
        match self {
            Perfil::Completo => None,
            Perfil::Minimo => Some(&[
                "memory_save",
                "memory_search",
                "memory_get",
                "context_get",
                "skills_search",
                "skill_get",
            ]),
        }
    }
}

/// Resuelve el proyecto de una herramienta: usa el indicado si no viene vacío; si se omite, lo
/// autodetecta del entorno/directorio (`$TURTLE_PROJECT` o la raíz del repo git), igual que la CLI.
/// Así el agente no está obligado a repetir el proyecto en cada llamada (paridad funcional).
fn proyecto_o_detectar(p: Option<String>) -> String {
    match p {
        Some(s) if !s.trim().is_empty() => s,
        _ => turtle_service::proyecto_actual(),
    }
}

/// Normaliza un filtro de proyecto opcional para las herramientas de listado/búsqueda: trata la
/// cadena vacía o de solo espacios como "sin filtro" (todos los proyectos). Evita el footgun de que
/// un cliente que mande `proyecto: ""` filtre por un proyecto vacío y no obtenga ningún resultado.
fn proyecto_filtro(p: Option<String>) -> Option<String> {
    p.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Servidor MCP de Turtle.
///
/// El servicio vive tras un `Mutex` porque la conexión SQLite es `Send` pero no `Sync`:
/// las herramientas la toman de forma breve y exclusiva para cada operación.
#[derive(Clone)]
pub struct TurtleMcp {
    service: Arc<Mutex<MemoryService>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TurtleMcp {
    /// Crea el servidor sobre un servicio de memorias ya inicializado (todas las herramientas).
    pub fn new(service: MemoryService) -> Self {
        Self::con_perfil(service, Perfil::default())
    }

    /// Crea el servidor exponiendo solo las herramientas del perfil indicado (RF-TOK-01).
    pub fn con_perfil(service: MemoryService, perfil: Perfil) -> Self {
        let mut router = Self::tool_router();
        if let Some(conservar) = perfil.herramientas() {
            let sobrantes: Vec<String> = router
                .list_all()
                .into_iter()
                .map(|t| t.name.to_string())
                .filter(|n| !conservar.contains(&n.as_str()))
                .collect();
            for n in sobrantes {
                router.remove_route(&n);
            }
        }
        Self {
            service: Arc::new(Mutex::new(service)),
            tool_router: router,
        }
    }

    /// Guarda una memoria nueva y devuelve su identificador (RF-MEM-01).
    #[tool(
        name = "memory_save",
        description = "Guarda una memoria persistente y devuelve su identificador."
    )]
    async fn memory_save(
        &self,
        Parameters(args): Parameters<GuardarArgs>,
    ) -> Result<Json<GuardarSalida>, ErrorData> {
        let kind = parse_tipo(args.tipo.as_deref())?;
        let scope = parse_scope(args.scope.as_deref())?;
        let nueva = NewMemory {
            project: proyecto_o_detectar(args.proyecto),
            kind,
            title: args.titulo,
            what: args.que,
            why: args.porque,
            where_: args.donde,
            learned: args.aprendido,
            content: args.contenido,
            summary: args.resumen,
            scope,
            topic_key: limpiar_opt(args.topic_key),
            prompt: limpiar_opt(args.prompt),
        };
        let servicio = self.service.lock().await;
        let id = servicio.save(&nueva).map_err(error_interno)?;
        // Importancia opcional al guardar (RF-MEM-04).
        if let Some(imp) = parse_importancia(args.importancia.as_deref())? {
            servicio.set_importance(&id, imp).map_err(error_interno)?;
        }
        // RF-CNF-01: candidatos a duplicado/conflicto, para que el agente los juzgue (RF-CNF-03).
        let candidatos = servicio
            .detectar_candidatos(&nueva, &id)
            .map_err(error_interno)?;
        Ok(Json(GuardarSalida {
            id,
            candidatos: candidatos.into_iter().map(a_fila_indice).collect(),
        }))
    }

    /// Cambia la importancia de una memoria: pinned, normal o ephemeral (RF-MEM-04).
    #[tool(
        name = "memory_importance",
        description = "Cambia la importancia de una memoria: pinned (fijada), normal o ephemeral \
                       (efímera). Las fijadas se priorizan en el contexto de sesión."
    )]
    async fn memory_importance(
        &self,
        Parameters(args): Parameters<ImportanciaArgs>,
    ) -> Result<Json<OkSalida>, ErrorData> {
        let imp = Importance::parse(args.importancia.trim()).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "Importancia desconocida: {}. Use pinned, normal o ephemeral.",
                    args.importancia
                ),
                None,
            )
        })?;
        let cambiada = self
            .service
            .lock()
            .await
            .set_importance(&args.id, imp)
            .map_err(error_interno)?;
        if cambiada {
            Ok(Json(OkSalida { ok: true }))
        } else {
            Err(ErrorData::invalid_params(
                format!("No existe una memoria con id {}.", args.id),
                None,
            ))
        }
    }

    /// Busca memorias por relevancia y devuelve solo el índice barato (RF-REC-01, RF-REC-03).
    #[tool(
        name = "memory_search",
        description = "Busca memorias por relevancia y devuelve un índice barato (sin el contenido \
                       completo), recortado a un presupuesto de tokens. Usa memory_get para traer \
                       el contenido de un resultado concreto."
    )]
    async fn memory_search(
        &self,
        Parameters(args): Parameters<BuscarArgs>,
    ) -> Result<Json<IndiceSalida>, ErrorData> {
        let presupuesto = args.presupuesto.unwrap_or(PRESUPUESTO_POR_DEFECTO);
        let verbosidad = parse_verbosidad(args.verbosidad.as_deref())?;
        let filtro = proyecto_filtro(args.proyecto);
        let outcome = self
            .service
            .lock()
            .await
            .search(&args.consulta, filtro.as_deref(), presupuesto, verbosidad)
            .map_err(error_interno)?;
        Ok(Json(a_indice_salida(outcome)))
    }

    /// Recupera el contenido completo de una memoria por id: segunda etapa (RF-REC-02).
    #[tool(
        name = "memory_get",
        description = "Recupera el contenido completo de una memoria por su identificador."
    )]
    async fn memory_get(
        &self,
        Parameters(args): Parameters<RecuperarArgs>,
    ) -> Result<Json<MemoriaSalida>, ErrorData> {
        let encontrada = self
            .service
            .lock()
            .await
            .get(&args.id)
            .map_err(error_interno)?;
        match encontrada {
            Some(m) => Ok(Json(a_memoria_salida(m))),
            None => Err(ErrorData::invalid_params(
                format!("No existe una memoria con id {}.", args.id),
                None,
            )),
        }
    }

    /// Línea de tiempo de una memoria y las relacionadas con ella, cronológica (RF-REC-09).
    #[tool(
        name = "memory_timeline",
        description = "Devuelve, en orden cronológico, una memoria y las relacionadas con ella \
                       (por relaciones registradas). Índice barato."
    )]
    async fn memory_timeline(
        &self,
        Parameters(args): Parameters<TimelineArgs>,
    ) -> Result<Json<IndiceSalida>, ErrorData> {
        let rows = self
            .service
            .lock()
            .await
            .memory_timeline(&args.id)
            .map_err(error_interno)?;
        Ok(Json(IndiceSalida {
            resultados: rows.into_iter().map(a_fila_indice).collect(),
            tokens_total: 0,
            truncado: false,
        }))
    }

    /// Historial de versiones de una memoria de tema evolutivo (versionado temporal de temas).
    #[tool(
        name = "memory_history",
        description = "Devuelve el historial de versiones de una memoria de tema evolutivo (las que \
                       fueron reemplazadas al actualizar por `topic_key`), de la más reciente a la \
                       más antigua, con su intervalo de validez. Vacío si nunca se actualizó."
    )]
    async fn memory_history(
        &self,
        Parameters(args): Parameters<RecuperarArgs>,
    ) -> Result<Json<HistorialSalida>, ErrorData> {
        let versiones = self
            .service
            .lock()
            .await
            .memory_history(&args.id)
            .map_err(error_interno)?;
        Ok(Json(HistorialSalida {
            memory_id: args.id,
            versiones: versiones.into_iter().map(a_version_salida).collect(),
        }))
    }

    /// Consolidación asistida: propone pares de memorias probablemente duplicadas (sin IA).
    #[tool(
        name = "memory_duplicates",
        description = "Propone pares de memorias probablemente duplicadas en un proyecto (por \
                       solapamiento de título y contenido vía FTS, sin IA) para que las consolides: fusiona \
                       con memory_save + topic_key, vincula con relation_add, o borra la redundante. \
                       Turtle propone; tú decides."
    )]
    async fn memory_duplicates(
        &self,
        Parameters(args): Parameters<DuplicadosArgs>,
    ) -> Result<Json<DuplicadosSalida>, ErrorData> {
        let proyecto = proyecto_o_detectar(args.proyecto);
        let limite = args.limite.unwrap_or(MAX_DUP_SCAN);
        let pares = self
            .service
            .lock()
            .await
            .consolidation_candidates(&proyecto, limite)
            .map_err(error_interno)?;
        Ok(Json(DuplicadosSalida {
            pares: pares.into_iter().map(a_par_duplicado).collect(),
        }))
    }

    /// Arma el contexto inicial de una sesión para un proyecto y una tarea (RF-REC-08).
    #[tool(
        name = "context_get",
        description = "Arma el contexto inicial de una sesión: memorias relevantes al proyecto y a \
                       la tarea en curso, dentro de un presupuesto de tokens."
    )]
    async fn context_get(
        &self,
        Parameters(args): Parameters<ContextoArgs>,
    ) -> Result<Json<IndiceSalida>, ErrorData> {
        let proyecto = proyecto_o_detectar(args.proyecto);
        let presupuesto = args.presupuesto.unwrap_or(PRESUPUESTO_POR_DEFECTO);
        let tarea = args.tarea.unwrap_or_default();
        let outcome = self
            .service
            .lock()
            .await
            .session_context(&proyecto, &tarea, presupuesto)
            .map_err(error_interno)?;
        Ok(Json(a_indice_salida(outcome)))
    }

    /// Inicia una sesión y devuelve su id junto con el contexto inicial relevante (RF-SES-01).
    #[tool(
        name = "session_start",
        description = "Inicia una sesión de trabajo (proyecto, tarea y agente) y devuelve su \
                       identificador junto con el contexto inicial relevante para la tarea."
    )]
    async fn session_start(
        &self,
        Parameters(args): Parameters<SesionIniciarArgs>,
    ) -> Result<Json<SesionIniciadaSalida>, ErrorData> {
        let presupuesto = args.presupuesto.unwrap_or(PRESUPUESTO_POR_DEFECTO);
        let proyecto = proyecto_o_detectar(args.proyecto);
        let servicio = self.service.lock().await;
        // RF-TOK-04: deltas desde la última sesión (se mide antes de abrir la nueva).
        let desde = servicio
            .previous_session_start(&proyecto)
            .map_err(error_interno)?;
        let id = servicio
            .start_session(
                &proyecto,
                args.tarea.as_deref(),
                args.agente.as_deref(),
                args.rama.as_deref(),
            )
            .map_err(error_interno)?;
        let tarea = args.tarea.unwrap_or_default();
        let outcome = servicio
            .session_deltas(&proyecto, &tarea, presupuesto, desde)
            .map_err(error_interno)?;
        // Relevos: entrega los mensajes pendientes para el agente (RF-COM-06).
        let mensajes = match args.agente.as_deref() {
            Some(label) => servicio
                .deliver_inbox(&proyecto, label)
                .map_err(error_interno)?,
            None => Vec::new(),
        };
        Ok(Json(SesionIniciadaSalida {
            id,
            contexto: a_indice_salida(outcome),
            mensajes: mensajes.into_iter().map(a_mensaje_salida).collect(),
        }))
    }

    /// Cierra una sesión y registra un resumen de lo realizado (RF-SES-02).
    #[tool(
        name = "session_close",
        description = "Cierra una sesión y registra un resumen de lo realizado. Si no se entrega \
                       un resumen, se genera uno local con las memorias creadas durante la sesión."
    )]
    async fn session_close(
        &self,
        Parameters(args): Parameters<SesionCerrarArgs>,
    ) -> Result<Json<SesionSalida>, ErrorData> {
        let cerrada = self
            .service
            .lock()
            .await
            .close_session(&args.id, args.resumen.as_deref())
            .map_err(error_interno)?;
        match cerrada {
            Some(s) => Ok(Json(a_sesion_salida(s))),
            None => Err(ErrorData::invalid_params(
                format!("No existe una sesión abierta con id {}.", args.id),
                None,
            )),
        }
    }

    /// Lista las sesiones anteriores con su resumen (RF-SES-03).
    #[tool(
        name = "sessions_list",
        description = "Lista las sesiones anteriores (tarea, estado y resumen), de la más reciente \
                       a la más antigua. Sin proyecto, lista todas."
    )]
    async fn sessions_list(
        &self,
        Parameters(args): Parameters<SesionesArgs>,
    ) -> Result<Json<SesionesSalida>, ErrorData> {
        let filtro = proyecto_filtro(args.proyecto);
        let sesiones = self
            .service
            .lock()
            .await
            .recent_sessions(filtro.as_deref(), MAX_SESIONES)
            .map_err(error_interno)?;
        Ok(Json(SesionesSalida {
            sesiones: sesiones.into_iter().map(a_sesion_salida).collect(),
        }))
    }

    /// Estadísticas de la base: totales y conteos por proyecto y por tipo (RF-DIA-01).
    #[tool(
        name = "stats",
        description = "Estadísticas de la base: totales por entidad y conteo de memorias por \
                       proyecto y por tipo."
    )]
    async fn stats(&self) -> Result<Json<EstadisticasSalida>, ErrorData> {
        let e = self
            .service
            .lock()
            .await
            .estadisticas()
            .map_err(error_interno)?;
        Ok(Json(a_estadisticas_salida(e)))
    }

    /// Lista los agentes registrados (rótulo, estado, rama, tarea) para coordinar entre agentes.
    #[tool(
        name = "agents_list",
        description = "Lista los agentes registrados (rótulo, estado, rama y tarea), del más \
                       recientemente activo al más antiguo. Sin proyecto, lista todos."
    )]
    async fn agents_list(
        &self,
        Parameters(args): Parameters<AgentesArgs>,
    ) -> Result<Json<AgentesSalida>, ErrorData> {
        let filtro = proyecto_filtro(args.proyecto);
        let agentes = self
            .service
            .lock()
            .await
            .list_agents(filtro.as_deref(), MAX_AGENTES)
            .map_err(error_interno)?;
        Ok(Json(AgentesSalida {
            agentes: agentes.into_iter().map(a_agente_salida).collect(),
        }))
    }

    /// Lista los eventos recientes del bus de actividad (operaciones atribuidas a agentes).
    #[tool(
        name = "events_list",
        description = "Lista los eventos recientes del bus de actividad (guardados, sesiones, \
                       registros de agente), del más nuevo al más antiguo. Sin proyecto, lista todos."
    )]
    async fn events_list(
        &self,
        Parameters(args): Parameters<EventosArgs>,
    ) -> Result<Json<EventosSalida>, ErrorData> {
        let filtro = proyecto_filtro(args.proyecto);
        let eventos = self
            .service
            .lock()
            .await
            .list_events(filtro.as_deref(), MAX_EVENTOS)
            .map_err(error_interno)?;
        Ok(Json(EventosSalida {
            eventos: eventos.into_iter().map(a_evento_salida).collect(),
        }))
    }

    /// Envía un mensaje a otro agente o por difusión (RF-COM-03..05).
    #[tool(
        name = "message_send",
        description = "Envía un mensaje a un agente (por su rótulo/rol) o por difusión a todo el \
                       proyecto (omitiendo el destinatario). Queda en la bandeja del destinatario \
                       y en el feed de actividad."
    )]
    async fn message_send(
        &self,
        Parameters(args): Parameters<MensajeArgs>,
    ) -> Result<Json<MensajeEnviadoSalida>, ErrorData> {
        let proyecto = proyecto_o_detectar(args.proyecto);
        let id = self
            .service
            .lock()
            .await
            .send_message(
                &proyecto,
                args.de.as_deref(),
                args.para.as_deref(),
                &args.cuerpo,
            )
            .map_err(error_interno)?;
        Ok(Json(MensajeEnviadoSalida { id }))
    }

    /// Lee la bandeja de un agente: mensajes dirigidos a su rol o por difusión (RF-COM-06).
    #[tool(
        name = "inbox",
        description = "Lee la bandeja de un agente: mensajes dirigidos a su rótulo o por difusión. \
                       Por defecto, solo los pendientes; no los marca entregados."
    )]
    async fn inbox(
        &self,
        Parameters(args): Parameters<BandejaArgs>,
    ) -> Result<Json<BandejaSalida>, ErrorData> {
        let solo_pendientes = args.solo_pendientes.unwrap_or(true);
        let proyecto = proyecto_o_detectar(args.proyecto);
        let mensajes = self
            .service
            .lock()
            .await
            .inbox(&proyecto, &args.agente, solo_pendientes, MAX_MENSAJES)
            .map_err(error_interno)?;
        Ok(Json(BandejaSalida {
            mensajes: mensajes.into_iter().map(a_mensaje_salida).collect(),
        }))
    }

    /// Registra cómo se relacionan dos memorias, tras el juicio del agente (RF-CNF-02/03).
    #[tool(
        name = "relation_add",
        description = "Registra cómo se relacionan dos memorias: replaces (reemplaza), conflicts \
                       (conflicto), relates (se relaciona) o duplicate (duplicado). Pensado para \
                       usarse sobre los candidatos que devuelve memory_save."
    )]
    async fn relation_add(
        &self,
        Parameters(args): Parameters<RelacionArgs>,
    ) -> Result<Json<RelacionAgregadaSalida>, ErrorData> {
        let kind = RelationKind::parse(&args.tipo).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "tipo de relación desconocido: {}. Use replaces, conflicts, relates o duplicate.",
                    args.tipo
                ),
                None,
            )
        })?;
        let servicio = self.service.lock().await;
        // Integridad referencial del contrato: una relación solo une memorias que existen.
        // Sin esta verificación, ids inexistentes o vacíos crean filas huérfanas que ninguna
        // lectura (list_relations/timeline hacen JOIN con memories) vuelve a mostrar.
        // Mismo comportamiento que `memory_compare`: inexistente -> invalid_params (-32602).
        if servicio.get(&args.de).map_err(error_interno)?.is_none() {
            return Err(ErrorData::invalid_params(
                format!("No existe la memoria origen {}.", args.de),
                None,
            ));
        }
        if servicio.get(&args.a).map_err(error_interno)?.is_none() {
            return Err(ErrorData::invalid_params(
                format!("No existe la memoria destino {}.", args.a),
                None,
            ));
        }
        let id = servicio
            .add_relation(&args.de, &args.a, kind, args.nota.as_deref())
            .map_err(error_interno)?;
        Ok(Json(RelacionAgregadaSalida { id }))
    }

    /// Lista las relaciones que tocan una memoria (RF-CNF-02).
    #[tool(
        name = "relations_list",
        description = "Lista las relaciones que tocan una memoria (como origen o destino)."
    )]
    async fn relations_list(
        &self,
        Parameters(args): Parameters<RelacionesArgs>,
    ) -> Result<Json<RelacionesSalida>, ErrorData> {
        let rels = self
            .service
            .lock()
            .await
            .list_relations(&args.id)
            .map_err(error_interno)?;
        Ok(Json(RelacionesSalida {
            relaciones: rels.into_iter().map(a_relacion_salida).collect(),
        }))
    }

    /// Compara dos memorias devolviendo el contenido completo de ambas (RF-CNF-04).
    #[tool(
        name = "memory_compare",
        description = "Devuelve el contenido completo de dos memorias para compararlas y decidir \
                       si se relacionan, reemplazan o duplican."
    )]
    async fn memory_compare(
        &self,
        Parameters(args): Parameters<CompararArgs>,
    ) -> Result<Json<CompararSalida>, ErrorData> {
        let servicio = self.service.lock().await;
        let a = servicio
            .get(&args.id_a)
            .map_err(error_interno)?
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("No existe la memoria {}.", args.id_a), None)
            })?;
        let b = servicio
            .get(&args.id_b)
            .map_err(error_interno)?
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("No existe la memoria {}.", args.id_b), None)
            })?;
        Ok(Json(CompararSalida {
            a: a_memoria_salida(a),
            b: a_memoria_salida(b),
        }))
    }

    /// Ingiere skills y agentes de `skills/`/`agents/` en SQLite/FTS5 (RF-SKL-06).
    #[tool(
        name = "skills_import",
        description = "Escanea los directorios skills/ y agents/ (del proyecto y de ~/.claude) y \
                       los indexa en Turtle para poder buscarlos. Reescanear es idempotente."
    )]
    async fn skills_import(
        &self,
        Parameters(args): Parameters<SkillsImportArgs>,
    ) -> Result<Json<SkillsImportSalida>, ErrorData> {
        // Autodetecta el proyecto si se omite (paridad con `turtle skills importar`): así las skills
        // locales quedan bajo el proyecto actual y no bajo "" (que las dejaría huérfanas de búsqueda).
        let proyecto = proyecto_o_detectar(args.proyecto);
        let servicio = self.service.lock().await;
        let reporte = if args.rutas.is_empty() {
            let cwd = std::env::current_dir().unwrap_or_default();
            servicio.import_skills_default(&cwd, &proyecto)
        } else {
            let rutas: Vec<PathBuf> = args.rutas.iter().map(PathBuf::from).collect();
            servicio.import_skills(&rutas, &proyecto)
        }
        .map_err(error_interno)?;
        Ok(Json(SkillsImportSalida {
            importadas: reporte.importadas,
            fuentes: reporte
                .fuentes
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            avisos: reporte.avisos,
        }))
    }

    /// Activa o desactiva una skill de comportamiento con una intensidad (RF-SKL-07).
    #[tool(
        name = "skill_activate",
        description = "Activa o desactiva una skill de comportamiento con una intensidad: off, \
                       lite, full o ultra. Las activas se inyectan en el contexto de sesión."
    )]
    async fn skill_activate(
        &self,
        Parameters(args): Parameters<SkillActivarArgs>,
    ) -> Result<Json<OkSalida>, ErrorData> {
        let nivel = Intensidad::parse(&args.intensidad).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "intensidad desconocida: {}. Use off, lite, full o ultra.",
                    args.intensidad
                ),
                None,
            )
        })?;
        let ok = self
            .service
            .lock()
            .await
            .set_skill_intensity(&args.id, nivel)
            .map_err(error_interno)?;
        if ok {
            Ok(Json(OkSalida { ok: true }))
        } else {
            Err(ErrorData::invalid_params(
                format!("No existe una skill con id {}.", args.id),
                None,
            ))
        }
    }

    /// Busca skills en modo índice barato (RF-SKL-03).
    #[tool(
        name = "skills_search",
        description = "Busca skills por palabras y devuelve solo metadatos baratos (id, nombre, \
                       tipo, cuándo usarla). Usá skill_get para cargar el contenido completo."
    )]
    async fn skills_search(
        &self,
        Parameters(args): Parameters<SkillsBuscarArgs>,
    ) -> Result<Json<SkillsIndiceSalida>, ErrorData> {
        let limite = args.limite.unwrap_or(MAX_SKILLS);
        let filtro = proyecto_filtro(args.proyecto);
        let filas = self
            .service
            .lock()
            .await
            .search_skills(&args.consulta, filtro.as_deref(), limite)
            .map_err(error_interno)?;
        Ok(Json(SkillsIndiceSalida {
            resultados: filas.into_iter().map(a_fila_skill).collect(),
        }))
    }

    /// Carga el contenido completo de una skill por id (RF-SKL-04).
    #[tool(
        name = "skill_get",
        description = "Carga el contenido completo de una skill por su identificador."
    )]
    async fn skill_get(
        &self,
        Parameters(args): Parameters<SkillGetArgs>,
    ) -> Result<Json<SkillSalida>, ErrorData> {
        let s = self
            .service
            .lock()
            .await
            .get_skill(&args.id)
            .map_err(error_interno)?;
        match s {
            Some(s) => Ok(Json(a_skill_salida(s))),
            None => Err(ErrorData::invalid_params(
                format!("No existe una skill con id {}.", args.id),
                None,
            )),
        }
    }

    /// Guarda una skill capturada en la interacción (RF-SKL-05).
    #[tool(
        name = "skill_save",
        description = "Guarda una skill nueva (behavior/knowledge/tool/agent) capturada en la \
                       conversación."
    )]
    async fn skill_save(
        &self,
        Parameters(args): Parameters<SkillGuardarArgs>,
    ) -> Result<Json<SkillGuardadaSalida>, ErrorData> {
        let tipo = args.tipo.as_deref().unwrap_or("knowledge");
        let kind = SkillKind::parse(tipo).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "tipo de skill desconocido: {tipo}. Use behavior, knowledge, tool o agent."
                ),
                None,
            )
        })?;
        let id = self
            .service
            .lock()
            .await
            .save_skill(&NewSkill {
                // Autodetecta el proyecto si se omite (paridad con `turtle skills guardar`), en vez
                // de guardar bajo "" y dejar la skill huérfana de una búsqueda por proyecto explícito.
                project: proyecto_o_detectar(args.proyecto),
                name: args.nombre,
                kind,
                when_to_use: args.cuando_usar,
                content: args.contenido,
                tags: args.etiquetas,
                source: None,
            })
            .map_err(error_interno)?;
        Ok(Json(SkillGuardadaSalida { id }))
    }

    /// Carga las skills semilla embebidas (RF-SKL-09).
    #[tool(
        name = "skills_seed",
        description = "Carga el bundle de skills y personas embebidas en Turtle (comportamiento, \
                       conocimiento, herramienta y subagentes). Idempotente."
    )]
    async fn skills_seed(&self) -> Result<Json<SkillsSeedSalida>, ErrorData> {
        let cargadas = self
            .service
            .lock()
            .await
            .seed_bundled()
            .map_err(error_interno)?;
        Ok(Json(SkillsSeedSalida { cargadas }))
    }

    /// Guarda el trabajo en curso para sobrevivir a una compactación de contexto (RF-SES-04).
    #[tool(
        name = "checkpoint_save",
        description = "Guarda el trabajo en curso (qué estás haciendo, próximos pasos) para \
                       sobrevivir a una compactación de contexto. Llamalo al alcanzar un hito o \
                       si la compactación se acerca."
    )]
    async fn checkpoint_save(
        &self,
        Parameters(args): Parameters<CheckpointGuardarArgs>,
    ) -> Result<Json<CheckpointGuardadoSalida>, ErrorData> {
        let proyecto = proyecto_o_detectar(args.proyecto);
        let id = self
            .service
            .lock()
            .await
            .save_checkpoint(&proyecto, &args.contenido)
            .map_err(error_interno)?;
        Ok(Json(CheckpointGuardadoSalida { id }))
    }

    /// Recupera el último checkpoint de trabajo en curso al reanudar (RF-SES-04).
    #[tool(
        name = "checkpoint_get",
        description = "Devuelve el último checkpoint de trabajo en curso de un proyecto, para \
                       retomar el contexto tras una compactación."
    )]
    async fn checkpoint_get(
        &self,
        Parameters(args): Parameters<CheckpointGetArgs>,
    ) -> Result<Json<CheckpointSalida>, ErrorData> {
        let proyecto = proyecto_o_detectar(args.proyecto);
        let c = self
            .service
            .lock()
            .await
            .latest_checkpoint(&proyecto)
            .map_err(error_interno)?;
        Ok(Json(a_checkpoint_salida(c)))
    }

    /// Ciclo de vida de memorias añejas (paridad con el estado de revisión): listar las marcadas
    /// para revisión, o marcar una como revisada. Acciones: `list` (por defecto) y `mark_reviewed`.
    #[tool(
        name = "memory_review",
        description = "Gestiona memorias por revisar (contexto añejo). accion=list devuelve las \
                       marcadas needs_review del proyecto; accion=mark_reviewed con un id la vuelve \
                       a vigente (active). No se auto-marcan revisadas: es decisión del agente."
    )]
    async fn memory_review(
        &self,
        Parameters(args): Parameters<RevisionArgs>,
    ) -> Result<Json<RevisionSalida>, ErrorData> {
        let accion = args
            .accion
            .as_deref()
            .unwrap_or("list")
            .trim()
            .to_lowercase();
        let servicio = self.service.lock().await;
        match accion.as_str() {
            "list" | "listar" => {
                let proyecto = proyecto_o_detectar(args.proyecto);
                let filas = servicio
                    .needs_review_list(&proyecto, MAX_REVISION)
                    .map_err(error_interno)?;
                Ok(Json(RevisionSalida {
                    revisar: filas.into_iter().map(a_fila_indice).collect(),
                    ok: None,
                }))
            }
            "mark_reviewed" | "marcar" => {
                let id = args
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ErrorData::invalid_params(
                            "mark_reviewed requiere el id de la memoria.",
                            None,
                        )
                    })?;
                let ok = servicio.mark_reviewed(id).map_err(error_interno)?;
                if !ok {
                    return Err(ErrorData::invalid_params(
                        format!("No existe una memoria con id {id}."),
                        None,
                    ));
                }
                Ok(Json(RevisionSalida {
                    revisar: Vec::new(),
                    ok: Some(true),
                }))
            }
            otra => Err(ErrorData::invalid_params(
                format!("Acción desconocida: {otra}. Use list o mark_reviewed."),
                None,
            )),
        }
    }

    /// Registra el último prompt del usuario para un proyecto (best-effort). Lo usa el hook
    /// prompt-submit para que un `memory_save` posterior sin prompt explícito lo adjunte solo.
    #[tool(
        name = "memory_save_prompt",
        description = "Registra el prompt actual del usuario para el proyecto. Un memory_save \
                       posterior que no traiga prompt lo adjunta automáticamente (best-effort)."
    )]
    async fn memory_save_prompt(
        &self,
        Parameters(args): Parameters<GuardarPromptArgs>,
    ) -> Result<Json<OkSalida>, ErrorData> {
        let prompt = args.prompt.trim();
        if prompt.is_empty() {
            return Err(ErrorData::invalid_params(
                "El prompt no puede ser vacío.",
                None,
            ));
        }
        let proyecto = proyecto_o_detectar(args.proyecto);
        self.service
            .lock()
            .await
            .record_prompt(&proyecto, args.sesion.as_deref(), prompt)
            .map_err(error_interno)?;
        Ok(Json(OkSalida { ok: true }))
    }

    /// Propone una clave de tema estable (`area/sub`) para usar como `topic_key` en memory_save.
    /// Heurística simple, sin IA; el agente puede ignorarla y pasar su propia clave.
    #[tool(
        name = "suggest_topic_key",
        description = "Sugiere una clave de tema estable (area/sub) a partir del título y el \
                       contenido, para agrupar una memoria evolutiva con memory_save(topic_key)."
    )]
    async fn suggest_topic_key(
        &self,
        Parameters(args): Parameters<SugerirTopicArgs>,
    ) -> Result<Json<TopicKeySalida>, ErrorData> {
        let kind = parse_tipo(args.tipo.as_deref())?;
        let topic_key = turtle_service::suggest_topic_key(
            kind.as_str(),
            &args.titulo,
            args.contenido.as_deref().unwrap_or(""),
        );
        Ok(Json(TopicKeySalida { topic_key }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TurtleMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("turtle", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Memoria persistente de Turtle. Herramientas: memory_save (guardar una memoria), \
                 memory_search (buscar en el índice por relevancia), memory_get (traer el contenido \
                 completo de un resultado) y context_get (contexto inicial de una sesión). La \
                 búsqueda devuelve metadatos baratos; recupera el contenido completo solo cuando lo \
                 necesites para cuidar el presupuesto de tokens.",
            )
    }
}

/// Ejecuta el servidor MCP sobre stdio hasta que el cliente cierra la conexión.
pub async fn serve_stdio(service: MemoryService) -> Result<(), TurtleMcpError> {
    serve_stdio_con_perfil(service, Perfil::default()).await
}

/// Como `serve_stdio`, pero exponiendo solo las herramientas del perfil (RF-TOK-01).
pub async fn serve_stdio_con_perfil(
    service: MemoryService,
    perfil: Perfil,
) -> Result<(), TurtleMcpError> {
    let server = TurtleMcp::con_perfil(service, perfil);
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| TurtleMcpError(e.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|e| TurtleMcpError(e.to_string()))?;
    Ok(())
}

/// Variante bloqueante: crea su propio runtime de Tokio. Pensada para invocarse desde la
/// CLI síncrona (`turtle servir`), que no necesita conocer Tokio ni `async`.
pub fn serve_stdio_blocking(service: MemoryService) -> Result<(), TurtleMcpError> {
    serve_stdio_blocking_con_perfil(service, Perfil::default())
}

/// Variante bloqueante con perfil de herramientas (RF-TOK-01).
pub fn serve_stdio_blocking_con_perfil(
    service: MemoryService,
    perfil: Perfil,
) -> Result<(), TurtleMcpError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .map_err(|e| TurtleMcpError(e.to_string()))?;
    rt.block_on(serve_stdio_con_perfil(service, perfil))
}

/// Error opaco del servidor MCP: encapsula los errores de inicialización o de ejecución del
/// transporte como un mensaje en español, sin filtrar tipos de `rmcp` a las capas de arriba.
#[derive(Debug)]
pub struct TurtleMcpError(String);

impl std::fmt::Display for TurtleMcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TurtleMcpError {}

// ---- Tipos de entrada y salida de las herramientas ----

/// Argumentos de `memory_save`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GuardarArgs {
    /// Proyecto al que pertenece la memoria. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Tipo de memoria: decision, architecture, correction, convention o note. Por defecto note.
    pub tipo: Option<String>,
    /// Título breve y descriptivo.
    pub titulo: String,
    /// Contenido completo de la memoria.
    pub contenido: String,
    /// Resumen de una línea para el índice de búsqueda.
    pub resumen: Option<String>,
    /// Qué se decidió o describió.
    pub que: Option<String>,
    /// Por qué: la justificación.
    pub porque: Option<String>,
    /// Dónde aplica (archivo, módulo, área).
    pub donde: Option<String>,
    /// Qué se aprendió.
    pub aprendido: Option<String>,
    /// Importancia: pinned, normal (por defecto) o ephemeral (RF-MEM-04).
    pub importancia: Option<String>,
    /// Alcance: project (por defecto) o personal (transversal al usuario; visible en todos los
    /// proyectos). Paridad con sistemas afines.
    pub scope: Option<String>,
    /// Clave de tema evolutivo (p. ej. `area/sub`). Si ya existe una memoria con la misma clave en
    /// el mismo proyecto+scope, se ACTUALIZA en lugar de duplicarse (UPSERT, paridad funcional).
    pub topic_key: Option<String>,
    /// Prompt del usuario que originó la memoria. Si se omite, Turtle intenta adjuntar el último
    /// prompt registrado del proyecto (best-effort, nunca inventado).
    pub prompt: Option<String>,
}

/// Argumentos de `memory_importance`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportanciaArgs {
    /// Identificador de la memoria.
    pub id: String,
    /// Importancia: pinned, normal o ephemeral.
    pub importancia: String,
}

/// Salida genérica de confirmación.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OkSalida {
    pub ok: bool,
}

/// Salida de `memory_save`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GuardarSalida {
    /// Identificador asignado a la memoria.
    pub id: String,
    /// Posibles duplicados o conflictos (por relevancia FTS5) para que el agente los juzgue y,
    /// si corresponde, registre una relación con `relation_add` (RF-CNF-01/03).
    pub candidatos: Vec<FilaIndice>,
}

/// Argumentos de `memory_search`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuscarArgs {
    /// Consulta en lenguaje natural o términos clave.
    pub consulta: String,
    /// Limita la búsqueda a un proyecto.
    pub proyecto: Option<String>,
    /// Presupuesto de tokens del índice devuelto (por defecto 2000).
    pub presupuesto: Option<usize>,
    /// Perfil de verbosidad: indice (por defecto), compacto o completo (RF-REC-04).
    pub verbosidad: Option<String>,
}

/// Argumentos de `context_get`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextoArgs {
    /// Proyecto de la sesión. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Descripción de la tarea en curso; guía qué memorias traer. Si se omite, devuelve las recientes.
    pub tarea: Option<String>,
    /// Presupuesto de tokens del contexto (por defecto 2000).
    pub presupuesto: Option<usize>,
}

/// Argumentos de `memory_get`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecuperarArgs {
    /// Identificador de la memoria a recuperar.
    pub id: String,
}

/// Argumentos de `session_start`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SesionIniciarArgs {
    /// Proyecto de la sesión. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Tarea declarada que guía el contexto inicial.
    pub tarea: Option<String>,
    /// Rótulo del agente (rol o dominio).
    pub agente: Option<String>,
    /// Rama de git en la que trabaja el agente.
    pub rama: Option<String>,
    /// Presupuesto de tokens del contexto inicial (por defecto 2000).
    pub presupuesto: Option<usize>,
}

/// Argumentos de `session_close`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SesionCerrarArgs {
    /// Identificador de la sesión a cerrar.
    pub id: String,
    /// Resumen de lo realizado. Si se omite, se genera uno local.
    pub resumen: Option<String>,
}

/// Salida de `session_start`: id de la sesión más el contexto inicial.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SesionIniciadaSalida {
    /// Identificador asignado a la sesión.
    pub id: String,
    /// Memorias relevantes para arrancar la sesión.
    pub contexto: IndiceSalida,
    /// Mensajes pendientes entregados al agente (relevos/handoffs).
    pub mensajes: Vec<MensajeSalida>,
}

/// Salida de `session_close`: la sesión ya cerrada.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SesionSalida {
    pub id: String,
    pub proyecto: String,
    pub tarea: Option<String>,
    pub agente: Option<String>,
    pub resumen: Option<String>,
    pub estado: String,
    /// Inicio de la sesión en epoch ms UTC.
    pub iniciada_en: i64,
    /// Cierre de la sesión en epoch ms UTC, si ya terminó.
    pub cerrada_en: Option<i64>,
}

/// Una fila del índice barato devuelta por la búsqueda (sin contenido completo).
#[derive(Debug, Serialize, JsonSchema)]
pub struct FilaIndice {
    /// Identificador de la memoria.
    pub id: String,
    /// Título de la memoria.
    pub titulo: String,
    /// Tipo de la memoria.
    pub tipo: String,
    /// Resumen de una línea, si existe.
    pub resumen: Option<String>,
    /// `true` si la memoria está marcada para revisión (contexto añejo): verifica antes de confiar.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub needs_review: bool,
    /// Cuerpo según la verbosidad: extracto (compacto) o contenido completo (completo).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cuerpo: Option<String>,
}

/// Salida de `memory_search` y `context_get`: índice recortado al presupuesto de tokens.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndiceSalida {
    /// Filas del índice en orden de relevancia.
    pub resultados: Vec<FilaIndice>,
    /// Tokens estimados que ocupan las filas devueltas.
    pub tokens_total: usize,
    /// `true` si se omitieron resultados por exceder el presupuesto.
    pub truncado: bool,
}

/// Argumentos de `memory_timeline`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TimelineArgs {
    /// Identificador de la memoria de partida.
    pub id: String,
}

/// Salida de `memory_get`: la memoria completa.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MemoriaSalida {
    pub id: String,
    pub proyecto: String,
    pub tipo: String,
    pub titulo: String,
    pub contenido: String,
    pub resumen: Option<String>,
    pub que: Option<String>,
    pub porque: Option<String>,
    pub donde: Option<String>,
    pub aprendido: Option<String>,
    /// Alcance: project o personal.
    pub scope: String,
    /// Clave de tema evolutivo, si la memoria pertenece a un tema.
    pub topic_key: Option<String>,
    /// Estado del ciclo de vida: active o needs_review.
    pub review_state: String,
    /// Prompt del usuario que originó la memoria, si se capturó.
    pub prompt: Option<String>,
}

/// Interpreta el tipo recibido; `None` se mapea a `note` (RF-MEM-01).
fn parse_tipo(s: Option<&str>) -> Result<MemoryKind, ErrorData> {
    match s {
        None => Ok(MemoryKind::Note),
        Some(t) => MemoryKind::parse(t).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "Tipo de memoria desconocido: {t}. Use decision, architecture, correction, \
                     convention o note."
                ),
                None,
            )
        }),
    }
}

/// Interpreta el alcance; `None` usa el por defecto (project). Paridad con `project|personal`.
fn parse_scope(s: Option<&str>) -> Result<Scope, ErrorData> {
    match s.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(Scope::default()),
        Some(v) => Scope::parse(v).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("Alcance desconocido: {v}. Use project o personal."),
                None,
            )
        }),
    }
}

/// Normaliza un texto opcional: `None`/vacío → `None`; si no, recorta espacios.
fn limpiar_opt(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Interpreta el perfil de verbosidad; `None` usa el perfil por defecto (índice) (RF-REC-04).
fn parse_verbosidad(s: Option<&str>) -> Result<Verbosidad, ErrorData> {
    match s {
        None => Ok(Verbosidad::default()),
        Some(v) => Verbosidad::parse(v).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("Verbosidad desconocida: {v}. Use indice, compacto o completo."),
                None,
            )
        }),
    }
}

/// Interpreta la importancia opcional; `None` deja la que tenga (RF-MEM-04).
fn parse_importancia(s: Option<&str>) -> Result<Option<Importance>, ErrorData> {
    match s.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => Importance::parse(v).map(Some).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("Importancia desconocida: {v}. Use pinned, normal o ephemeral."),
                None,
            )
        }),
    }
}

/// Convierte un error de la capa de datos en un error MCP interno, sin filtrar el tipo concreto.
fn error_interno(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

fn a_fila_indice(r: MemoryIndexRow) -> FilaIndice {
    FilaIndice {
        id: r.id,
        titulo: r.title,
        tipo: r.kind.as_str().to_string(),
        resumen: r.summary,
        needs_review: r.needs_review,
        cuerpo: r.cuerpo,
    }
}

fn a_indice_salida(o: SearchOutcome) -> IndiceSalida {
    IndiceSalida {
        resultados: o.rows.into_iter().map(a_fila_indice).collect(),
        tokens_total: o.total_tokens,
        truncado: o.truncated,
    }
}

fn a_sesion_salida(s: Session) -> SesionSalida {
    SesionSalida {
        id: s.id,
        proyecto: s.project,
        tarea: s.task,
        agente: s.agent_id,
        resumen: s.summary,
        estado: s.status.as_str().to_string(),
        iniciada_en: s.started_at,
        cerrada_en: s.ended_at,
    }
}

/// Argumentos de `sessions_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SesionesArgs {
    /// Limita a un proyecto; si se omite, lista todas.
    pub proyecto: Option<String>,
}

/// Un conteo etiquetado (clave → cantidad).
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConteoSalida {
    pub clave: String,
    pub cantidad: i64,
}

/// Salida de `stats` (RF-DIA-01).
#[derive(Debug, Serialize, JsonSchema)]
pub struct EstadisticasSalida {
    pub totales: Vec<ConteoSalida>,
    pub por_proyecto: Vec<ConteoSalida>,
    pub por_tipo: Vec<ConteoSalida>,
}

fn a_estadisticas_salida(e: Estadisticas) -> EstadisticasSalida {
    let conv = |v: Vec<(String, i64)>| {
        v.into_iter()
            .map(|(clave, cantidad)| ConteoSalida { clave, cantidad })
            .collect()
    };
    EstadisticasSalida {
        totales: conv(e.totales),
        por_proyecto: conv(e.por_proyecto),
        por_tipo: conv(e.por_tipo),
    }
}

/// Salida de `sessions_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SesionesSalida {
    pub sesiones: Vec<SesionSalida>,
}

/// Argumentos de `agents_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentesArgs {
    /// Limita la lista a un proyecto. Si se omite, lista todos.
    pub proyecto: Option<String>,
}

/// Salida de `agents_list`: la lista de agentes (objeto en la raíz, como pide el esquema MCP).
#[derive(Debug, Serialize, JsonSchema)]
pub struct AgentesSalida {
    pub agentes: Vec<AgenteSalida>,
}

/// Un agente registrado dentro de `agents_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AgenteSalida {
    pub proyecto: String,
    pub rotulo: String,
    pub estado: String,
    pub tarea: Option<String>,
    pub rama: Option<String>,
    /// Última vez que se lo vio, en epoch ms UTC.
    pub visto_en: i64,
}

fn a_agente_salida(a: Agent) -> AgenteSalida {
    AgenteSalida {
        proyecto: a.project,
        rotulo: a.label,
        estado: a.status.as_str().to_string(),
        tarea: a.task,
        rama: a.branch,
        visto_en: a.last_seen_at,
    }
}

/// Argumentos de `events_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EventosArgs {
    /// Limita la lista a un proyecto. Si se omite, lista todos.
    pub proyecto: Option<String>,
}

/// Salida de `events_list`: el feed de actividad (objeto en la raíz, como pide el esquema MCP).
#[derive(Debug, Serialize, JsonSchema)]
pub struct EventosSalida {
    pub eventos: Vec<EventoSalida>,
}

/// Un evento del bus de actividad dentro de `events_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EventoSalida {
    pub proyecto: String,
    pub agente: Option<String>,
    /// Tipo de evento (p. ej. memory_saved, session_started).
    pub tipo: String,
    /// Id del objeto afectado, si aplica.
    pub objetivo: Option<String>,
    pub resumen: Option<String>,
    /// Momento del evento, en epoch ms UTC.
    pub cuando: i64,
}

fn a_evento_salida(e: Event) -> EventoSalida {
    EventoSalida {
        proyecto: e.project,
        agente: e.agent,
        tipo: e.kind.as_str().to_string(),
        objetivo: e.target_id,
        resumen: e.summary,
        cuando: e.created_at,
    }
}

/// Argumentos de `message_send`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MensajeArgs {
    /// Proyecto del mensaje. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Rótulo del remitente, si se conoce.
    pub de: Option<String>,
    /// Rótulo del destinatario. Si se omite, es difusión a todo el proyecto.
    pub para: Option<String>,
    /// Cuerpo del mensaje.
    pub cuerpo: String,
}

/// Salida de `message_send`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MensajeEnviadoSalida {
    /// Identificador asignado al mensaje.
    pub id: String,
}

/// Argumentos de `inbox`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BandejaArgs {
    /// Proyecto de la bandeja. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Rótulo del agente cuya bandeja se lee.
    pub agente: String,
    /// Si es `true` (por defecto), solo los mensajes pendientes.
    pub solo_pendientes: Option<bool>,
}

/// Salida de `inbox`: la lista de mensajes (objeto en la raíz, como pide el esquema MCP).
#[derive(Debug, Serialize, JsonSchema)]
pub struct BandejaSalida {
    pub mensajes: Vec<MensajeSalida>,
}

/// Un mensaje de la bandeja.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MensajeSalida {
    pub de: Option<String>,
    pub para: Option<String>,
    pub cuerpo: String,
    /// Momento de envío, en epoch ms UTC.
    pub cuando: i64,
}

fn a_mensaje_salida(m: Message) -> MensajeSalida {
    MensajeSalida {
        de: m.from_agent,
        para: m.to_agent,
        cuerpo: m.body,
        cuando: m.created_at,
    }
}

/// Argumentos de `relation_add`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelacionArgs {
    /// Memoria origen (id).
    pub de: String,
    /// Memoria destino (id).
    pub a: String,
    /// Tipo: replaces, conflicts, relates o duplicate.
    pub tipo: String,
    /// Nota opcional que explica el porqué.
    pub nota: Option<String>,
}

/// Salida de `relation_add`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RelacionAgregadaSalida {
    pub id: String,
}

/// Argumentos de `relations_list`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RelacionesArgs {
    /// Memoria cuyas relaciones se listan (id).
    pub id: String,
}

/// Salida de `relations_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RelacionesSalida {
    pub relaciones: Vec<RelacionSalida>,
}

/// Una relación entre memorias.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RelacionSalida {
    pub de: String,
    pub a: String,
    pub tipo: String,
    pub nota: Option<String>,
    pub cuando: i64,
}

/// Argumentos de `memory_compare`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompararArgs {
    pub id_a: String,
    pub id_b: String,
}

/// Salida de `memory_compare`: las dos memorias completas.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CompararSalida {
    pub a: MemoriaSalida,
    pub b: MemoriaSalida,
}

fn a_relacion_salida(r: Relation) -> RelacionSalida {
    RelacionSalida {
        de: r.from_id,
        a: r.to_id,
        tipo: r.kind.as_str().to_string(),
        nota: r.note,
        cuando: r.created_at,
    }
}

/// Argumentos de `skills_import`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillsImportArgs {
    /// Rutas a escanear; vacío = directorios por defecto (skills/, agents/ y ~/.claude/...).
    #[serde(default)]
    pub rutas: Vec<String>,
    /// Proyecto a asignar a las skills locales (por defecto: global).
    pub proyecto: Option<String>,
}

/// Salida de `skills_import`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillsImportSalida {
    pub importadas: usize,
    pub fuentes: Vec<String>,
    /// Avisos de la ingesta (p. ej. skills de comportamiento grandes, RF-SKL-08).
    pub avisos: Vec<String>,
}

/// Argumentos de `skill_activate`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillActivarArgs {
    /// Identificador de la skill.
    pub id: String,
    /// Intensidad: off, lite, full o ultra.
    pub intensidad: String,
}

/// Argumentos de `skills_search`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillsBuscarArgs {
    pub consulta: String,
    pub proyecto: Option<String>,
    pub limite: Option<u32>,
}

/// Salida de `skills_search`: índice barato.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillsIndiceSalida {
    pub resultados: Vec<FilaSkill>,
}

/// Fila del índice de skills (sin contenido completo).
#[derive(Debug, Serialize, JsonSchema)]
pub struct FilaSkill {
    pub id: String,
    pub nombre: String,
    pub tipo: String,
    pub cuando_usar: Option<String>,
}

/// Argumentos de `skill_get`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillGetArgs {
    pub id: String,
}

/// Salida de `skill_get`: la skill completa.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillSalida {
    pub id: String,
    pub proyecto: String,
    pub nombre: String,
    pub tipo: String,
    pub cuando_usar: Option<String>,
    pub etiquetas: Option<String>,
    pub origen: Option<String>,
    pub contenido: String,
}

/// Argumentos de `skill_save`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillGuardarArgs {
    pub nombre: String,
    pub contenido: String,
    /// behavior, knowledge, tool o agent (por defecto: knowledge).
    pub tipo: Option<String>,
    pub cuando_usar: Option<String>,
    pub etiquetas: Option<String>,
    pub proyecto: Option<String>,
}

/// Salida de `skill_save`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillGuardadaSalida {
    pub id: String,
}

/// Salida de `skills_seed`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillsSeedSalida {
    pub cargadas: usize,
}

/// Argumentos de `checkpoint_save`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckpointGuardarArgs {
    /// Proyecto del checkpoint. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Qué estás haciendo y los próximos pasos.
    pub contenido: String,
}

/// Salida de `checkpoint_save`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CheckpointGuardadoSalida {
    pub id: String,
}

/// Argumentos de `checkpoint_get`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckpointGetArgs {
    /// Proyecto del checkpoint. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
}

/// Salida de `checkpoint_get`: el último checkpoint, si lo hay.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CheckpointSalida {
    pub contenido: Option<String>,
    /// Marca de tiempo del checkpoint, en epoch ms UTC.
    pub cuando: Option<i64>,
}

/// Argumentos de `memory_review`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RevisionArgs {
    /// Acción: `list` (por defecto) o `mark_reviewed`.
    pub accion: Option<String>,
    /// Proyecto (para `list`). Si se omite, se autodetecta.
    pub proyecto: Option<String>,
    /// Id de la memoria (requerido para `mark_reviewed`).
    pub id: Option<String>,
}

/// Salida de `memory_review`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RevisionSalida {
    /// Memorias marcadas para revisión (en `list`).
    pub revisar: Vec<FilaIndice>,
    /// Confirmación de `mark_reviewed`, si aplica.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

/// Argumentos de `memory_save_prompt`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GuardarPromptArgs {
    /// Proyecto. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Identificador de sesión, si se conoce.
    pub sesion: Option<String>,
    /// Texto del prompt del usuario.
    pub prompt: String,
}

/// Argumentos de `suggest_topic_key`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SugerirTopicArgs {
    /// Título de la memoria.
    pub titulo: String,
    /// Contenido (opcional, como respaldo si el título es pobre).
    pub contenido: Option<String>,
    /// Tipo de memoria (da el `area` de la clave): decision, architecture, etc.
    pub tipo: Option<String>,
}

/// Salida de `suggest_topic_key`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopicKeySalida {
    /// Clave sugerida, o `None` si no hay texto útil.
    pub topic_key: Option<String>,
}

fn a_checkpoint_salida(c: Option<Checkpoint>) -> CheckpointSalida {
    CheckpointSalida {
        contenido: c.as_ref().map(|x| x.content.clone()),
        cuando: c.as_ref().map(|x| x.created_at),
    }
}

/// Salida de `memory_history`: las versiones archivadas de una memoria de tema (recientes primero).
#[derive(Debug, Serialize, JsonSchema)]
pub struct HistorialSalida {
    /// Id de la memoria viva cuyas versiones se listan.
    pub memory_id: String,
    pub versiones: Vec<VersionSalida>,
}

/// Una versión histórica de una memoria de tema, con su intervalo de validez.
#[derive(Debug, Serialize, JsonSchema)]
pub struct VersionSalida {
    pub id: String,
    pub titulo: String,
    pub resumen: Option<String>,
    pub contenido: String,
    /// Epoch ms desde cuándo era válida esta versión.
    pub valido_desde: i64,
    /// Epoch ms hasta cuándo fue válida (cuándo la reemplazó otra).
    pub valido_hasta: i64,
}

/// Argumentos de `memory_duplicates`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DuplicadosArgs {
    /// Proyecto a escanear. Si se omite, se autodetecta del directorio actual.
    pub proyecto: Option<String>,
    /// Cuántas memorias recientes escanear (cota de latencia; por defecto 100).
    pub limite: Option<u32>,
}

/// Salida de `memory_duplicates`: pares candidatos a consolidar, del solapamiento más fuerte al más débil.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DuplicadosSalida {
    pub pares: Vec<ParDuplicado>,
}

/// Un par de memorias candidatas a fusionar.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ParDuplicado {
    pub a_id: String,
    pub a_titulo: String,
    pub b_id: String,
    pub b_titulo: String,
    /// Puntaje bm25 del solapamiento (más negativo = más fuerte).
    pub score: f64,
}

fn a_par_duplicado(d: DupCandidato) -> ParDuplicado {
    ParDuplicado {
        a_id: d.a_id,
        a_titulo: d.a_titulo,
        b_id: d.b_id,
        b_titulo: d.b_titulo,
        score: d.score,
    }
}

fn a_version_salida(v: MemoryVersion) -> VersionSalida {
    VersionSalida {
        id: v.id,
        titulo: v.title,
        resumen: v.summary,
        contenido: v.content,
        valido_desde: v.valid_from,
        valido_hasta: v.valid_to,
    }
}

fn a_fila_skill(r: SkillIndexRow) -> FilaSkill {
    FilaSkill {
        id: r.id,
        nombre: r.name,
        tipo: r.kind.as_str().to_string(),
        cuando_usar: r.when_to_use,
    }
}

fn a_skill_salida(s: Skill) -> SkillSalida {
    SkillSalida {
        id: s.id,
        proyecto: s.project,
        nombre: s.name,
        tipo: s.kind.as_str().to_string(),
        cuando_usar: s.when_to_use,
        etiquetas: s.tags,
        origen: s.source,
        contenido: s.content,
    }
}

fn a_memoria_salida(m: Memory) -> MemoriaSalida {
    MemoriaSalida {
        id: m.id,
        proyecto: m.project,
        tipo: m.kind.as_str().to_string(),
        titulo: m.title,
        contenido: m.content,
        resumen: m.summary,
        que: m.what,
        porque: m.why,
        donde: m.where_,
        aprendido: m.learned,
        scope: m.scope.as_str().to_string(),
        topic_key: m.topic_key,
        review_state: m.review_state.as_str().to_string(),
        prompt: m.prompt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use turtle_data::Db;

    fn servidor() -> TurtleMcp {
        TurtleMcp::new(MemoryService::new(Db::open_in_memory().unwrap()))
    }

    fn guardar_args(titulo: &str, contenido: &str, tipo: Option<&str>) -> GuardarArgs {
        GuardarArgs {
            proyecto: Some("turtle".into()),
            tipo: tipo.map(str::to_string),
            titulo: titulo.into(),
            contenido: contenido.into(),
            resumen: Some("resumen breve".into()),
            que: None,
            porque: None,
            donde: None,
            aprendido: None,
            importancia: None,
            scope: None,
            topic_key: None,
            prompt: None,
        }
    }

    #[test]
    fn proyecto_o_detectar_usa_el_dado_o_autodetecta() {
        // Explícito y no vacío: se respeta tal cual.
        assert_eq!(
            proyecto_o_detectar(Some("mi-proyecto".into())),
            "mi-proyecto"
        );
        // Vacío o ausente: cae en la autodetección, que nunca devuelve vacío.
        assert!(!proyecto_o_detectar(Some("   ".into())).is_empty());
        assert!(!proyecto_o_detectar(None).is_empty());
    }

    #[tokio::test]
    async fn guardar_sin_proyecto_autodetecta() {
        let s = servidor();
        let mut args = guardar_args(
            "Sin proyecto",
            "Se autodetecta del directorio.",
            Some("note"),
        );
        args.proyecto = None;
        let id = s.memory_save(Parameters(args)).await.unwrap().0.id;
        let mem = s
            .memory_get(Parameters(RecuperarArgs { id: id.clone() }))
            .await
            .unwrap()
            .0;
        assert_eq!(mem.id, id);
        assert!(!mem.proyecto.is_empty(), "el proyecto se autodetectó");
    }

    #[tokio::test]
    async fn guardar_y_recuperar_por_mcp() {
        let s = servidor();
        let id = s
            .memory_save(Parameters(guardar_args(
                "Usar rmcp",
                "El servidor MCP usa rmcp sobre stdio.",
                Some("decision"),
            )))
            .await
            .unwrap()
            .0
            .id;
        let mem = s
            .memory_get(Parameters(RecuperarArgs { id: id.clone() }))
            .await
            .unwrap()
            .0;
        assert_eq!(mem.id, id);
        assert_eq!(mem.titulo, "Usar rmcp");
        assert_eq!(mem.tipo, "decision");
    }

    #[tokio::test]
    async fn buscar_devuelve_indice_sin_contenido() {
        let s = servidor();
        s.memory_save(Parameters(guardar_args(
            "Sobre ratatui",
            "La TUI usa ratatui para el mission control.",
            None,
        )))
        .await
        .unwrap();
        let salida = s
            .memory_search(Parameters(BuscarArgs {
                consulta: "ratatui".into(),
                proyecto: None,
                presupuesto: Some(1_000),
                verbosidad: None,
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(salida.resultados.len(), 1);
        assert_eq!(salida.resultados[0].titulo, "Sobre ratatui");
        assert_eq!(salida.resultados[0].tipo, "note");
        assert!(salida.tokens_total <= 1_000);
    }

    #[tokio::test]
    async fn buscar_con_proyecto_vacio_no_filtra() {
        // Footgun: un cliente que manda `proyecto: ""` no debe filtrar por proyecto vacío (que no
        // matchea nada), sino comportarse como "todos los proyectos" (igual que omitirlo).
        let s = servidor();
        s.memory_save(Parameters(guardar_args(
            "Sobre ratatui",
            "La búsqueda con proyecto vacío igual encuentra esto.",
            None,
        )))
        .await
        .unwrap();
        let salida = s
            .memory_search(Parameters(BuscarArgs {
                consulta: "ratatui".into(),
                proyecto: Some("   ".into()),
                presupuesto: Some(1_000),
                verbosidad: None,
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(salida.resultados.len(), 1, "proyecto vacío = sin filtro");
    }

    #[test]
    fn proyecto_filtro_normaliza_vacio_a_none() {
        assert_eq!(proyecto_filtro(None), None);
        assert_eq!(proyecto_filtro(Some("".into())), None);
        assert_eq!(proyecto_filtro(Some("   ".into())), None);
        assert_eq!(
            proyecto_filtro(Some(" turtle ".into())),
            Some("turtle".into())
        );
    }

    #[tokio::test]
    async fn recuperar_inexistente_es_error_de_parametros() {
        let s = servidor();
        let err = s
            .memory_get(Parameters(RecuperarArgs {
                id: "no-existe".into(),
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn tipo_desconocido_es_rechazado() {
        let s = servidor();
        let err = s
            .memory_save(Parameters(guardar_args("x", "y", Some("inventado"))))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn ciclo_de_sesion_por_mcp() {
        let s = servidor();
        let iniciada = s
            .session_start(Parameters(SesionIniciarArgs {
                proyecto: Some("turtle".into()),
                tarea: Some("trabajar en sesiones".into()),
                agente: Some("dev".into()),
                rama: Some("main".into()),
                presupuesto: Some(1_000),
            }))
            .await
            .unwrap()
            .0;
        assert!(!iniciada.id.is_empty());

        s.memory_save(Parameters(guardar_args(
            "Sobre sesiones",
            "Implementamos M4.",
            Some("note"),
        )))
        .await
        .unwrap();

        let cerrada = s
            .session_close(Parameters(SesionCerrarArgs {
                id: iniciada.id.clone(),
                resumen: None,
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(cerrada.id, iniciada.id);
        assert_eq!(cerrada.estado, "closed");
        assert!(cerrada.cerrada_en.is_some());
        assert!(cerrada.resumen.unwrap().contains("Sobre sesiones"));
    }

    #[tokio::test]
    async fn cerrar_sesion_inexistente_es_error() {
        let s = servidor();
        let err = s
            .session_close(Parameters(SesionCerrarArgs {
                id: "no-existe".into(),
                resumen: None,
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn enviar_y_leer_mensaje_por_mcp() {
        let s = servidor();
        s.message_send(Parameters(MensajeArgs {
            proyecto: Some("turtle".into()),
            de: Some("frontend".into()),
            para: Some("backend".into()),
            cuerpo: "revisa el endpoint".into(),
        }))
        .await
        .unwrap();
        let bandeja = s
            .inbox(Parameters(BandejaArgs {
                proyecto: Some("turtle".into()),
                agente: "backend".into(),
                solo_pendientes: None,
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(bandeja.mensajes.len(), 1);
        assert_eq!(bandeja.mensajes[0].cuerpo, "revisa el endpoint");
        assert_eq!(bandeja.mensajes[0].de.as_deref(), Some("frontend"));
    }

    #[tokio::test]
    async fn guardar_devuelve_candidatos_y_se_registra_relacion() {
        let s = servidor();
        let a = s
            .memory_save(Parameters(guardar_args(
                "Usar rmcp",
                "El MCP usa rmcp.",
                Some("decision"),
            )))
            .await
            .unwrap()
            .0;
        let b = s
            .memory_save(Parameters(guardar_args(
                "Usar rmcp en el servidor",
                "rmcp sobre stdio.",
                Some("decision"),
            )))
            .await
            .unwrap()
            .0;
        // Al guardar b, a aparece como candidato (comparten términos).
        assert!(b.candidatos.iter().any(|c| c.id == a.id));

        s.relation_add(Parameters(RelacionArgs {
            de: b.id.clone(),
            a: a.id.clone(),
            tipo: "duplicate".into(),
            nota: None,
        }))
        .await
        .unwrap();
        let rels = s
            .relations_list(Parameters(RelacionesArgs { id: a.id.clone() }))
            .await
            .unwrap()
            .0;
        assert_eq!(rels.relaciones.len(), 1);
        assert_eq!(rels.relaciones[0].tipo, "duplicate");

        let cmp = s
            .memory_compare(Parameters(CompararArgs {
                id_a: a.id.clone(),
                id_b: b.id.clone(),
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(cmp.a.id, a.id);
        assert_eq!(cmp.b.id, b.id);
    }

    #[tokio::test]
    async fn relacionar_memorias_inexistentes_es_rechazado() {
        // Integridad referencial del contrato: relation_add no debe crear relaciones huérfanas
        // entre ids que no existen (ni vacíos). Debe responder INVALID_PARAMS, no crear basura.
        let s = servidor();
        let real = s
            .memory_save(Parameters(guardar_args("real", "existe", Some("note"))))
            .await
            .unwrap()
            .0
            .id;

        // Origen inexistente.
        let err = s
            .relation_add(Parameters(RelacionArgs {
                de: "01NOEXISTE".into(),
                a: real.clone(),
                tipo: "relates".into(),
                nota: None,
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);

        // Destino inexistente.
        let err = s
            .relation_add(Parameters(RelacionArgs {
                de: real.clone(),
                a: "01NOEXISTE".into(),
                tipo: "relates".into(),
                nota: None,
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);

        // Ids vacíos.
        let err = s
            .relation_add(Parameters(RelacionArgs {
                de: "".into(),
                a: "".into(),
                tipo: "duplicate".into(),
                nota: None,
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);

        // Y no quedó ninguna relación colgando de la memoria real.
        let rels = s
            .relations_list(Parameters(RelacionesArgs { id: real }))
            .await
            .unwrap()
            .0;
        assert!(
            rels.relaciones.is_empty(),
            "no se crearon relaciones huérfanas"
        );
    }

    #[tokio::test]
    async fn skills_guardar_buscar_y_cargar() {
        let s = servidor();
        let g = s
            .skill_save(Parameters(SkillGuardarArgs {
                nombre: "ponytail".into(),
                contenido: "Skill para manejar ramas de git.".into(),
                tipo: Some("tool".into()),
                cuando_usar: Some("al cambiar de rama".into()),
                etiquetas: Some("git".into()),
                proyecto: None,
            }))
            .await
            .unwrap()
            .0;
        let hits = s
            .skills_search(Parameters(SkillsBuscarArgs {
                consulta: "ramas git".into(),
                proyecto: None,
                limite: None,
            }))
            .await
            .unwrap()
            .0;
        assert!(hits
            .resultados
            .iter()
            .any(|f| f.id == g.id && f.tipo == "tool"));
        let full = s
            .skill_get(Parameters(SkillGetArgs { id: g.id.clone() }))
            .await
            .unwrap()
            .0;
        assert_eq!(full.nombre, "ponytail");
        assert!(full.contenido.contains("ramas de git"));
    }

    #[tokio::test]
    async fn memory_save_con_topic_key_hace_upsert() {
        let s = servidor();
        let con_tema = |titulo: &str| GuardarArgs {
            topic_key: Some("ui/tema".into()),
            ..guardar_args(titulo, "contenido", Some("decision"))
        };
        let id1 = s
            .memory_save(Parameters(con_tema("v1")))
            .await
            .unwrap()
            .0
            .id;
        let id2 = s
            .memory_save(Parameters(con_tema("v2")))
            .await
            .unwrap()
            .0
            .id;
        assert_eq!(id1, id2, "mismo tema actualiza, no duplica");
        let m = s
            .memory_get(Parameters(RecuperarArgs { id: id1.clone() }))
            .await
            .unwrap()
            .0;
        assert_eq!(m.titulo, "v2");
        assert_eq!(m.topic_key.as_deref(), Some("ui/tema"));
    }

    #[tokio::test]
    async fn memory_save_personal_y_visible_cross_proyecto() {
        let s = servidor();
        let mut args = guardar_args("Estilo neutro", "Español latino neutro siempre.", None);
        args.proyecto = Some("proyA".into());
        args.scope = Some("personal".into());
        s.memory_save(Parameters(args)).await.unwrap();
        // Búsqueda filtrando por otro proyecto: la personal aparece.
        let r = s
            .memory_search(Parameters(BuscarArgs {
                consulta: "español".into(),
                proyecto: Some("proyB".into()),
                presupuesto: Some(2_000),
                verbosidad: None,
            }))
            .await
            .unwrap()
            .0;
        assert!(r.resultados.iter().any(|x| x.titulo == "Estilo neutro"));
    }

    #[tokio::test]
    async fn memory_review_lista_y_marca() {
        let s = servidor();
        let id = s
            .memory_save(Parameters(guardar_args("Vieja", "contenido", None)))
            .await
            .unwrap()
            .0
            .id;
        // Escalonar a frío via servicio (cortes futuros) marca needs_review.
        s.service.lock().await.escalonar("turtle", -1, -1).unwrap();
        let lista = s
            .memory_review(Parameters(RevisionArgs {
                accion: Some("list".into()),
                proyecto: Some("turtle".into()),
                id: None,
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(lista.revisar.len(), 1);
        assert!(lista.revisar[0].needs_review);

        // Marcar revisada.
        let ok = s
            .memory_review(Parameters(RevisionArgs {
                accion: Some("mark_reviewed".into()),
                proyecto: None,
                id: Some(id.clone()),
            }))
            .await
            .unwrap()
            .0;
        assert_eq!(ok.ok, Some(true));
        // Ya no hay nada por revisar.
        let lista2 = s
            .memory_review(Parameters(RevisionArgs {
                accion: None,
                proyecto: Some("turtle".into()),
                id: None,
            }))
            .await
            .unwrap()
            .0;
        assert!(lista2.revisar.is_empty());

        // mark_reviewed sin id es error de parámetros.
        let err = s
            .memory_review(Parameters(RevisionArgs {
                accion: Some("mark_reviewed".into()),
                proyecto: None,
                id: None,
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn memory_save_prompt_se_adjunta_en_el_siguiente_save() {
        let s = servidor();
        s.memory_save_prompt(Parameters(GuardarPromptArgs {
            proyecto: Some("turtle".into()),
            sesion: None,
            prompt: "implementá la feature".into(),
        }))
        .await
        .unwrap();
        let id = s
            .memory_save(Parameters(guardar_args("Hecho", "lo implementé", None)))
            .await
            .unwrap()
            .0
            .id;
        let m = s
            .memory_get(Parameters(RecuperarArgs { id }))
            .await
            .unwrap()
            .0;
        assert_eq!(m.prompt.as_deref(), Some("implementá la feature"));

        // Prompt vacío es rechazado.
        let err = s
            .memory_save_prompt(Parameters(GuardarPromptArgs {
                proyecto: None,
                sesion: None,
                prompt: "   ".into(),
            }))
            .await
            .err()
            .unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn suggest_topic_key_tool_devuelve_clave() {
        let s = servidor();
        let out = s
            .suggest_topic_key(Parameters(SugerirTopicArgs {
                titulo: "Diseño del esquema de datos".into(),
                contenido: None,
                tipo: Some("architecture".into()),
            }))
            .await
            .unwrap()
            .0;
        assert!(out.topic_key.unwrap().starts_with("architecture/"));
    }

    #[tokio::test]
    async fn scope_desconocido_es_rechazado() {
        let s = servidor();
        let mut args = guardar_args("x", "y", None);
        args.scope = Some("inventado".into());
        let err = s.memory_save(Parameters(args)).await.err().unwrap();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn get_info_anuncia_herramientas() {
        let s = servidor();
        let info = s.get_info();
        assert!(info.capabilities.tools.is_some());
        assert_eq!(info.server_info.name, "turtle");
        assert!(info.instructions.is_some());
        // Treinta: memoria (8, incl. memory_history y memory_duplicates) +
        // review/save_prompt/suggest_topic_key (3) = 11, sesión (3), checkpoint (2), stats (1),
        // agentes (1), eventos (1), mensajes (2), relaciones (3), skills (6).
        assert_eq!(s.tool_router.list_all().len(), 30);
    }

    #[test]
    fn perfil_minimo_expone_solo_el_nucleo() {
        let s = TurtleMcp::con_perfil(
            MemoryService::new(Db::open_in_memory().unwrap()),
            Perfil::Minimo,
        );
        let nombres: Vec<String> = s
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        assert_eq!(nombres.len(), 6);
        assert!(nombres.contains(&"memory_search".to_string()));
        assert!(nombres.contains(&"skill_get".to_string()));
        assert!(!nombres.contains(&"session_start".to_string()));
        assert!(!nombres.contains(&"relation_add".to_string()));
        assert_eq!(Perfil::parse("mínimo"), Some(Perfil::Minimo));
        assert_eq!(Perfil::parse("?"), None);
    }
}
