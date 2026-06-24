//! `turtle-service` — Casos de uso y reglas de negocio.
//!
//! Único lugar que conoce las reglas del dominio: recuperación en dos etapas y
//! presupuesto de tokens (CC-2). MCP y CLI son adaptadores delgados sobre esta
//! capa. Ver arquitectura §2 y §5.

mod scan;
mod seeds;
pub use seeds::{
    modelo_persona, modelo_valido, personas, subagentes_claude, ModeloInfo, Persona,
    SubagenteClaude, MODELOS_CLAUDE,
};

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use turtle_core::agent::{Agent, NewAgent};
use turtle_core::checkpoint::Checkpoint;
use turtle_core::event::{Event, EventKind, NewEvent};
use turtle_core::memory::{
    Importance, Memory, MemoryIndexRow, MemoryVersion, NewMemory, Tier, Verbosidad,
};
use turtle_core::message::{Message, NewMessage};
use turtle_core::relation::{NewRelation, Relation, RelationKind};
use turtle_core::session::{NewSession, Session, SessionStatus};
use turtle_core::skill::{Intensidad, NewSkill, Skill, SkillIndexRow, SkillKind};
use turtle_data::{rusqlite, Db};

/// Tamaño máximo recomendado (bytes) de una skill de comportamiento (RF-SKL-08).
const MAX_BYTES_COMPORTAMIENTO: usize = 4096;

/// Milisegundos en un día, para el escalonamiento por antigüedad (RF-TOK-02/03/05).
const DIA_MS: i64 = 24 * 60 * 60 * 1000;

/// Cantidad máxima de títulos de memorias que se enumeran en un resumen local de sesión.
const MAX_TITULOS_RESUMEN: usize = 5;

/// Cuántos candidatos a duplicado/conflicto se devuelven al guardar.
const MAX_CANDIDATOS_DUP: u32 = 5;

/// Cantidad máxima de candidatos que se piden a la capa de datos antes de aplicar el
/// presupuesto de tokens.
const MAX_CANDIDATES: u32 = 200;

/// Clave de `settings`: semántica opt-in prendida (`"1"`) o no.
const SEMANTIC_KEY: &str = "semantic_enabled";
/// Clave de `settings`: modelo de embeddings elegido.
const EMBED_MODEL_KEY: &str = "embed_model";
/// Constante de la fusión Reciprocal Rank Fusion (RRF). 60 es el valor clásico de la literatura.
const RRF_K: f64 = 60.0;
/// Umbral de coseno para proponer un par como duplicado por **significado** (pase semántico de
/// consolidación). Conservador, para surfacear solo solapamientos fuertes y no inundar de ruido.
const UMBRAL_DUP_SEM: f32 = 0.82;

/// Detecta el proyecto a partir de `cwd`: nombre de la raíz del repo git si la hay; si no, el
/// nombre de la carpeta; `default` como último recurso. Es la convención compartida por CLI, MCP
/// y hooks, para que todos resuelvan el mismo proyecto desde un mismo directorio.
pub fn proyecto_en(cwd: &Path) -> String {
    let mut dir: &Path = cwd;
    loop {
        if dir.join(".git").exists() {
            if let Some(nombre) = dir.file_name().and_then(|s| s.to_str()) {
                return nombre.to_string();
            }
        }
        match dir.parent() {
            Some(padre) => dir = padre,
            None => break,
        }
    }
    cwd.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "default".to_string())
}

/// Resuelve el proyecto activo cuando el llamador no lo provee: `$TURTLE_PROJECT` si está, si no
/// lo detecta del directorio actual con [`proyecto_en`]. Pensado para que las herramientas MCP no
/// obliguen al agente a repetir el proyecto en cada llamada (paridad con la autodetección de la CLI).
pub fn proyecto_actual() -> String {
    if let Some(p) = std::env::var("TURTLE_PROJECT")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        return p;
    }
    match std::env::current_dir() {
        Ok(d) => proyecto_en(&d),
        Err(_) => "default".to_string(),
    }
}

/// Servicio de memorias: orquesta la capa de datos aplicando las reglas de tokens.
pub struct MemoryService {
    db: Db,
}

/// Resultado de una búsqueda en modo índice, ya recortado al presupuesto de tokens.
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    /// Filas del índice barato (sin contenido completo), en orden de relevancia.
    pub rows: Vec<MemoryIndexRow>,
    /// Tokens estimados que ocupan las filas devueltas.
    pub total_tokens: usize,
    /// `true` si se omitieron resultados por exceder el presupuesto.
    pub truncated: bool,
}

/// Un par de memorias del mismo proyecto candidatas a fusionar (duplicado/solapamiento por FTS de
/// títulos). Turtle lo PROPONE; el agente decide y consolida (topic_key/relation_add/delete). Sin IA.
#[derive(Debug, Clone)]
pub struct DupCandidato {
    pub a_id: String,
    pub a_titulo: String,
    pub b_id: String,
    pub b_titulo: String,
    /// Puntaje bm25 del solapamiento (más negativo = más fuerte).
    pub score: f64,
}

/// Reporte de una ingesta de skills/agentes (RF-SKL-06).
#[derive(Debug, Clone)]
pub struct ImporteSkills {
    /// Cantidad de skills insertadas o actualizadas.
    pub importadas: usize,
    /// Directorios que existían y se leyeron.
    pub fuentes: Vec<PathBuf>,
    /// Avisos (p. ej. skills de comportamiento demasiado grandes, RF-SKL-08).
    pub avisos: Vec<String>,
}

/// Estado de un chequeo de salud (RF-DIA-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoChequeo {
    Ok,
    Aviso,
    Error,
}

/// Un chequeo individual del diagnóstico.
#[derive(Debug, Clone)]
pub struct Chequeo {
    pub nombre: String,
    pub estado: EstadoChequeo,
    pub detalle: String,
    /// Acción de reparación sugerida (no se aplica sola), si la hay (RF-DIA-03).
    pub reparacion: Option<String>,
}

/// Estadísticas de la base: totales y desgloses (RF-DIA-01).
#[derive(Debug, Clone)]
pub struct Estadisticas {
    pub totales: Vec<(String, i64)>,
    pub por_proyecto: Vec<(String, i64)>,
    pub por_tipo: Vec<(String, i64)>,
}

/// Reporte de salud de la base (RF-DIA-02).
#[derive(Debug, Clone)]
pub struct Diagnostico {
    pub chequeos: Vec<Chequeo>,
    /// Conteos informativos por entidad.
    pub stats: Vec<(String, i64)>,
}

impl Diagnostico {
    /// `true` si algún chequeo terminó en error.
    pub fn hay_errores(&self) -> bool {
        self.chequeos
            .iter()
            .any(|c| c.estado == EstadoChequeo::Error)
    }
}

/// Chequeo de un índice FTS5: compara las filas de la tabla base con las indexadas.
fn chequeo_fts(nombre: &str, base: i64, indice: i64) -> Chequeo {
    if base == indice {
        Chequeo {
            nombre: nombre.into(),
            estado: EstadoChequeo::Ok,
            detalle: format!("{base} en sincronía"),
            reparacion: None,
        }
    } else {
        Chequeo {
            nombre: nombre.into(),
            estado: EstadoChequeo::Error,
            detalle: format!("desincronizado: {base} filas vs {indice} indexadas"),
            reparacion: Some("turtle doctor --reparar (reconstruye el índice FTS)".into()),
        }
    }
}

impl MemoryService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Guarda una memoria nueva y devuelve su id (RF-MEM-01). Si no trae resumen, deriva uno por
    /// heurística local (RF-MEM-08). Registra el evento (RF-COM-01).
    ///
    /// Si la memoria trae `topic_key`, el guardado hace UPSERT por tema (no duplica; paridad con
    /// sistemas afines). Si NO trae `prompt`, intenta adjuntar el último prompt no consumido del proyecto
    /// (best-effort; nunca inventa texto). Si no hay, el guardado igual funciona sin prompt.
    pub fn save(&self, m: &NewMemory) -> rusqlite::Result<String> {
        let m = self.asegurar_prompt(asegurar_resumen(m));
        let id = self.db.insert_memory(&m)?;
        self.registrar(
            &m.project,
            None,
            EventKind::MemorySaved,
            Some(&id),
            Some(&m.title),
        )?;
        // Semántica opt-in: si está prendida, embebe la memoria (best-effort; nunca falla el guardado
        // ni lo bloquea más allá del timeout corto de Ollama; si Ollama no está, queda para el backfill).
        if self.semantica_activa() {
            self.embed_memoria(&id, &m.title, &m.content);
        }
        Ok(id)
    }

    /// Adjunta, si falta, el último prompt no consumido del proyecto (best-effort). Consume el
    /// prompt para no pegarlo a varias memorias del mismo turno. Nunca inventa texto.
    fn asegurar_prompt<'a>(&self, m: Cow<'a, NewMemory>) -> Cow<'a, NewMemory> {
        if m.prompt.as_deref().is_some_and(|p| !p.trim().is_empty()) {
            return m;
        }
        match self.db.take_last_prompt(&m.project) {
            Ok(Some(prompt)) => Cow::Owned(NewMemory {
                prompt: Some(prompt),
                ..m.into_owned()
            }),
            _ => m,
        }
    }

    /// Registra el prompt del usuario para un proyecto (best-effort). Lo invoca el hook
    /// prompt-submit para cerrar el círculo: un `save` posterior sin prompt lo adjunta.
    pub fn record_prompt(
        &self,
        project: &str,
        session_id: Option<&str>,
        prompt: &str,
    ) -> rusqlite::Result<()> {
        self.db.record_last_prompt(project, session_id, prompt)
    }

    /// Memorias marcadas para revisión de un proyecto, en índice (el listado de revisión).
    pub fn needs_review_list(
        &self,
        project: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        self.db.needs_review_index(project, limit)
    }

    /// Marca una memoria como revisada: vuelve a `active` y refresca su acceso (sistemas afines
    /// `mem_review mark_reviewed`). Decisión explícita del agente; no la hace el sistema solo.
    pub fn mark_reviewed(&self, id: &str) -> rusqlite::Result<bool> {
        self.db.mark_reviewed(id)
    }

    /// Actualiza una memoria preservando su id (RF-MEM-05). Deriva resumen si falta (RF-MEM-08).
    pub fn update(&self, id: &str, m: &NewMemory) -> rusqlite::Result<bool> {
        let m = asegurar_resumen(m);
        let cambiada = self.db.update_memory(id, &m)?;
        if cambiada {
            self.registrar(
                &m.project,
                None,
                EventKind::MemoryUpdated,
                Some(id),
                Some(&m.title),
            )?;
        }
        Ok(cambiada)
    }

    /// Cambia la importancia de una memoria: fijada, normal o efímera (RF-MEM-04).
    pub fn set_importance(&self, id: &str, importance: Importance) -> rusqlite::Result<bool> {
        self.db.set_importance(id, importance)
    }

    /// Elimina una memoria (RF-MEM-06). Registra el evento con su proyecto y título.
    pub fn delete(&self, id: &str) -> rusqlite::Result<bool> {
        let previa = self.db.get_memory(id)?;
        let eliminada = self.db.delete_memory(id)?;
        if let Some(m) = previa.filter(|_| eliminada) {
            self.registrar(
                &m.project,
                None,
                EventKind::MemoryDeleted,
                Some(id),
                Some(&m.title),
            )?;
        }
        Ok(eliminada)
    }

    /// Candidatos a duplicado/conflicto para una memoria recién guardada (RF-CNF-01): memorias del
    /// mismo proyecto que comparten términos del título, resumen y contenido, excluyendo la propia.
    /// FTS5, sin IA.
    pub fn detectar_candidatos(
        &self,
        m: &NewMemory,
        exclude_id: &str,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let query = consulta_dup(&m.title, m.summary.as_deref(), Some(&m.content));
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .find_candidates(&query, Some(&m.project), exclude_id, MAX_CANDIDATOS_DUP)
    }

    /// Registra una relación entre dos memorias (RF-CNF-02), tras el juicio del agente (RF-CNF-03).
    pub fn add_relation(
        &self,
        from_id: &str,
        to_id: &str,
        kind: RelationKind,
        note: Option<&str>,
    ) -> rusqlite::Result<String> {
        self.db.add_relation(&NewRelation {
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            kind,
            note: note.map(str::to_string),
        })
    }

    /// Relaciones que tocan una memoria (RF-CNF-02).
    pub fn list_relations(&self, memory_id: &str) -> rusqlite::Result<Vec<Relation>> {
        self.db.list_relations(memory_id)
    }

    /// Importa skills/agentes desde rutas explícitas, asignándoles `project` (RF-SKL-06).
    pub fn import_skills(
        &self,
        rutas: &[PathBuf],
        project: &str,
    ) -> rusqlite::Result<ImporteSkills> {
        let escaneo = scan::escanear(rutas, project);
        let mut importadas = 0usize;
        let mut avisos = Vec::new();
        for s in &escaneo.skills {
            self.db.upsert_skill(s)?;
            importadas += 1;
            if let Some(av) = aviso_tamano_skill(s) {
                avisos.push(av);
            }
        }
        Ok(ImporteSkills {
            importadas,
            fuentes: escaneo.fuentes,
            avisos,
        })
    }

    /// Importa desde los directorios por defecto: `skills/` y `agents/` del proyecto (etiquetadas
    /// con `project`) y de `~/.claude/` (globales) (RF-SKL-06).
    pub fn import_skills_default(
        &self,
        cwd: &Path,
        project: &str,
    ) -> rusqlite::Result<ImporteSkills> {
        let mut total = self.import_skills(&scan::rutas_proyecto(cwd), project)?;
        let globales = self.import_skills(&scan::rutas_globales(), "")?;
        total.importadas += globales.importadas;
        total.fuentes.extend(globales.fuentes);
        total.avisos.extend(globales.avisos);
        Ok(total)
    }

    /// Cambia la intensidad de activación de una skill (RF-SKL-07).
    pub fn set_skill_intensity(&self, id: &str, intensidad: Intensidad) -> rusqlite::Result<bool> {
        self.db.set_skill_intensity(id, intensidad)
    }

    /// Skills de comportamiento activas del proyecto (y globales), para el contexto (RF-SKL-07).
    pub fn active_skills(&self, project: &str) -> rusqlite::Result<Vec<Skill>> {
        self.db.active_behavior_skills(project, MAX_CANDIDATES)
    }

    /// Carga las skills semilla embebidas en el binario (RF-SKL-09). Idempotente; devuelve cuántas.
    pub fn seed_skills(&self) -> rusqlite::Result<usize> {
        let semillas = seeds::semillas();
        for sk in &semillas {
            self.db.upsert_skill(sk)?;
        }
        Ok(semillas.len())
    }

    /// Carga el bundle completo embebido (todas las `skills/` + `agents/` del repo). Idempotente;
    /// devuelve cuántas. Es la base del configurador: siembra sin necesitar el repo presente.
    pub fn seed_bundled(&self) -> rusqlite::Result<usize> {
        let bundle = seeds::bundled();
        for sk in &bundle {
            self.db.upsert_skill(sk)?;
        }
        // Reconcilia: quita las entradas de **origen-bundle** (skills/personas del repo) cuyo
        // `(name, kind)` ya no está en el bundle actual —p. ej. personas renombradas— para no dejar
        // duplicados huérfanos. Solo toca lo de origen-bundle; nunca las skills del usuario.
        let presentes: Vec<(String, String)> = bundle
            .iter()
            .map(|sk| (sk.name.clone(), sk.kind.as_str().to_string()))
            .collect();
        self.db.prune_bundle_orphans(&presentes)?;
        Ok(bundle.len())
    }

    /// Persiste un checkpoint de trabajo en curso para sobrevivir a la compactación (RF-SES-04).
    pub fn save_checkpoint(&self, project: &str, content: &str) -> rusqlite::Result<String> {
        self.db.save_checkpoint(project, content)
    }

    /// El checkpoint más reciente de un proyecto, para recuperar el contexto al reanudar (RF-SES-04).
    pub fn latest_checkpoint(&self, project: &str) -> rusqlite::Result<Option<Checkpoint>> {
        self.db.latest_checkpoint(project)
    }

    /// Cambia el nivel de escalonamiento de una memoria: caliente, tibio o frío (RF-TOK-02).
    pub fn set_tier(&self, id: &str, tier: Tier) -> rusqlite::Result<bool> {
        self.db.set_tier(id, tier)
    }

    /// Escalonamiento automático por antigüedad (RF-TOK-02/03): pasa a tibio lo no accedido en
    /// `dias_tibio` y a frío lo no accedido en `dias_frio`. Devuelve `(a_tibio, a_frio)`.
    pub fn escalonar(
        &self,
        project: &str,
        dias_tibio: i64,
        dias_frio: i64,
    ) -> rusqlite::Result<(usize, usize)> {
        let now = ahora_ms();
        self.db
            .escalonar_tiers(project, now - dias_tibio * DIA_MS, now - dias_frio * DIA_MS)
    }

    /// Poda las memorias efímeras sin acceso en `dias` (RF-TOK-05). Devuelve cuántas se eliminaron.
    pub fn podar_efimeras(&self, project: &str, dias: i64) -> rusqlite::Result<usize> {
        self.db.prune_ephemeral(project, ahora_ms() - dias * DIA_MS)
    }

    /// Línea de tiempo de una memoria y sus relacionadas, en orden cronológico (RF-REC-09).
    pub fn memory_timeline(&self, id: &str) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        self.db.related_timeline(id)
    }

    /// Historial de versiones de una memoria de tema evolutivo (versionado temporal de temas), de la
    /// más reciente a la más antigua. Vacío si la memoria nunca se actualizó por `topic_key`.
    pub fn memory_history(&self, id: &str) -> rusqlite::Result<Vec<MemoryVersion>> {
        self.db.memory_versions(id)
    }

    /// Pares de memorias candidatas a fusionar en un proyecto, por solapamiento FTS de títulos
    /// (consolidación asistida): Turtle PROPONE; el agente decide y consolida con las herramientas
    /// existentes (topic_key, relation_add, delete). Sin IA. El escaneo se acota a `scan_limit`
    /// memorias recientes para mantener la latencia baja (no infla la memoria del proceso).
    pub fn consolidation_candidates(
        &self,
        project: &str,
        scan_limit: u32,
    ) -> rusqlite::Result<Vec<DupCandidato>> {
        let recientes = self.db.recent_index(Some(project), scan_limit)?;
        let mut vistos: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut pares = Vec::new();
        for m in &recientes {
            // El índice no trae contenido; usamos título + resumen (el cuerpo de las OTRAS
            // memorias sí está en el FTS, así que igual pesca solapamientos de contenido).
            let query = consulta_dup(&m.title, m.summary.as_deref(), None);
            if query.is_empty() {
                continue;
            }
            // Top-1 candidato por memoria: el solapamiento más fuerte, para no inundar de ruido.
            if let Some(c) = self
                .db
                .find_candidates(&query, Some(project), &m.id, 1)?
                .into_iter()
                .next()
            {
                let clave = if m.id <= c.id {
                    (m.id.clone(), c.id.clone())
                } else {
                    (c.id.clone(), m.id.clone())
                };
                if vistos.insert(clave) {
                    pares.push(DupCandidato {
                        a_id: m.id.clone(),
                        a_titulo: m.title.clone(),
                        b_id: c.id.clone(),
                        b_titulo: c.title.clone(),
                        score: c.score,
                    });
                }
            }
        }
        // Match más fuerte primero (bm25: más negativo = mejor).
        pares.sort_by(|x, y| {
            x.score
                .partial_cmp(&y.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // Pase semántico (solo si la semántica está prendida): agrega pares por **significado**
        // (coseno sobre los embeddings YA guardados; no llama a Ollama). Cierra la brecha con la
        // detección por IA de otros sistemas, pero local. Se anexan después de los de FTS.
        if self.semantica_activa() {
            let semanticos = self.candidatos_semanticos(project, &recientes, &mut vistos)?;
            pares.extend(semanticos);
        }
        Ok(pares)
    }

    /// Candidatos a duplicado por **similitud semántica**: coseno sobre los embeddings guardados,
    /// top-1 por memoria escaneada por encima de `UMBRAL_DUP_SEM`. Usa solo vectores ya persistidos
    /// (no llama a Ollama) y deduplica contra `vistos` (lo que ya propuso el pase FTS).
    fn candidatos_semanticos(
        &self,
        project: &str,
        recientes: &[MemoryIndexRow],
        vistos: &mut std::collections::HashSet<(String, String)>,
    ) -> rusqlite::Result<Vec<DupCandidato>> {
        let embs = self.db.embeddings_for_scope(Some(project))?;
        if embs.len() < 2 {
            return Ok(Vec::new());
        }
        let mut crudos: Vec<(String, String, f32)> = Vec::new();
        for m in recientes {
            let Some(mv) = embs
                .iter()
                .find_map(|(id, v)| (id == &m.id).then_some(v.as_slice()))
            else {
                continue; // esta memoria todavía no tiene embedding (p. ej. guardada con semántica off)
            };
            let mut mejor: Option<(&str, f32)> = None;
            for (oid, ov) in &embs {
                if oid == &m.id {
                    continue;
                }
                let sim = turtle_embed::coseno(mv, ov);
                if sim >= UMBRAL_DUP_SEM && mejor.map_or(true, |(_, bs)| sim > bs) {
                    mejor = Some((oid.as_str(), sim));
                }
            }
            if let Some((oid, sim)) = mejor {
                let clave = if m.id.as_str() <= oid {
                    (m.id.clone(), oid.to_string())
                } else {
                    (oid.to_string(), m.id.clone())
                };
                if vistos.insert(clave) {
                    crudos.push((m.id.clone(), oid.to_string(), sim));
                }
            }
        }
        if crudos.is_empty() {
            return Ok(Vec::new());
        }
        crudos.sort_by(|a, b| b.2.total_cmp(&a.2)); // coseno desc: más parecido primero
        let ids: Vec<String> = crudos
            .iter()
            .flat_map(|(a, b, _)| [a.clone(), b.clone()])
            .collect();
        let titulos: std::collections::HashMap<String, String> = self
            .db
            .index_rows_for_ids(&ids)?
            .into_iter()
            .map(|r| (r.id, r.title))
            .collect();
        Ok(crudos
            .into_iter()
            .map(|(a, b, sim)| DupCandidato {
                a_titulo: titulos.get(&a).cloned().unwrap_or_default(),
                b_titulo: titulos.get(&b).cloned().unwrap_or_default(),
                a_id: a,
                b_id: b,
                score: -(sim as f64), // negativo: coherente con "más negativo = más fuerte"
            })
            .collect())
    }

    /// Consolida memorias de un proyecto en otro reasignándolas (RF-MEM-09). Operación explícita.
    pub fn consolidate_projects(&self, from: &str, to: &str) -> rusqlite::Result<usize> {
        self.db.move_project_memories(from, to)
    }

    /// Búsqueda de skills en modo índice barato (RF-SKL-03). Sin consulta, las más recientes.
    pub fn search_skills(
        &self,
        query: &str,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<SkillIndexRow>> {
        let q = fts_query_from_task(query);
        if q.is_empty() {
            self.db.recent_skills(project, limit)
        } else {
            self.db.search_skills(&q, project, limit)
        }
    }

    /// Carga el contenido completo de una skill por id (RF-SKL-04).
    pub fn get_skill(&self, id: &str) -> rusqlite::Result<Option<Skill>> {
        self.db.get_skill(id)
    }

    /// Guarda una skill capturada en una interacción (RF-SKL-05).
    pub fn save_skill(&self, s: &NewSkill) -> rusqlite::Result<String> {
        self.db.upsert_skill(s)
    }

    /// Cuenta las skills almacenadas.
    pub fn count_skills(&self) -> rusqlite::Result<i64> {
        self.db.count_skills()
    }

    /// Diagnóstico de salud de la base (RF-DIA-02): esquema, integridad, índices FTS
    /// desincronizados, duplicados y conteos por entidad.
    pub fn diagnosticar(&self) -> rusqlite::Result<Diagnostico> {
        let mut chequeos = Vec::new();

        chequeos.push(Chequeo {
            nombre: "Esquema".into(),
            estado: EstadoChequeo::Ok,
            detalle: format!("versión {}", self.db.schema_version()?),
            reparacion: None,
        });

        let integridad = self.db.integrity_check()?;
        chequeos.push(Chequeo {
            nombre: "Integridad SQLite".into(),
            estado: if integridad == "ok" {
                EstadoChequeo::Ok
            } else {
                EstadoChequeo::Error
            },
            detalle: integridad,
            reparacion: None,
        });

        chequeos.push(chequeo_fts(
            "Índice FTS de memorias",
            self.db.count_memories()?,
            self.db.count_fts_docs("memories_fts")?,
        ));
        chequeos.push(chequeo_fts(
            "Índice FTS de skills",
            self.db.count_skills()?,
            self.db.count_fts_docs("skills_fts")?,
        ));

        let dups = self.db.duplicate_memory_groups()?;
        chequeos.push(Chequeo {
            nombre: "Memorias duplicadas".into(),
            estado: if dups == 0 {
                EstadoChequeo::Ok
            } else {
                EstadoChequeo::Aviso
            },
            detalle: if dups == 0 {
                "ninguna".into()
            } else {
                format!("{dups} grupo(s) con (proyecto, título) repetido")
            },
            reparacion: if dups == 0 {
                None
            } else {
                Some("revisalos con «turtle buscar» y unificá con «turtle relacionar» (no se borra solo)".into())
            },
        });

        let stats = vec![
            ("memorias".into(), self.db.count_memories()?),
            ("skills".into(), self.db.count_skills()?),
            ("sesiones".into(), self.db.count_table("sessions")?),
            ("agentes".into(), self.db.count_table("agents")?),
            ("eventos".into(), self.db.count_table("events")?),
            ("relaciones".into(), self.db.count_table("relations")?),
        ];

        Ok(Diagnostico { chequeos, stats })
    }

    /// Estadísticas de la base: totales y desgloses por proyecto y por tipo (RF-DIA-01).
    pub fn estadisticas(&self) -> rusqlite::Result<Estadisticas> {
        Ok(Estadisticas {
            totales: vec![
                ("memorias".into(), self.db.count_memories()?),
                ("skills".into(), self.db.count_skills()?),
                ("sesiones".into(), self.db.count_table("sessions")?),
                ("agentes".into(), self.db.count_table("agents")?),
                ("eventos".into(), self.db.count_table("events")?),
                ("relaciones".into(), self.db.count_table("relations")?),
            ],
            por_proyecto: self.db.count_memories_by_project()?,
            por_tipo: self.db.count_memories_by_type()?,
        })
    }

    /// Aplica las reparaciones seguras (RF-DIA-03): reconstruye los índices FTS desincronizados.
    /// Devuelve la lista de acciones realizadas. No toca duplicados (requieren juicio).
    pub fn reparar(&self) -> rusqlite::Result<Vec<String>> {
        let mut acciones = Vec::new();
        if self.db.count_memories()? != self.db.count_fts_docs("memories_fts")? {
            self.db.rebuild_fts_memories()?;
            acciones.push("Índice FTS de memorias reconstruido.".to_string());
        }
        if self.db.count_skills()? != self.db.count_fts_docs("skills_fts")? {
            self.db.rebuild_fts_skills()?;
            acciones.push("Índice FTS de skills reconstruido.".to_string());
        }
        Ok(acciones)
    }

    /// Memorias completas de un proyecto (o de todos) para exportar (RF-SYN).
    pub fn export_memories(&self, project: Option<&str>) -> rusqlite::Result<Vec<Memory>> {
        self.db.all_memories(project)
    }

    /// Importa memorias completas preservando sus ids (RF-SYN). Devuelve `(nuevas, actualizadas)`.
    /// La base local sigue siendo la fuente de verdad: las existentes se actualizan en el lugar.
    pub fn import_memories(&self, mems: &[Memory]) -> rusqlite::Result<(usize, usize)> {
        let mut nuevas = 0;
        let mut actualizadas = 0;
        for m in mems {
            if self.db.import_memory(m)? {
                nuevas += 1;
            } else {
                actualizadas += 1;
            }
        }
        Ok((nuevas, actualizadas))
    }

    /// Recupera el contenido completo de una memoria por id: segunda etapa (RF-REC-02).
    pub fn get(&self, id: &str) -> rusqlite::Result<Option<Memory>> {
        self.db.get_memory(id)
    }

    /// Búsqueda con presupuesto de tokens (RF-REC-01, RF-REC-03) y perfil de verbosidad
    /// (RF-REC-04): `índice` (metadatos), `compacto` (con extracto) o `completo` (con contenido).
    /// El cuerpo extra cuenta para el presupuesto, así que los perfiles verbosos traen menos filas.
    pub fn search(
        &self,
        query: &str,
        project: Option<&str>,
        token_budget: usize,
        verbosidad: Verbosidad,
    ) -> rusqlite::Result<SearchOutcome> {
        // Sanitiza la consulta: ningún carácter especial de FTS5 (paréntesis, comillas, operadores)
        // debe romper el MATCH (RNF-USA-03). Si no quedan términos, no hay resultados.
        let q = sanitizar_fts(query);
        if q.is_empty() {
            return Ok(apply_budget(Vec::new(), token_budget));
        }
        let candidates = match verbosidad {
            Verbosidad::Indice if self.semantica_activa() => {
                self.buscar_hibrido_indice(&q, query, project)?
            }
            Verbosidad::Indice => self.db.search_index(&q, project, MAX_CANDIDATES)?,
            Verbosidad::Compacto => {
                let mut rows = self.db.search_index_full(&q, project, MAX_CANDIDATES)?;
                for r in &mut rows {
                    r.cuerpo = r.cuerpo.as_deref().map(extracto);
                }
                rows
            }
            Verbosidad::Completo => self.db.search_index_full(&q, project, MAX_CANDIDATES)?,
        };
        Ok(apply_budget(candidates, token_budget))
    }

    // ─── Semántica opt-in (vía Ollama). Default: apagada → todo sigue en FTS. ───

    /// `true` si la semántica está prendida (`turtle semantic on`).
    pub fn semantica_activa(&self) -> bool {
        matches!(self.db.setting_get(SEMANTIC_KEY), Ok(Some(v)) if v == "1")
    }

    /// Modelo de embeddings configurado, o el por defecto.
    fn modelo_embed(&self) -> String {
        self.db
            .setting_get(EMBED_MODEL_KEY)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| turtle_embed::MODELO_POR_DEFECTO.to_string())
    }

    /// Embebe una memoria (best-effort): si Ollama responde, guarda el vector; si no, lo deja para
    /// el backfill. Nunca falla ni propaga error (degrada en silencio).
    fn embed_memoria(&self, id: &str, title: &str, content: &str) {
        let host = turtle_embed::ollama_host();
        let modelo = self.modelo_embed();
        let texto = format!("{title} {content}");
        if let Ok(v) = turtle_embed::embed(&host, &modelo, &texto) {
            let _ = self.db.upsert_embedding(id, &modelo, &v);
        }
    }

    /// Búsqueda híbrida en modo índice: fusiona FTS + similitud semántica por **RRF**. Si Ollama no
    /// responde o el embedding de la consulta falla, **degrada a FTS puro** (nunca rompe).
    fn buscar_hibrido_indice(
        &self,
        q_sanitizada: &str,
        query_original: &str,
        project: Option<&str>,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let fts = self
            .db
            .search_index(q_sanitizada, project, MAX_CANDIDATES)?;
        let host = turtle_embed::ollama_host();
        if !turtle_embed::disponible(&host) {
            return Ok(fts);
        }
        let qvec = match turtle_embed::embed(&host, &self.modelo_embed(), query_original) {
            Ok(v) => v,
            Err(_) => return Ok(fts),
        };
        // Ranking semántico: coseno contra los embeddings del alcance, top MAX_CANDIDATES.
        let mut sem: Vec<(String, f32)> = self
            .db
            .embeddings_for_scope(project)?
            .into_iter()
            .map(|(id, v)| (id, turtle_embed::coseno(&qvec, &v)))
            .collect();
        sem.sort_by(|a, b| b.1.total_cmp(&a.1));
        sem.truncate(MAX_CANDIDATES as usize);
        if sem.is_empty() {
            return Ok(fts);
        }
        // RRF: score(id) = Σ 1/(K + rank) sobre ambas listas; más alto = mejor.
        let mut score: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for (i, r) in fts.iter().enumerate() {
            *score.entry(r.id.clone()).or_default() += 1.0 / (RRF_K + i as f64);
        }
        for (i, (id, _)) in sem.iter().enumerate() {
            *score.entry(id.clone()).or_default() += 1.0 / (RRF_K + i as f64);
        }
        let mut ids: Vec<String> = score.keys().cloned().collect();
        ids.sort_by(|a, b| score[b].total_cmp(&score[a]));
        ids.truncate(MAX_CANDIDATES as usize);
        // Filas finales: reusar las de FTS; traer (sin tocar accessed_at) las que solo aporta la semántica.
        let mut por_id: std::collections::HashMap<String, MemoryIndexRow> =
            fts.into_iter().map(|r| (r.id.clone(), r)).collect();
        let faltantes: Vec<String> = ids
            .iter()
            .filter(|id| !por_id.contains_key(*id))
            .cloned()
            .collect();
        for r in self.db.index_rows_for_ids(&faltantes)? {
            por_id.insert(r.id.clone(), r);
        }
        Ok(ids
            .into_iter()
            .filter_map(|id| por_id.remove(&id))
            .collect())
    }

    /// Estado de la semántica: `(prendida, modelo, ollama_disponible, embebidas, total)`.
    pub fn semantic_status(&self) -> rusqlite::Result<(bool, String, bool, i64, i64)> {
        let activa = self.semantica_activa();
        let modelo = self.modelo_embed();
        let disponible = turtle_embed::disponible(&turtle_embed::ollama_host());
        let (con, total) = self.db.embedding_counts()?;
        Ok((activa, modelo, disponible, con, total))
    }

    /// Prende la semántica: registra el flag y el modelo. Falla si Ollama no responde (el pull del
    /// modelo y el backfill los orquesta la CLI con feedback).
    pub fn semantic_enable(&self, modelo: &str) -> Result<(), String> {
        let host = turtle_embed::ollama_host();
        if !turtle_embed::disponible(&host) {
            return Err(format!(
                "Ollama no responde en {host}. Instala/arranca Ollama y reintenta."
            ));
        }
        self.db
            .setting_set(SEMANTIC_KEY, "1")
            .map_err(|e| e.to_string())?;
        self.db
            .setting_set(EMBED_MODEL_KEY, modelo)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Apaga la semántica (vuelve a FTS puro). No borra los embeddings ya calculados.
    pub fn semantic_disable(&self) -> rusqlite::Result<()> {
        self.db.setting_set(SEMANTIC_KEY, "0")
    }

    /// `true` si el modelo ya está descargado en Ollama.
    pub fn semantic_model_present(&self, modelo: &str) -> bool {
        turtle_embed::modelo_presente(&turtle_embed::ollama_host(), modelo)
    }

    /// Descarga el modelo de embeddings en Ollama (bloqueante; puede tardar).
    pub fn semantic_pull(&self, modelo: &str) -> Result<(), String> {
        turtle_embed::pull(&turtle_embed::ollama_host(), modelo).map_err(|e| e.to_string())
    }

    /// Embebe en lote las memorias que aún no tienen embedding. Devuelve `(hechas, fallidas)`.
    pub fn semantic_backfill(&self, lote: u32) -> rusqlite::Result<(usize, usize)> {
        let host = turtle_embed::ollama_host();
        let modelo = self.modelo_embed();
        let pendientes = self.db.memories_missing_embedding(lote)?;
        let (mut ok, mut err) = (0usize, 0usize);
        for (id, texto) in pendientes {
            match turtle_embed::embed(&host, &modelo, &texto) {
                Ok(v) => {
                    let _ = self.db.upsert_embedding(&id, &modelo, &v);
                    ok += 1;
                }
                Err(_) => err += 1,
            }
        }
        Ok((ok, err))
    }

    /// Contexto de sesión: conjunto acotado y relevante para el proyecto y la tarea
    /// declarada (RF-REC-08). Si la tarea no aporta términos útiles, devuelve las
    /// memorias más recientes del proyecto.
    pub fn session_context(
        &self,
        project: &str,
        task: &str,
        token_budget: usize,
    ) -> rusqlite::Result<SearchOutcome> {
        let query = fts_query_from_task(task);
        let candidates = if query.is_empty() {
            self.db.recent_index(Some(project), MAX_CANDIDATES)?
        } else {
            self.db
                .search_index(&query, Some(project), MAX_CANDIDATES)?
        };
        Ok(apply_budget(candidates, token_budget))
    }

    /// Marca de inicio de la sesión previa de un proyecto (RF-TOK-04). Conviene llamarla **antes**
    /// de iniciar la nueva sesión, para saber qué cambió desde la última.
    pub fn previous_session_start(&self, project: &str) -> rusqlite::Result<Option<i64>> {
        self.db.last_session_start(project)
    }

    /// Marca de tiempo (epoch ms) del último guardado de memoria del proyecto, o `None` si nunca se
    /// guardó. La usa el nudge de guardado del hook prompt-submit (paridad funcional).
    pub fn last_memory_save_time(&self, project: &str) -> rusqlite::Result<Option<i64>> {
        self.db.last_event_time(project, EventKind::MemorySaved)
    }

    /// Deltas de sesión (RF-TOK-04): memorias **fijadas** + **cambios desde `since`** + las
    /// **relevantes a la tarea**, deduplicadas y recortadas al presupuesto. Evita reinyectar todo
    /// el contexto.
    pub fn session_deltas(
        &self,
        project: &str,
        task: &str,
        token_budget: usize,
        since: Option<i64>,
    ) -> rusqlite::Result<SearchOutcome> {
        let mut combinado = Vec::new();
        let mut vistos = HashSet::new();
        agregar_unicas(
            &mut combinado,
            &mut vistos,
            self.db.pinned_index(project, MAX_CANDIDATES)?,
        );
        if let Some(s) = since {
            agregar_unicas(
                &mut combinado,
                &mut vistos,
                self.db.changed_since_index(project, s, MAX_CANDIDATES)?,
            );
        }
        let query = fts_query_from_task(task);
        let relevantes = if query.is_empty() {
            self.db.recent_index(Some(project), MAX_CANDIDATES)?
        } else {
            self.db
                .search_index(&query, Some(project), MAX_CANDIDATES)?
        };
        agregar_unicas(&mut combinado, &mut vistos, relevantes);
        Ok(apply_budget(combinado, token_budget))
    }

    /// Memorias más recientes en modo índice, para vistas de overview.
    pub fn recent(
        &self,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        self.db.recent_index(project, limit)
    }

    /// Sesiones más recientes, para la consulta de sesiones previas.
    pub fn recent_sessions(
        &self,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<Session>> {
        self.db.recent_sessions(project, limit)
    }

    /// Inicia una sesión asociada a un proyecto, una tarea declarada y un agente (RF-SES-01).
    /// Si declara un agente, lo registra/actualiza como "trabajando", con su rama (RF-AGN-01/02).
    /// Devuelve el id de la sesión.
    pub fn start_session(
        &self,
        project: &str,
        task: Option<&str>,
        agent: Option<&str>,
        branch: Option<&str>,
    ) -> rusqlite::Result<String> {
        let id = self.db.start_session(&NewSession {
            project: project.to_string(),
            task: task.map(str::to_string),
            agent_id: agent.map(str::to_string),
        })?;
        if let Some(label) = agent {
            self.db.register_agent(&NewAgent {
                project: project.to_string(),
                label: label.to_string(),
                task: task.map(str::to_string),
                branch: branch.map(str::to_string),
            })?;
        }
        self.registrar(project, agent, EventKind::SessionStarted, Some(&id), task)?;
        Ok(id)
    }

    /// Registra o actualiza un agente directamente (sin abrir sesión).
    pub fn register_agent(
        &self,
        project: &str,
        label: &str,
        task: Option<&str>,
        branch: Option<&str>,
    ) -> rusqlite::Result<Agent> {
        let agente = self.db.register_agent(&NewAgent {
            project: project.to_string(),
            label: label.to_string(),
            task: task.map(str::to_string),
            branch: branch.map(str::to_string),
        })?;
        self.registrar(project, Some(label), EventKind::AgentRegistered, None, task)?;
        Ok(agente)
    }

    /// Lista los agentes registrados (para la coordinación entre agentes).
    pub fn list_agents(&self, project: Option<&str>, limit: u32) -> rusqlite::Result<Vec<Agent>> {
        self.db.list_agents(project, limit)
    }

    /// Lista los eventos del bus de actividad, del más reciente al más antiguo (RF-COM-01).
    pub fn list_events(&self, project: Option<&str>, limit: u32) -> rusqlite::Result<Vec<Event>> {
        self.db.list_events(project, limit)
    }

    /// Registra un evento de actividad (uso de herramienta o dispatch a subagente). Lo usan los
    /// hooks de Claude Code (`turtle hook activity`) para alimentar el feed de actividad.
    pub fn record_activity(
        &self,
        project: &str,
        agent: Option<&str>,
        kind: EventKind,
        summary: Option<&str>,
    ) -> rusqlite::Result<()> {
        // Best-effort: corre por cada tool-call del agente (hook PreToolUse). El insert es barato
        // (~3 ms; el grueso del hook es el arranque del proceso) porque la base ya está en WAL con
        // synchronous=NORMAL, que NO hace fsync por commit; bajar a OFF no ahorraría nada y arriesga
        // la base que guarda las memorias del usuario. Para desactivarlo del todo: $TURTLE_NO_ACTIVITY.
        self.db.record_event(&NewEvent {
            project: project.to_string(),
            agent: agent.map(|s| s.to_string()),
            kind,
            target_id: None,
            summary: summary.map(|s| s.to_string()),
        })
    }

    /// Envía un mensaje a la bandeja y lo anota como evento (RF-COM-03). `to` `None` es difusión.
    pub fn send_message(
        &self,
        project: &str,
        from: Option<&str>,
        to: Option<&str>,
        body: &str,
    ) -> rusqlite::Result<String> {
        let id = self.db.send_message(&NewMessage {
            project: project.to_string(),
            from_agent: from.map(str::to_string),
            to_agent: to.map(str::to_string),
            body: body.to_string(),
        })?;
        let resumen = match to {
            Some(t) => format!("para {t}"),
            None => "difusión".to_string(),
        };
        self.registrar(
            project,
            from,
            EventKind::MessageSent,
            Some(&id),
            Some(&resumen),
        )?;
        Ok(id)
    }

    /// Bandeja de un destinatario (mensajes a su rol o por difusión). Con `only_pending`,
    /// solo los no entregados (RF-COM-06).
    pub fn inbox(
        &self,
        project: &str,
        recipient: &str,
        only_pending: bool,
        limit: u32,
    ) -> rusqlite::Result<Vec<Message>> {
        self.db.inbox(project, recipient, only_pending, limit)
    }

    /// Entrega los mensajes pendientes de un destinatario: los devuelve y los marca entregados.
    /// Pensado para los relevos al iniciar sesión (RF-COM-06).
    pub fn deliver_inbox(&self, project: &str, recipient: &str) -> rusqlite::Result<Vec<Message>> {
        let pendientes = self.db.inbox(project, recipient, true, 1000)?;
        if !pendientes.is_empty() {
            self.db.mark_delivered(project, recipient)?;
        }
        Ok(pendientes)
    }

    /// Atajo interno para anotar un evento en el bus de actividad.
    fn registrar(
        &self,
        project: &str,
        agent: Option<&str>,
        kind: EventKind,
        target_id: Option<&str>,
        summary: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.db.record_event(&NewEvent {
            project: project.to_string(),
            agent: agent.map(str::to_string),
            kind,
            target_id: target_id.map(str::to_string),
            summary: summary.map(str::to_string),
        })
    }

    /// Cierra una sesión y registra un resumen de lo realizado (RF-SES-02). Si `summary` es
    /// `None`, genera uno local a partir de las memorias creadas en el proyecto desde que la
    /// sesión comenzó (resumen simple en el MVP; sin IA). Devuelve la sesión cerrada, o `None`
    /// si no existe o ya estaba cerrada.
    pub fn close_session(
        &self,
        id: &str,
        summary: Option<&str>,
    ) -> rusqlite::Result<Option<Session>> {
        let sesion = match self.db.get_session(id)? {
            Some(s) if s.status == SessionStatus::Open => s,
            _ => return Ok(None),
        };
        let resumen = match summary {
            Some(s) => s.to_string(),
            None => {
                let titulos = self
                    .db
                    .memory_titles_since(&sesion.project, sesion.started_at)?;
                resumen_local(&titulos)
            }
        };
        let cerrada = self.db.close_session(id, &resumen)?;
        // Al cerrar, el agente de la sesión deja de estar "trabajando".
        if let Some(s) = &cerrada {
            if let Some(label) = &s.agent_id {
                self.db.set_agent_idle(&s.project, label)?;
            }
            self.registrar(
                &s.project,
                s.agent_id.as_deref(),
                EventKind::SessionClosed,
                Some(&s.id),
                None,
            )?;
        }
        Ok(cerrada)
    }
}

/// Construye un resumen local de "lo realizado" a partir de los títulos de las memorias
/// creadas durante la sesión (RF-SES-02). Enumera hasta [`MAX_TITULOS_RESUMEN`] títulos.
fn resumen_local(titulos: &[String]) -> String {
    if titulos.is_empty() {
        return "Sesión cerrada sin memorias nuevas registradas.".to_string();
    }
    let mostrados: Vec<&str> = titulos
        .iter()
        .take(MAX_TITULOS_RESUMEN)
        .map(String::as_str)
        .collect();
    let mut resumen = format!(
        "{} memoria(s) guardada(s): {}",
        titulos.len(),
        mostrados.join("; ")
    );
    if titulos.len() > mostrados.len() {
        resumen.push_str(&format!(" y {} más.", titulos.len() - mostrados.len()));
    } else {
        resumen.push('.');
    }
    resumen
}

/// Estima cuántos tokens ocupa presentar una fila del índice al agente.
/// No incluye el contenido completo (esa es justamente la ventaja del índice).
/// Aviso si una skill de comportamiento excede el tamaño recomendado (RF-SKL-08).
fn aviso_tamano_skill(s: &NewSkill) -> Option<String> {
    if s.kind == SkillKind::Behavior && s.content.len() > MAX_BYTES_COMPORTAMIENTO {
        Some(format!(
            "skill de comportamiento «{}» grande ({} bytes; recomendado < {}): puede degradar el presupuesto por turno.",
            s.name,
            s.content.len(),
            MAX_BYTES_COMPORTAMIENTO
        ))
    } else {
        None
    }
}

/// Marca de tiempo actual en epoch ms UTC (para el escalonamiento por antigüedad).
fn ahora_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Agrega filas a `dest` saltando las que ya estén (por id), preservando el orden de llegada.
fn agregar_unicas(
    dest: &mut Vec<MemoryIndexRow>,
    vistos: &mut HashSet<String>,
    filas: Vec<MemoryIndexRow>,
) {
    for f in filas {
        if vistos.insert(f.id.clone()) {
            dest.push(f);
        }
    }
}

fn index_row_tokens(row: &MemoryIndexRow) -> usize {
    let chars = row.id.len()
        + row.title.len()
        + row.kind.as_str().len()
        + row.summary.as_deref().map_or(0, str::len)
        + row.cuerpo.as_deref().map_or(0, str::len)
        + 8; // etiquetas y separadores del formato de salida
    chars.div_ceil(4)
}

/// Longitud máxima del extracto de contenido para el perfil compacto (RF-REC-04).
const LARGO_EXTRACTO: usize = 280;

/// Recorta el contenido a un extracto de una sola pieza para el perfil compacto.
fn extracto(contenido: &str) -> String {
    let limpio = contenido.trim();
    if limpio.chars().count() <= LARGO_EXTRACTO {
        return limpio.to_string();
    }
    let corte: String = limpio.chars().take(LARGO_EXTRACTO).collect();
    format!("{}…", corte.trim_end())
}

/// Largo máximo del resumen derivado automáticamente (RF-MEM-08).
const LARGO_RESUMEN_AUTO: usize = 120;

/// `true` si la memoria no trae un resumen utilizable.
fn necesita_resumen(m: &NewMemory) -> bool {
    match m.summary.as_deref() {
        None => true,
        Some(s) => s.trim().is_empty(),
    }
}

/// Devuelve la memoria con un resumen garantizado: el provisto, o uno derivado del contenido.
fn asegurar_resumen(m: &NewMemory) -> Cow<'_, NewMemory> {
    if necesita_resumen(m) {
        Cow::Owned(NewMemory {
            summary: Some(resumen_heuristico(&m.content)),
            ..m.clone()
        })
    } else {
        Cow::Borrowed(m)
    }
}

/// Resumen heurístico de una línea: la primera oración del contenido, recortada (RF-MEM-08).
/// No usa ningún modelo de IA.
fn resumen_heuristico(contenido: &str) -> String {
    let primera = contenido.trim().lines().next().unwrap_or("").trim();
    let oracion = primera
        .split_inclusive(['.', '!', '?'])
        .next()
        .unwrap_or(primera)
        .trim();
    let base = if oracion.is_empty() { primera } else { oracion };
    if base.chars().count() <= LARGO_RESUMEN_AUTO {
        base.to_string()
    } else {
        let corte: String = base.chars().take(LARGO_RESUMEN_AUTO).collect();
        format!("{}…", corte.trim_end())
    }
}

/// Recorta las filas para no exceder el presupuesto, preservando el orden de relevancia
/// (RF-REC-03). Si una fila no cabe, se detiene (no se excede el presupuesto).
fn apply_budget(rows: Vec<MemoryIndexRow>, token_budget: usize) -> SearchOutcome {
    let mut out = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    for row in rows {
        let cost = index_row_tokens(&row);
        if total + cost > token_budget {
            truncated = true;
            break;
        }
        total += cost;
        out.push(row);
    }
    SearchOutcome {
        rows: out,
        total_tokens: total,
        truncated,
    }
}

/// Cuántas palabras del título entran en la sub-clave de un `topic_key` sugerido.
const TOPIC_KEY_PALABRAS: usize = 4;

/// Propone una clave de tema estable tipo `area/sub` a partir del título (y, como respaldo, del
/// contenido) (paridad con la sugerencia de clave de tema). Heurística simple, sin IA: NO inventa un
/// área temática; usa el tipo de memoria como `area` (un agrupador estable que ya existe) y un slug
/// de las primeras palabras significativas del título como `sub`. Devuelve `None` si no hay texto
/// útil. El agente puede ignorarla y pasar su propia clave.
pub fn suggest_topic_key(kind_area: &str, title: &str, content: &str) -> Option<String> {
    let base = if title.trim().is_empty() {
        content
    } else {
        title
    };
    let sub = slug_palabras(base, TOPIC_KEY_PALABRAS);
    if sub.is_empty() {
        return None;
    }
    let area = slug_palabras(kind_area, 1);
    let area = if area.is_empty() { "tema" } else { &area };
    Some(format!("{area}/{sub}"))
}

/// Slug minúsculo: separa por todo lo que no sea alfanumérico, descarta tokens cortos (1-2 chars),
/// toma hasta `max` palabras y las une con guiones. Sin acentos especiales: deja los alfanuméricos
/// Unicode tal cual en minúscula (no transliterá), suficiente para una clave estable.
fn slug_palabras(texto: &str, max: usize) -> String {
    texto
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .take(max)
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// Construye una consulta FTS5 a partir del texto libre de una tarea: toma las palabras
/// de 3+ caracteres y las une con `OR`. Devuelve cadena vacía si no hay términos útiles.
fn fts_query_from_task(task: &str) -> String {
    let words: Vec<String> = task
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        // Entre comillas: una palabra que sea palabra clave de FTS5 (and/or/not/near) se trata
        // como literal y no como operador, evitando errores de sintaxis.
        .map(|w| format!("\"{}\"", w.to_lowercase()))
        .collect();
    words.join(" OR ")
}

/// Construye la consulta FTS para detección de duplicados: combina **título + resumen + un prefijo
/// acotado del contenido**, en vez de solo el título. Da más señal para pescar solapamientos de
/// *contenido* (el `MATCH` corre contra el índice FTS, que cubre título+contenido+resumen de las
/// otras memorias), sin convertir la consulta en cientos de términos. Sin IA.
fn consulta_dup(titulo: &str, resumen: Option<&str>, contenido: Option<&str>) -> String {
    let mut texto = String::from(titulo);
    if let Some(s) = resumen {
        texto.push(' ');
        texto.push_str(s);
    }
    if let Some(c) = contenido {
        texto.push(' ');
        // Prefijo acotado del cuerpo: suma señal sin explotar el número de términos OR.
        texto.extend(c.chars().take(280));
    }
    fts_query_from_task(&texto)
}

/// Convierte texto libre en una consulta FTS5 **segura**: envuelve cada token (separado por
/// espacios) entre comillas dobles, escapando las internas, para tratarlo como literal. Así ningún
/// carácter especial ni palabra clave de FTS5 (paréntesis, `*`, `:`, comillas, AND/OR/NOT/NEAR)
/// rompe el `MATCH` (RNF-USA-03). Devuelve cadena vacía si no quedan tokens.
fn sanitizar_fts(q: &str) -> String {
    q.split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::MemoryService;
    use turtle_core::memory::{MemoryKind, NewMemory, Verbosidad};
    use turtle_data::Db;

    fn servicio() -> MemoryService {
        MemoryService::new(Db::open_in_memory().unwrap())
    }

    fn nueva(title: &str, content: &str, summary: &str) -> NewMemory {
        NewMemory {
            summary: Some(summary.into()),
            ..NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Decision,
                title.into(),
                content.into(),
            )
        }
    }

    #[test]
    fn historial_de_tema_via_servicio() {
        let s = servicio();
        let con_tema = |t: &str, c: &str| NewMemory {
            topic_key: Some("api/v".into()),
            ..NewMemory::nueva("turtle".into(), MemoryKind::Decision, t.into(), c.into())
        };
        let id = s.save(&con_tema("v1", "uno")).unwrap();
        assert!(
            s.memory_history(&id).unwrap().is_empty(),
            "sin historial al crear"
        );
        s.save(&con_tema("v2", "dos")).unwrap();
        let h = s.memory_history(&id).unwrap();
        assert_eq!(h.len(), 1, "una versión archivada");
        assert_eq!(h[0].title, "v1");
        assert_eq!(h[0].content, "uno");
    }

    #[test]
    fn consolidation_propone_pares_por_solapamiento_de_titulo() {
        let s = servicio();
        // Dos títulos solapados → candidatos; uno sin relación → no aporta par.
        s.save(&nueva("Migración del esquema SQLite", "uno", ""))
            .unwrap();
        s.save(&nueva("Migración del esquema SQLite a v10", "dos", ""))
            .unwrap();
        s.save(&nueva("Política de modelos por persona", "ajeno", ""))
            .unwrap();
        let pares = s.consolidation_candidates("turtle", 50).unwrap();
        assert!(!pares.is_empty(), "detecta el par de migraciones");
        assert!(
            pares
                .iter()
                .any(|p| p.a_titulo.contains("Migración") && p.b_titulo.contains("Migración")),
            "el par involucra las dos memorias de 'Migración'"
        );
        // Sin pares espurios con la memoria no relacionada.
        assert!(
            pares
                .iter()
                .all(|p| !p.a_titulo.contains("Política") && !p.b_titulo.contains("Política")),
            "la memoria no relacionada no genera par"
        );
    }

    #[test]
    fn consolidation_semantica_propone_por_significado() {
        let s = servicio();
        // Títulos/contenido SIN solapamiento léxico: el pase FTS no los emparejaría.
        let id1 = s.save(&nueva("Alfa", "uno", "a")).unwrap();
        let id2 = s.save(&nueva("Beta", "dos", "b")).unwrap();
        // Embeddings idénticos → coseno 1.0 ≥ umbral. Los cargo a mano (sin Ollama) y prendo el flag.
        s.db.upsert_embedding(&id1, "m", &[1.0, 0.0, 0.0]).unwrap();
        s.db.upsert_embedding(&id2, "m", &[1.0, 0.0, 0.0]).unwrap();
        s.db.setting_set("semantic_enabled", "1").unwrap();

        let pares = s.consolidation_candidates("turtle", 50).unwrap();
        assert!(
            pares
                .iter()
                .any(|p| { (p.a_id == id1 && p.b_id == id2) || (p.a_id == id2 && p.b_id == id1) }),
            "el pase semántico debe proponer Alfa↔Beta (coseno alto) aunque FTS no los una"
        );
    }

    #[test]
    fn buscar_no_crashea_con_caracteres_especiales_de_fts() {
        let s = servicio();
        s.save(&nueva(
            "Servidor rmcp",
            "El MCP usa rmcp sobre stdio.",
            "rmcp",
        ))
        .unwrap();
        // Entradas que romperían un MATCH crudo (paréntesis, comillas, operadores, símbolos, `*`).
        for q in [
            "rmcp(",
            "\"",
            "!!!",
            "OR",
            "a AND b",
            "rmcp NEAR stdio",
            "((",
            "col:val",
            "*",
        ] {
            let r = s.search(q, Some("turtle"), 2000, Verbosidad::Indice);
            assert!(
                r.is_ok(),
                "la consulta {q:?} no debe romper el MATCH: {r:?}"
            );
        }
        // Aunque traiga basura, el término real sigue encontrando la memoria.
        let r = s
            .search("rmcp(", Some("turtle"), 2000, Verbosidad::Indice)
            .unwrap();
        assert_eq!(r.rows.len(), 1, "«rmcp(» debe encontrar la memoria de rmcp");
        // Solo símbolos: sin resultados y sin error.
        let r2 = s
            .search("!!!", Some("turtle"), 2000, Verbosidad::Indice)
            .unwrap();
        assert!(r2.rows.is_empty());
    }

    #[test]
    fn guardar_y_recuperar() {
        let s = servicio();
        let id = s
            .save(&nueva("Título", "contenido sobre rust", "resumen"))
            .unwrap();
        assert_eq!(s.get(&id).unwrap().unwrap().title, "Título");
    }

    #[test]
    fn actualizar_preserva_id_y_reindexa() {
        let s = servicio();
        let id = s.save(&nueva("Viejo", "tema antiguo", "r")).unwrap();
        assert!(s
            .update(&id, &nueva("Nuevo", "ahora habla de datos", "r2"))
            .unwrap());
        let got = s.get(&id).unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.title, "Nuevo");
        assert_eq!(
            s.search("datos", None, 100_000, Verbosidad::Indice)
                .unwrap()
                .rows
                .len(),
            1
        );
    }

    #[test]
    fn verbosidad_trae_cuerpo_segun_perfil() {
        let s = servicio();
        let largo = "detalle ".repeat(80); // > 280 caracteres
        s.save(&nueva("Decisión X", &largo, "resumen")).unwrap();

        let idx = s
            .search("decisión", None, 100_000, Verbosidad::Indice)
            .unwrap();
        assert!(idx.rows[0].cuerpo.is_none());

        let comp = s
            .search("decisión", None, 100_000, Verbosidad::Compacto)
            .unwrap();
        let cuerpo = comp.rows[0].cuerpo.as_deref().unwrap();
        assert!(cuerpo.ends_with('…'));
        assert!(cuerpo.chars().count() <= 281);

        let full = s
            .search("decisión", None, 100_000, Verbosidad::Completo)
            .unwrap();
        assert_eq!(full.rows[0].cuerpo.as_deref(), Some(largo.as_str()));
    }

    #[test]
    fn busqueda_respeta_presupuesto_de_tokens() {
        let s = servicio();
        for i in 0..20 {
            s.save(&nueva(
                &format!("Memoria rust {i}"),
                "contenido sobre rust y datos",
                "resumen breve",
            ))
            .unwrap();
        }
        let amplio = s.search("rust", None, 100_000, Verbosidad::Indice).unwrap();
        assert_eq!(amplio.rows.len(), 20);

        let acotado = s.search("rust", None, 50, Verbosidad::Indice).unwrap();
        assert!(acotado.total_tokens <= 50);
        assert!(acotado.rows.len() < amplio.rows.len());
        assert!(acotado.truncated);
    }

    #[test]
    fn costo_del_indice_es_bajo_por_resultado() {
        // RNF-PER-02 / TM-18: < 40 tokens por resultado en promedio, sin contar el contenido.
        let s = servicio();
        let cuerpo = "cuerpo extenso que NO debe contarse en el indice ".repeat(20);
        for i in 0..10 {
            s.save(&nueva(
                &format!("Decisión de arquitectura {i}"),
                &cuerpo,
                "Resumen de una línea del cambio",
            ))
            .unwrap();
        }
        let r = s
            .search("arquitectura", None, 100_000, Verbosidad::Indice)
            .unwrap();
        assert_eq!(r.rows.len(), 10);
        let promedio = r.total_tokens as f64 / r.rows.len() as f64;
        assert!(promedio < 40.0, "promedio {promedio} tokens/resultado");
    }

    #[test]
    fn contexto_de_sesion_por_tarea() {
        let s = servicio();
        s.save(&nueva(
            "Sobre embeddings",
            "usamos ollama para vectores",
            "embeddings",
        ))
        .unwrap();
        s.save(&nueva(
            "Sobre la TUI",
            "ratatui para el mission control",
            "tui",
        ))
        .unwrap();
        let ctx = s
            .session_context("turtle", "trabajo de embeddings con ollama", 100_000)
            .unwrap();
        assert!(ctx.rows.iter().any(|r| r.title == "Sobre embeddings"));
        assert!(!ctx.rows.iter().any(|r| r.title == "Sobre la TUI"));
    }

    #[test]
    fn contexto_sin_tarea_devuelve_recientes() {
        let s = servicio();
        s.save(&nueva("A", "x", "a")).unwrap();
        s.save(&nueva("B", "y", "b")).unwrap();
        let ctx = s.session_context("turtle", "", 100_000).unwrap();
        assert_eq!(ctx.rows.len(), 2);
    }

    #[test]
    fn cerrar_con_resumen_explicito() {
        let s = servicio();
        let id = s
            .start_session("turtle", Some("tarea"), Some("dev"), None)
            .unwrap();
        let cerrada = s
            .close_session(&id, Some("Hecho a mano."))
            .unwrap()
            .unwrap();
        assert_eq!(cerrada.summary.as_deref(), Some("Hecho a mano."));
    }

    #[test]
    fn cerrar_genera_resumen_local_de_lo_realizado() {
        let s = servicio();
        let id = s
            .start_session("turtle", Some("tarea"), None, None)
            .unwrap();
        s.save(&nueva("Decidir el stack", "rust", "r")).unwrap();
        s.save(&nueva("Diseñar el esquema", "sqlite", "r")).unwrap();
        let cerrada = s.close_session(&id, None).unwrap().unwrap();
        let resumen = cerrada.summary.unwrap();
        assert!(resumen.contains("2 memoria(s)"), "resumen: {resumen}");
        assert!(resumen.contains("Decidir el stack"), "resumen: {resumen}");
        assert!(resumen.contains("Diseñar el esquema"), "resumen: {resumen}");
    }

    #[test]
    fn cerrar_sin_memorias_da_resumen_vacio_legible() {
        let s = servicio();
        let id = s.start_session("turtle", None, None, None).unwrap();
        let cerrada = s.close_session(&id, None).unwrap().unwrap();
        assert_eq!(
            cerrada.summary.as_deref(),
            Some("Sesión cerrada sin memorias nuevas registradas.")
        );
    }

    #[test]
    fn cerrar_inexistente_o_repetida_es_none() {
        let s = servicio();
        assert!(s.close_session("no-existe", None).unwrap().is_none());
        let id = s.start_session("turtle", None, None, None).unwrap();
        assert!(s.close_session(&id, Some("una vez")).unwrap().is_some());
        assert!(s.close_session(&id, Some("otra vez")).unwrap().is_none());
    }

    #[test]
    fn iniciar_sesion_registra_al_agente_y_cerrar_lo_deja_idle() {
        use turtle_core::agent::AgentStatus;
        let s = servicio();
        let id = s
            .start_session(
                "turtle",
                Some("implementar"),
                Some("backend"),
                Some("feat/x"),
            )
            .unwrap();
        let agentes = s.list_agents(Some("turtle"), 10).unwrap();
        assert_eq!(agentes.len(), 1);
        assert_eq!(agentes[0].label, "backend");
        assert_eq!(agentes[0].status, AgentStatus::Working);
        assert_eq!(agentes[0].branch.as_deref(), Some("feat/x"));

        s.close_session(&id, None).unwrap();
        let agentes = s.list_agents(Some("turtle"), 10).unwrap();
        assert_eq!(agentes[0].status, AgentStatus::Idle);
    }

    #[test]
    fn las_operaciones_quedan_en_el_feed_de_actividad() {
        use turtle_core::event::EventKind;
        let s = servicio();
        s.save(&nueva("Una memoria", "contenido", "r")).unwrap();
        let id = s
            .start_session("turtle", Some("tarea"), Some("dev"), Some("main"))
            .unwrap();
        s.close_session(&id, None).unwrap();

        let eventos = s.list_events(Some("turtle"), 50).unwrap();
        // El más reciente primero: cerró sesión.
        assert_eq!(eventos[0].kind, EventKind::SessionClosed);
        // Hay un guardado de memoria y un inicio de sesión atribuido a "dev".
        assert!(eventos.iter().any(|e| e.kind == EventKind::MemorySaved));
        assert!(eventos
            .iter()
            .any(|e| e.kind == EventKind::SessionStarted && e.agent.as_deref() == Some("dev")));
    }

    #[test]
    fn enviar_mensaje_queda_en_actividad_y_se_entrega_una_vez() {
        use turtle_core::event::EventKind;
        let s = servicio();
        s.send_message(
            "turtle",
            Some("frontend"),
            Some("backend"),
            "revisa el endpoint",
        )
        .unwrap();

        // Aparece en el feed de actividad, atribuido al remitente.
        let eventos = s.list_events(Some("turtle"), 10).unwrap();
        assert!(eventos
            .iter()
            .any(|e| e.kind == EventKind::MessageSent && e.agent.as_deref() == Some("frontend")));

        // La entrega devuelve el pendiente y lo marca; la segunda vez ya no hay pendientes.
        let entregados = s.deliver_inbox("turtle", "backend").unwrap();
        assert_eq!(entregados.len(), 1);
        assert_eq!(entregados[0].body, "revisa el endpoint");
        assert!(s.deliver_inbox("turtle", "backend").unwrap().is_empty());
    }

    #[test]
    fn detecta_candidatos_al_guardar_y_registra_relacion() {
        use turtle_core::relation::RelationKind;
        let s = servicio();
        let id_a = s
            .save(&nueva("Usar rmcp", "El MCP usa rmcp.", "r"))
            .unwrap();
        let nueva_b = nueva("Usar rmcp en el servidor", "rmcp sobre stdio.", "r2");
        let id_b = s.save(&nueva_b).unwrap();

        // Los candidatos de b incluyen a (comparten términos), y no a la propia b.
        let cands = s.detectar_candidatos(&nueva_b, &id_b).unwrap();
        assert!(cands.iter().any(|c| c.id == id_a));
        assert!(!cands.iter().any(|c| c.id == id_b));

        // El agente decide que b es duplicado de a; queda registrado.
        s.add_relation(&id_b, &id_a, RelationKind::DuplicateOf, Some("casi igual"))
            .unwrap();
        let rels = s.list_relations(&id_a).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].kind, RelationKind::DuplicateOf);
    }

    #[test]
    fn diagnostico_sano_y_detecta_duplicados() {
        use super::EstadoChequeo;
        let s = servicio();
        let d = s.diagnosticar().unwrap();
        assert!(!d.hay_errores());
        assert!(d
            .chequeos
            .iter()
            .any(|c| c.nombre == "Integridad SQLite" && c.estado == EstadoChequeo::Ok));
        let dup = d
            .chequeos
            .iter()
            .find(|c| c.nombre == "Memorias duplicadas")
            .unwrap();
        assert_eq!(dup.estado, EstadoChequeo::Ok);

        // Dos memorias con el mismo título → aviso de duplicado (no error).
        s.save(&nueva("igual", "a", "")).unwrap();
        s.save(&nueva("igual", "b", "")).unwrap();
        let d2 = s.diagnosticar().unwrap();
        let dup2 = d2
            .chequeos
            .iter()
            .find(|c| c.nombre == "Memorias duplicadas")
            .unwrap();
        assert_eq!(dup2.estado, EstadoChequeo::Aviso);
        assert!(!d2.hay_errores());
    }

    #[test]
    fn export_import_ida_y_vuelta() {
        let a = servicio();
        a.save(&nueva("t1", "c1", "s1")).unwrap();
        a.save(&nueva("t2", "c2", "s2")).unwrap();
        let mems = a.export_memories(Some("turtle")).unwrap();
        assert_eq!(mems.len(), 2);

        let b = servicio();
        assert_eq!(b.import_memories(&mems).unwrap(), (2, 0));
        // Idempotente: reimportar actualiza, no duplica.
        assert_eq!(b.import_memories(&mems).unwrap(), (0, 2));
        assert_eq!(b.export_memories(Some("turtle")).unwrap().len(), 2);
    }

    #[test]
    fn resumen_derivado_y_cambio_de_importancia() {
        use turtle_core::memory::Importance;
        let s = servicio();
        let sin_resumen = NewMemory::nueva(
            "turtle".into(),
            MemoryKind::Note,
            "Sin resumen".into(),
            "Primera oración del cuerpo. Segunda que no debería aparecer.".into(),
        );
        let id = s.save(&sin_resumen).unwrap();
        let m = s.get(&id).unwrap().unwrap();
        assert_eq!(m.summary.as_deref(), Some("Primera oración del cuerpo."));
        assert_eq!(m.importance, Importance::Normal);

        assert!(s.set_importance(&id, Importance::Pinned).unwrap());
        assert_eq!(s.get(&id).unwrap().unwrap().importance, Importance::Pinned);
    }

    #[test]
    fn deltas_de_sesion_combinan_fijadas_cambios_y_relevantes_sin_repetir() {
        use turtle_core::memory::Importance;
        let s = servicio();
        let pin = s.save(&nueva("Fijada", "algo importante", "")).unwrap();
        s.set_importance(&pin, Importance::Pinned).unwrap();
        let nuevo = s.save(&nueva("Reciente", "novedad de hoy", "")).unwrap();

        // since=0 hace que todo cuente como "cambio"; la tarea matchea "novedad".
        let d = s
            .session_deltas("turtle", "novedad", 100_000, Some(0))
            .unwrap();
        let ids: Vec<&str> = d.rows.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&pin.as_str()));
        assert!(ids.contains(&nuevo.as_str()));
        let unicos: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unicos.len(), ids.len(), "no debe haber filas repetidas");
    }

    #[test]
    fn estadisticas_por_proyecto_y_tipo_y_reparacion() {
        let s = servicio();
        s.save(&nueva("A", "a", "")).unwrap();
        s.save(&nueva("B", "b", "")).unwrap();
        let e = s.estadisticas().unwrap();
        assert_eq!(
            e.por_proyecto
                .iter()
                .find(|(k, _)| k == "turtle")
                .unwrap()
                .1,
            2
        );
        assert!(e.por_tipo.iter().any(|(k, n)| k == "decision" && *n == 2));
        // Una base sana no requiere reparaciones.
        assert!(s.reparar().unwrap().is_empty());
    }

    #[test]
    fn semillas_se_cargan_y_son_idempotentes() {
        let s = servicio();
        let n = s.seed_skills().unwrap();
        assert!(n >= 3);
        assert_eq!(s.count_skills().unwrap(), n as i64);
        // Recargar no duplica.
        s.seed_skills().unwrap();
        assert_eq!(s.count_skills().unwrap(), n as i64);
        // Una semilla queda buscable.
        assert!(s
            .search_skills("turtle protocolo", None, 10)
            .unwrap()
            .iter()
            .any(|r| r.name == "turtle-protocol"));
    }

    #[test]
    fn bundle_embebido_se_carga_y_es_idempotente() {
        let s = servicio();
        let n = s.seed_bundled().unwrap();
        assert!(
            n >= 28,
            "esperaba el bundle completo (skills + personas), hubo {n}"
        );
        assert_eq!(s.count_skills().unwrap(), n as i64);
        // Recargar no duplica.
        s.seed_bundled().unwrap();
        assert_eq!(s.count_skills().unwrap(), n as i64);
        // Una persona embebida (agents/brunelleschi/AGENT.md) queda buscable.
        assert!(s
            .search_skills("brunelleschi", None, 40)
            .unwrap()
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("brunelleschi")));
    }

    #[test]
    fn subagentes_claude_se_generan_con_modelo() {
        let subs = super::subagentes_claude(&std::collections::BTreeMap::new());
        assert!(
            subs.len() >= 8,
            "esperaba >=8 personas, hubo {}",
            subs.len()
        );
        let brunelleschi = subs
            .iter()
            .find(|s| s.slug == "brunelleschi")
            .expect("brunelleschi");
        assert!(brunelleschi.contenido.contains("name: brunelleschi"));
        assert!(
            brunelleschi.contenido.contains("model:"),
            "lleva campo model"
        );
        assert!(
            brunelleschi.contenido.contains("TURTLE-AGENT"),
            "lleva el marcador"
        );
    }

    #[test]
    fn subagente_respeta_override_de_modelo() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("brunelleschi".to_string(), "haiku".to_string());
        let subs = super::subagentes_claude(&overrides);
        let brunelleschi = subs
            .iter()
            .find(|s| s.slug == "brunelleschi")
            .expect("brunelleschi");
        assert!(
            brunelleschi.contenido.contains("model: haiku"),
            "el override pisa el modelo del frontmatter"
        );
        // Una persona sin override conserva su modelo por defecto del bundle.
        let otra = subs
            .iter()
            .find(|s| s.slug != "brunelleschi")
            .expect("otra persona");
        assert!(!otra.contenido.contains("model: haiku") || otra.slug == "brunelleschi");
    }

    #[test]
    fn catalogo_de_modelos_valida_tokens() {
        assert!(super::modelo_valido("opus"));
        assert!(super::modelo_valido("claude-fable-5"));
        assert!(super::modelo_valido("inherit"));
        assert!(!super::modelo_valido("gpt-5"));
        assert!(!super::modelo_valido(""));
    }

    #[test]
    fn personas_traen_modelo_default() {
        let ps = super::personas();
        assert!(ps.len() >= 8, "esperaba >=8 personas, hubo {}", ps.len());
        let brunelleschi = ps
            .iter()
            .find(|p| p.slug == "brunelleschi")
            .expect("brunelleschi");
        assert!(!brunelleschi.modelo_default.is_empty());
    }

    #[test]
    fn save_con_topic_key_actualiza_en_vez_de_duplicar() {
        let s = servicio();
        let con_tema = |titulo: &str, content: &str| NewMemory {
            topic_key: Some("api/contrato".into()),
            ..NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Decision,
                titulo.into(),
                content.into(),
            )
        };
        let id1 = s.save(&con_tema("Contrato v1", "primera versión")).unwrap();
        let id2 = s.save(&con_tema("Contrato v2", "segunda versión")).unwrap();
        assert_eq!(id1, id2, "el mismo tema se actualiza");
        let m = s.get(&id1).unwrap().unwrap();
        assert_eq!(m.title, "Contrato v2");
        // Una sola memoria para el proyecto.
        assert_eq!(s.export_memories(Some("turtle")).unwrap().len(), 1);
    }

    #[test]
    fn save_sin_prompt_adjunta_el_ultimo_registrado_best_effort() {
        let s = servicio();
        // Sin prompt registrado: el save igual funciona, sin prompt.
        let id0 = s.save(&nueva("Sin prompt", "contenido", "r")).unwrap();
        assert!(s.get(&id0).unwrap().unwrap().prompt.is_none());

        // Registro un prompt; el próximo save sin prompt lo adjunta.
        s.record_prompt("turtle", None, "implementá la feature X")
            .unwrap();
        let id1 = s.save(&nueva("Con prompt heredado", "c", "r")).unwrap();
        assert_eq!(
            s.get(&id1).unwrap().unwrap().prompt.as_deref(),
            Some("implementá la feature X")
        );
        // El prompt se consumió: el siguiente save no lo vuelve a adjuntar.
        let id2 = s.save(&nueva("Otra", "c2", "r")).unwrap();
        assert!(s.get(&id2).unwrap().unwrap().prompt.is_none());
    }

    #[test]
    fn save_con_prompt_explicito_no_lo_pisa() {
        let s = servicio();
        s.record_prompt("turtle", None, "prompt del entorno")
            .unwrap();
        let m = NewMemory {
            prompt: Some("prompt explícito".into()),
            ..nueva("Explícita", "c", "r")
        };
        let id = s.save(&m).unwrap();
        assert_eq!(
            s.get(&id).unwrap().unwrap().prompt.as_deref(),
            Some("prompt explícito"),
            "el prompt explícito gana sobre el del entorno"
        );
    }

    #[test]
    fn personal_visible_cross_proyecto_en_busqueda() {
        let s = servicio();
        let personal = NewMemory {
            scope: turtle_core::memory::Scope::Personal,
            ..NewMemory::nueva(
                "proyA".into(),
                MemoryKind::Convention,
                "Estilo neutro".into(),
                "Siempre español latino neutro.".into(),
            )
        };
        s.save(&personal).unwrap();
        // Buscando filtrando por otro proyecto, la personal aparece igual.
        let r = s
            .search("español", Some("proyB"), 100_000, Verbosidad::Indice)
            .unwrap();
        assert!(r.rows.iter().any(|x| x.title == "Estilo neutro"));
    }

    #[test]
    fn ciclo_de_revision_por_servicio() {
        let s = servicio();
        let id = s.save(&nueva("Vieja", "contenido", "r")).unwrap();
        // Escalonar a frío (cortes futuros) marca needs_review.
        s.escalonar("turtle", -1, -1).unwrap();
        let lista = s.needs_review_list("turtle", 10).unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, id);
        assert!(lista[0].needs_review);
        // Marcar revisada la saca de la lista.
        assert!(s.mark_reviewed(&id).unwrap());
        assert!(s.needs_review_list("turtle", 10).unwrap().is_empty());
    }

    #[test]
    fn suggest_topic_key_propone_clave_estable() {
        // area = tipo de memoria; sub = slug de las primeras 4 palabras significativas (3+ chars)
        // del título. "de" (2 chars) se descarta; los acentos se conservan en minúscula.
        let k = super::suggest_topic_key("architecture", "Diseño del esquema de datos", "");
        assert_eq!(k.as_deref(), Some("architecture/diseño-del-esquema-datos"));
        // Sin título usa el contenido.
        let k2 = super::suggest_topic_key("note", "", "Migración de la base SQLite");
        assert!(k2.unwrap().starts_with("note/"));
        // Sin texto útil: None.
        assert!(super::suggest_topic_key("note", "  ", "!! ?").is_none());
    }

    #[test]
    fn checkpoint_persiste_y_recupera_el_ultimo() {
        let s = servicio();
        assert!(s.latest_checkpoint("turtle").unwrap().is_none());
        s.save_checkpoint("turtle", "voy por el paso 2 de 5")
            .unwrap();
        s.save_checkpoint("turtle", "ahora voy por el paso 3")
            .unwrap();
        let c = s.latest_checkpoint("turtle").unwrap().unwrap();
        assert_eq!(c.content, "ahora voy por el paso 3");
        assert!(s.latest_checkpoint("otro").unwrap().is_none());
    }

    #[test]
    fn escalonamiento_archiva_y_poda_efimeras() {
        use turtle_core::memory::Importance;
        let s = servicio();
        let id = s.save(&nueva("Vieja", "contenido completo", "")).unwrap();
        let efim = s.save(&nueva("Efímera", "temporal", "")).unwrap();
        s.set_importance(&efim, Importance::Ephemeral).unwrap();

        // dias_tibio = -1: el corte queda en el futuro, así que todo lo no fijado pasa a tibio.
        let (tibio, _) = s.escalonar("turtle", -1, 9999).unwrap();
        assert!(tibio >= 1);
        // La memoria archivada sigue siendo recuperable bajo demanda (RF-TOK-03).
        assert!(s.get(&id).unwrap().is_some());

        // Poda de efímeras (corte futuro → elimina la efímera).
        assert_eq!(s.podar_efimeras("turtle", -1).unwrap(), 1);
        assert!(s.get(&efim).unwrap().is_none());
    }

    #[test]
    fn timeline_relacionado_y_consolidacion_entre_proyectos() {
        use turtle_core::relation::RelationKind;
        let s = servicio();
        let a = s.save(&nueva("A", "primera", "")).unwrap();
        let b = s.save(&nueva("B", "segunda", "")).unwrap();
        s.add_relation(&a, &b, RelationKind::RelatesTo, None)
            .unwrap();

        let tl = s.memory_timeline(&a).unwrap();
        let ids: Vec<&str> = tl.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&a.as_str()) && ids.contains(&b.as_str()));
        assert_eq!(tl[0].id, a, "orden cronológico: a antes que b");

        // Consolidar: mover todo de «turtle» a «global».
        assert_eq!(s.consolidate_projects("turtle", "global").unwrap(), 2);
        assert_eq!(s.export_memories(Some("global")).unwrap().len(), 2);
        assert!(s.export_memories(Some("turtle")).unwrap().is_empty());
    }
}
