//! `turtle-data` — Acceso a datos: SQLite (`rusqlite`), migraciones y FTS5.
//!
//! Único archivo SQLite local como fuente de verdad (RF-BD-07), en modo WAL para
//! concurrencia multi-agente sin corrupción (RNF-FIA-02). Ver arquitectura §3 y §7.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Row};
use turtle_core::agent::{Agent, AgentStatus, NewAgent};
use turtle_core::checkpoint::Checkpoint;
use turtle_core::event::{Event, EventKind, NewEvent};
use turtle_core::memory::{
    Importance, Memory, MemoryIndexRow, MemoryKind, MemoryVersion, NewMemory, ReviewState, Scope,
    Tier,
};
use turtle_core::message::{Message, NewMessage};
use turtle_core::relation::{NewRelation, Relation, RelationKind};
use turtle_core::session::{NewSession, Session, SessionStatus};
use turtle_core::skill::{Intensidad, NewSkill, Skill, SkillIndexRow, SkillKind};

/// Re-exporta `rusqlite` para que las capas superiores nombren sus tipos (p. ej. `Result`)
/// sin depender de la dependencia directamente.
pub use rusqlite;

/// Versión del esquema persistida en `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 11;

/// Conexión a la base de Turtle, con el esquema ya migrado.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Abre (o crea) la base en `path` y aplica las migraciones pendientes.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Abre una base en memoria (para pruebas).
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA synchronous=NORMAL;",
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let mut version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        // Hot path: el binario abre la base en CADA invocación (incluido el hook PreToolUse, que
        // corre por cada tool-call). Si el esquema ya está al día, salimos sin escribir: el
        // `pragma_update(user_version)` del final fuerza I/O de WAL en cada arranque y, sin
        // migraciones pendientes, es puro costo. Salida temprana barata (ponytail).
        //
        // Usamos `>=` y no `==`: si la base fue creada por un binario MÁS NUEVO (user_version >
        // SCHEMA_VERSION), también salimos sin tocar nada. Con `==` caíamos al `pragma_update` final
        // y **degradábamos** el número de versión al nuestro sin tocar el esquema, dejando un
        // `user_version` mentiroso. No la migramos hacia atrás: la dejamos intacta.
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        // Migraciones incrementales: cada paso lleva el esquema a la versión siguiente.
        if version < 1 {
            self.conn.execute_batch(MIGRATION_V1)?;
            version = 1;
        }
        if version < 2 {
            self.conn.execute_batch(MIGRATION_V2)?;
            version = 2;
        }
        if version < 3 {
            self.conn.execute_batch(MIGRATION_V3)?;
            version = 3;
        }
        if version < 4 {
            self.conn.execute_batch(MIGRATION_V4)?;
            version = 4;
        }
        if version < 5 {
            self.conn.execute_batch(MIGRATION_V5)?;
            version = 5;
        }
        if version < 6 {
            self.conn.execute_batch(MIGRATION_V6)?;
            version = 6;
        }
        if version < 7 {
            self.conn.execute_batch(MIGRATION_V7)?;
            version = 7;
        }
        if version < 8 {
            self.conn.execute_batch(MIGRATION_V8)?;
            version = 8;
        }
        if version < 9 {
            self.conn.execute_batch(MIGRATION_V9)?;
            version = 9;
        }
        if version < 10 {
            self.conn.execute_batch(MIGRATION_V10)?;
            version = 10;
        }
        if version < 11 {
            self.conn.execute_batch(MIGRATION_V11)?;
            version = 11;
        }
        let _ = version;
        self.conn
            .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Guarda una memoria nueva y devuelve su identificador (ULID). Asigna las marcas de tiempo.
    ///
    /// Si la memoria trae `topic_key`, hace UPSERT por `(project, scope, topic_key)` (paridad con
    /// sistemas afines: un tema evolutivo se actualiza en lugar de duplicarse). En conflicto preserva el id y
    /// la fecha de creación de la memoria existente y refresca el resto. Sin `topic_key`, es un alta
    /// común. Devuelve el id resultante (el nuevo, o el de la memoria del tema ya existente).
    pub fn insert_memory(&self, m: &NewMemory) -> rusqlite::Result<String> {
        let now = now_ms();
        let token_est = estimate_tokens(&m.content);
        // Camino de tema (UPSERT): solo cuando hay topic_key. Apunta al índice único parcial
        // (project, scope, topic_key). Conserva id/created_at; refresca el contenido y updated_at.
        if m.topic_key.as_deref().is_some_and(|t| !t.trim().is_empty()) {
            // Transacción atómica: archivar la versión viva del tema (si existe) y luego el upsert,
            // para no dejar un snapshot sin su actualización ni una actualización sin su historial.
            let tx = self.conn.unchecked_transaction()?;
            let vigente = tx
                .query_row(
                    "SELECT id, type, title, what, why, where_, learned, content, summary, updated_at
                     FROM memories WHERE project=?1 AND scope=?2 AND topic_key=?3",
                    params![m.project, m.scope.as_str(), m.topic_key],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                            r.get::<_, Option<String>>(6)?,
                            r.get::<_, String>(7)?,
                            r.get::<_, Option<String>>(8)?,
                            r.get::<_, i64>(9)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((mid, ty, title, what, why, wh, learned, content, summary, valid_from)) =
                vigente
            {
                tx.execute(
                    "INSERT INTO memory_versions
                        (id, memory_id, project, type, title, what, why, where_, learned, content,
                         summary, valid_from, valid_to, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
                    params![
                        ulid::Ulid::new().to_string(),
                        mid,
                        m.project,
                        ty,
                        title,
                        what,
                        why,
                        wh,
                        learned,
                        content,
                        summary,
                        valid_from,
                        now,
                    ],
                )?;
            }
            tx.execute(
                "INSERT INTO memories
                    (id, project, type, title, what, why, where_, learned, content, summary,
                     scope, topic_key, prompt, token_est, created_at, updated_at, accessed_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15,?15)
                 ON CONFLICT(project, scope, topic_key) WHERE topic_key IS NOT NULL DO UPDATE SET
                    type=excluded.type, title=excluded.title, what=excluded.what, why=excluded.why,
                    where_=excluded.where_, learned=excluded.learned, content=excluded.content,
                    summary=excluded.summary, prompt=COALESCE(excluded.prompt, memories.prompt),
                    token_est=excluded.token_est, updated_at=excluded.updated_at,
                    accessed_at=excluded.updated_at, review_state='active'",
                params![
                    ulid::Ulid::new().to_string(),
                    m.project,
                    m.kind.as_str(),
                    m.title,
                    m.what,
                    m.why,
                    m.where_,
                    m.learned,
                    m.content,
                    m.summary,
                    m.scope.as_str(),
                    m.topic_key,
                    m.prompt,
                    token_est,
                    now,
                ],
            )?;
            // Devuelve el id real del tema (nuevo o preexistente).
            let id: String = tx.query_row(
                "SELECT id FROM memories WHERE project=?1 AND scope=?2 AND topic_key=?3",
                params![m.project, m.scope.as_str(), m.topic_key],
                |r| r.get(0),
            )?;
            tx.commit()?;
            return Ok(id);
        }
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO memories
                (id, project, type, title, what, why, where_, learned, content, summary,
                 scope, prompt, token_est, created_at, updated_at, accessed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14,?14)",
            params![
                id,
                m.project,
                m.kind.as_str(),
                m.title,
                m.what,
                m.why,
                m.where_,
                m.learned,
                m.content,
                m.summary,
                m.scope.as_str(),
                m.prompt,
                token_est,
                now,
            ],
        )?;
        Ok(id)
    }

    /// Historial de versiones de una memoria de tema evolutivo, de la más reciente a la más antigua
    /// (versionado temporal de temas). Vacío si la memoria nunca se actualizó (no tiene versiones).
    pub fn memory_versions(&self, memory_id: &str) -> rusqlite::Result<Vec<MemoryVersion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, memory_id, project, type, title, what, why, where_, learned, content,
                    summary, valid_from, valid_to
             FROM memory_versions WHERE memory_id=?1 ORDER BY valid_to DESC",
        )?;
        let filas = stmt
            .query_map(params![memory_id], |r| {
                Ok(MemoryVersion {
                    id: r.get(0)?,
                    memory_id: r.get(1)?,
                    project: r.get(2)?,
                    kind: MemoryKind::parse(&r.get::<_, String>(3)?).unwrap_or(MemoryKind::Note),
                    title: r.get(4)?,
                    what: r.get(5)?,
                    why: r.get(6)?,
                    where_: r.get(7)?,
                    learned: r.get(8)?,
                    content: r.get(9)?,
                    summary: r.get(10)?,
                    valid_from: r.get(11)?,
                    valid_to: r.get(12)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(filas)
    }

    // ─── Semántica opt-in (v11): settings clave/valor + embeddings por memoria ───

    /// Lee un ajuste por clave (tabla `settings`); `None` si no existe.
    pub fn setting_get(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key=?1")?;
        let mut filas = stmt.query(params![key])?;
        match filas.next()? {
            Some(r) => Ok(Some(r.get(0)?)),
            None => Ok(None),
        }
    }

    /// Fija (upsert) un ajuste por clave.
    pub fn setting_set(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Guarda (upsert) el embedding de una memoria (BLOB f32 little-endian).
    pub fn upsert_embedding(
        &self,
        memory_id: &str,
        model: &str,
        vec: &[f32],
    ) -> rusqlite::Result<()> {
        let blob: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.conn.execute(
            "INSERT INTO memory_embeddings(memory_id, model, dim, vec, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(memory_id) DO UPDATE SET model=excluded.model, dim=excluded.dim,
                 vec=excluded.vec, updated_at=excluded.updated_at",
            params![memory_id, model, vec.len() as i64, blob, now_ms()],
        )?;
        Ok(())
    }

    /// `(memory_id, vector)` de las memorias del alcance de búsqueda (proyecto + personales), para el
    /// KNN coseno en Rust. `project = None` no filtra por proyecto.
    pub fn embeddings_for_scope(
        &self,
        project: Option<&str>,
    ) -> rusqlite::Result<Vec<(String, Vec<f32>)>> {
        let crudas: Vec<(String, Vec<u8>)> = match project {
            Some(p) => {
                let mut stmt = self.conn.prepare(
                    "SELECT e.memory_id, e.vec FROM memory_embeddings e
                     JOIN memories m ON m.id = e.memory_id
                     WHERE m.project=?1 OR m.scope='personal'",
                )?;
                let filas = stmt
                    .query_map(params![p], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                filas
            }
            None => {
                let mut stmt = self
                    .conn
                    .prepare("SELECT memory_id, vec FROM memory_embeddings")?;
                let filas = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                filas
            }
        };
        Ok(crudas
            .into_iter()
            .map(|(id, b)| {
                let v = b
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                (id, v)
            })
            .collect())
    }

    /// `(id, título + contenido)` de memorias **sin** embedding, para el backfill. Acotado por `limit`.
    pub fn memories_missing_embedding(
        &self,
        limit: u32,
    ) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.title || ' ' || m.content FROM memories m
             LEFT JOIN memory_embeddings e ON e.memory_id = m.id
             WHERE e.memory_id IS NULL LIMIT ?1",
        )?;
        let filas = stmt
            .query_map(params![limit], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(filas)
    }

    /// `(memorias con embedding, total de memorias)`. Para `turtle semantic status`.
    pub fn embedding_counts(&self) -> rusqlite::Result<(i64, i64)> {
        let con: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM memory_embeddings", [], |r| r.get(0))?;
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
        Ok((con, total))
    }

    /// Filas de índice (id, título, resumen, …) para un conjunto de ids, **sin** tocar `accessed_at`.
    /// La usa la fusión FTS + semántica (RRF) para traer las filas que solo aporta la semántica.
    /// Score 0 (no es ranking FTS); el orden final lo decide quien llama.
    pub fn index_rows_for_ids(&self, ids: &[String]) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT m.id, m.title, m.type, m.summary, 0.0 AS score,
                    (m.review_state = 'needs_review') AS needs_review
             FROM memories m WHERE m.id IN ({placeholders})"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(ids.iter()),
            map_index_full_row(false),
        )?;
        rows.collect()
    }

    /// Recupera el contenido completo de una memoria por id (segunda etapa, RF-REC-02).
    /// Actualiza la marca de último acceso (RF-MEM-03).
    pub fn get_memory(&self, id: &str) -> rusqlite::Result<Option<Memory>> {
        let touched = self.conn.execute(
            "UPDATE memories SET accessed_at=?1 WHERE id=?2",
            params![now_ms(), id],
        )?;
        if touched == 0 {
            return Ok(None);
        }
        self.conn
            .query_row(
                "SELECT id, project, type, title, what, why, where_, learned, content, summary,
                        importance, tier, scope, topic_key, review_state, prompt,
                        created_at, updated_at, accessed_at
                 FROM memories WHERE id=?1",
                params![id],
                map_memory,
            )
            .optional()
    }

    /// Búsqueda en modo índice: devuelve metadatos baratos (id, título, tipo, resumen, puntaje),
    /// **sin** el contenido completo (RF-REC-01, RF-REC-05). `score` es el `bm25` de FTS5
    /// (menor = más relevante); el orden ya viene por relevancia.
    pub fn search_index(
        &self,
        query: &str,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        // El filtro de proyecto incluye SIEMPRE las memorias personales (scope='personal'), que son
        // transversales al usuario (paridad funcional). Sin filtro (project NULL) ya las trae todas.
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.title, m.type, m.summary, bm25(memories_fts) AS score,
                    (m.review_state = 'needs_review') AS needs_review
             FROM memories_fts
             JOIN memories m ON m.rowid = memories_fts.rowid
             WHERE memories_fts MATCH ?1
               AND (?2 IS NULL OR m.project = ?2 OR m.scope = 'personal')
             ORDER BY rank
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, project, limit], map_index_full_row(false))?;
        rows.collect()
    }

    /// Como `search_index`, pero además trae el contenido completo en `cuerpo`, para los perfiles
    /// de verbosidad compacto y completo (RF-REC-04). El servicio decide si recortarlo.
    pub fn search_index_full(
        &self,
        query: &str,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.title, m.type, m.summary, bm25(memories_fts) AS score,
                    (m.review_state = 'needs_review') AS needs_review, m.content
             FROM memories_fts
             JOIN memories m ON m.rowid = memories_fts.rowid
             WHERE memories_fts MATCH ?1
               AND (?2 IS NULL OR m.project = ?2 OR m.scope = 'personal')
             ORDER BY rank
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, project, limit], map_index_full_row(true))?;
        rows.collect()
    }

    /// Elimina una memoria; devuelve `true` si existía (RF-MEM-06).
    pub fn delete_memory(&self, id: &str) -> rusqlite::Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM memories WHERE id=?1", params![id])?;
        Ok(n > 0)
    }

    /// Cuenta las memorias guardadas.
    pub fn count_memories(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
    }

    /// Cambia la importancia de una memoria (RF-MEM-04). Devuelve `true` si existía.
    pub fn set_importance(&self, id: &str, importance: Importance) -> rusqlite::Result<bool> {
        let n = self.conn.execute(
            "UPDATE memories SET importance=?2, updated_at=?3 WHERE id=?1",
            params![id, importance.as_str(), now_ms()],
        )?;
        Ok(n > 0)
    }

    /// Cambia el nivel de escalonamiento de una memoria (RF-TOK-02). Devuelve `true` si existía.
    pub fn set_tier(&self, id: &str, tier: Tier) -> rusqlite::Result<bool> {
        let n = self.conn.execute(
            "UPDATE memories SET tier=?2, updated_at=?3 WHERE id=?1",
            params![id, tier.as_str(), now_ms()],
        )?;
        Ok(n > 0)
    }

    /// Escalonamiento automático por antigüedad de acceso (RF-TOK-02/03): pasa a tibio las
    /// memorias calientes sin acceso desde `warm_cutoff_ms` (archivando su contenido completo, que
    /// sigue recuperable), y a frío las tibias sin acceso desde `cold_cutoff_ms`. Nunca toca las
    /// fijadas. Devuelve `(a_tibio, a_frio)`.
    pub fn escalonar_tiers(
        &self,
        project: &str,
        warm_cutoff_ms: i64,
        cold_cutoff_ms: i64,
    ) -> rusqlite::Result<(usize, usize)> {
        let now = now_ms();
        let a_tibio = self.conn.execute(
            "UPDATE memories SET tier='warm', archived=content, updated_at=?3
             WHERE project=?1 AND tier='hot' AND importance!='pinned' AND accessed_at < ?2",
            params![project, warm_cutoff_ms, now],
        )?;
        // Al pasar a frío, la memoria es contexto añejo: se marca needs_review (paridad funcional
        // mem_review). Así una búsqueda/contexto que la devuelva avisa "verificar antes de confiar".
        let a_frio = self.conn.execute(
            "UPDATE memories SET tier='cold', review_state='needs_review', updated_at=?3
             WHERE project=?1 AND tier='warm' AND importance!='pinned' AND accessed_at < ?2",
            params![project, cold_cutoff_ms, now],
        )?;
        Ok((a_tibio, a_frio))
    }

    /// Memorias marcadas `needs_review` de un proyecto, en modo índice (paridad funcional
    /// `mem_review list`). Incluye las personales. Más recientes primero (por `updated_at`).
    pub fn needs_review_index(
        &self,
        project: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, type, summary, 0.0 AS score, 1 AS needs_review
             FROM memories
             WHERE (project=?1 OR scope='personal') AND review_state='needs_review'
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_index_row)?;
        rows.collect()
    }

    /// Marca una memoria como revisada: vuelve a `active` y refresca `accessed_at` (el repaso
    /// cuenta como uso, así no recae de inmediato en frío). NO la hace el sistema solo: la dispara
    /// el agente con `memory_review mark_reviewed`. Devuelve `true` si existía.
    pub fn mark_reviewed(&self, id: &str) -> rusqlite::Result<bool> {
        let now = now_ms();
        let n = self.conn.execute(
            "UPDATE memories SET review_state='active', accessed_at=?2, updated_at=?2 WHERE id=?1",
            params![id, now],
        )?;
        Ok(n > 0)
    }

    /// Registra el último prompt del usuario para un proyecto (best-effort, paridad funcional).
    /// Lo escribe el hook prompt-submit; un `memory_save` sin prompt explícito lo adjunta luego.
    pub fn record_last_prompt(
        &self,
        project: &str,
        session_id: Option<&str>,
        prompt: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO last_prompts (id, project, session_id, prompt, consumed, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5)",
            params![
                ulid::Ulid::new().to_string(),
                project,
                session_id,
                prompt,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    /// Toma (y consume) el último prompt no consumido de un proyecto, o `None` si no hay. Marcarlo
    /// consumido evita adjuntar el mismo prompt a varias memorias del mismo turno.
    pub fn take_last_prompt(&self, project: &str) -> rusqlite::Result<Option<String>> {
        let fila: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT id, prompt FROM last_prompts
                 WHERE project=?1 AND consumed=0 ORDER BY created_at DESC LIMIT 1",
                params![project],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match fila {
            Some((id, prompt)) => {
                self.conn.execute(
                    "UPDATE last_prompts SET consumed=1 WHERE id=?1",
                    params![id],
                )?;
                Ok(Some(prompt))
            }
            None => Ok(None),
        }
    }

    /// Poda las memorias efímeras de un proyecto sin acceso desde `before_ms` (RF-TOK-05).
    /// Devuelve cuántas se eliminaron.
    pub fn prune_ephemeral(&self, project: &str, before_ms: i64) -> rusqlite::Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM memories WHERE project=?1 AND importance='ephemeral' AND accessed_at < ?2",
            params![project, before_ms],
        )?;
        Ok(n)
    }

    /// Línea de tiempo de una memoria y las relacionadas con ella (por la tabla de relaciones),
    /// en orden cronológico de creación (RF-REC-09). Modo índice barato.
    pub fn related_timeline(&self, id: &str) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.title, m.type, m.summary, 0.0 AS score,
                    (m.review_state = 'needs_review') AS needs_review
             FROM memories m
             WHERE m.id = ?1
                OR m.id IN (SELECT to_id FROM relations WHERE from_id = ?1)
                OR m.id IN (SELECT from_id FROM relations WHERE to_id = ?1)
             ORDER BY m.created_at",
        )?;
        let rows = stmt.query_map(params![id], map_index_row)?;
        rows.collect()
    }

    /// Reasigna todas las memorias del proyecto `from` al proyecto `to` (consolidación, RF-MEM-09).
    /// Devuelve cuántas se movieron.
    pub fn move_project_memories(&self, from: &str, to: &str) -> rusqlite::Result<usize> {
        let n = self.conn.execute(
            "UPDATE memories SET project=?2, updated_at=?3 WHERE project=?1",
            params![from, to, now_ms()],
        )?;
        Ok(n)
    }

    /// Memorias fijadas (pinned) de un proyecto, en modo índice (RF-TOK-04).
    pub fn pinned_index(&self, project: &str, limit: u32) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, type, summary, 0.0 AS score,
                    (review_state = 'needs_review') AS needs_review
             FROM memories
             WHERE project=?1 AND importance='pinned'
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_index_row)?;
        rows.collect()
    }

    /// Memorias creadas o actualizadas desde `since_ms`, en modo índice (RF-TOK-04).
    pub fn changed_since_index(
        &self,
        project: &str,
        since_ms: i64,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, type, summary, 0.0 AS score,
                    (review_state = 'needs_review') AS needs_review
             FROM memories
             WHERE project=?1 AND updated_at >= ?2
             ORDER BY updated_at DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![project, since_ms, limit], map_index_row)?;
        rows.collect()
    }

    /// Cuenta filas de una tabla del esquema (para diagnóstico, RF-DIA-02). El nombre debe ser
    /// uno de los conocidos del esquema; nunca proviene de la entrada del usuario.
    pub fn count_table(&self, table: &str) -> rusqlite::Result<i64> {
        self.conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
    }

    /// Versión del esquema persistida en la base.
    pub fn schema_version(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))
    }

    /// Resultado de `PRAGMA integrity_check` ("ok" si la base está sana).
    pub fn integrity_check(&self) -> rusqlite::Result<String> {
        self.conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
    }

    /// Conteo de memorias por proyecto, de mayor a menor (RF-DIA-01).
    pub fn count_memories_by_project(&self) -> rusqlite::Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT project, COUNT(*) FROM memories GROUP BY project ORDER BY COUNT(*) DESC, project",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Conteo de memorias por tipo, de mayor a menor (RF-DIA-01).
    pub fn count_memories_by_type(&self) -> rusqlite::Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT type, COUNT(*) FROM memories GROUP BY type ORDER BY COUNT(*) DESC, type",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// Reconstruye el índice FTS de memorias desde la tabla base (reparación, RF-DIA-03).
    pub fn rebuild_fts_memories(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO memories_fts(memories_fts) VALUES('rebuild')",
            [],
        )?;
        Ok(())
    }

    /// Reconstruye el índice FTS de skills desde la tabla base (reparación, RF-DIA-03).
    pub fn rebuild_fts_skills(&self) -> rusqlite::Result<()> {
        self.conn
            .execute("INSERT INTO skills_fts(skills_fts) VALUES('rebuild')", [])?;
        Ok(())
    }

    /// Cantidad de documentos realmente indexados en un índice FTS5 de **contenido externo**
    /// (shadow table `<fts>_docsize`). Es lo que hay que comparar contra la tabla base para
    /// detectar una desincronización: `count(*)` sobre la propia tabla FTS lee la tabla base
    /// (contenido externo) y por eso siempre coincide, sin importar el estado real del índice.
    pub fn count_fts_docs(&self, fts_table: &str) -> rusqlite::Result<i64> {
        // `fts_table` es una constante interna ("memories_fts"/"skills_fts"), no entrada de usuario.
        self.conn.query_row(
            &format!("SELECT count(*) FROM {fts_table}_docsize"),
            [],
            |r| r.get(0),
        )
    }

    /// Cantidad de grupos `(proyecto, título)` con más de una memoria: duplicados (RF-DIA-02).
    pub fn duplicate_memory_groups(&self) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM (
                 SELECT project, title FROM memories GROUP BY project, title HAVING COUNT(*) > 1
             )",
            [],
            |r| r.get(0),
        )
    }

    /// Todas las memorias completas de un proyecto (o de todos), para exportar (RF-SYN).
    pub fn all_memories(&self, project: Option<&str>) -> rusqlite::Result<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project, type, title, what, why, where_, learned, content, summary,
                    importance, tier, scope, topic_key, review_state, prompt,
                    created_at, updated_at, accessed_at
             FROM memories
             WHERE (?1 IS NULL OR project = ?1)
             ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![project], map_memory)?;
        rows.collect()
    }

    /// Inserta o actualiza una memoria completa preservando su id (importación, RF-SYN). Devuelve
    /// `true` si era nueva. En conflicto, actualiza los campos mutables. Los triggers mantienen el
    /// índice FTS en sincronía.
    pub fn import_memory(&self, m: &Memory) -> rusqlite::Result<bool> {
        let nueva = self
            .conn
            .query_row("SELECT 1 FROM memories WHERE id=?1", params![m.id], |_| {
                Ok(())
            })
            .optional()?
            .is_none();
        self.conn.execute(
            "INSERT INTO memories
                (id, project, type, title, what, why, where_, learned, content, summary,
                 importance, tier, scope, topic_key, review_state, prompt,
                 token_est, created_at, updated_at, accessed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
             ON CONFLICT(id) DO UPDATE SET
                project=excluded.project, type=excluded.type, title=excluded.title,
                what=excluded.what, why=excluded.why, where_=excluded.where_,
                learned=excluded.learned, content=excluded.content, summary=excluded.summary,
                importance=excluded.importance, tier=excluded.tier, scope=excluded.scope,
                topic_key=excluded.topic_key, review_state=excluded.review_state,
                prompt=excluded.prompt, token_est=excluded.token_est,
                updated_at=excluded.updated_at",
            params![
                m.id,
                m.project,
                m.kind.as_str(),
                m.title,
                m.what,
                m.why,
                m.where_,
                m.learned,
                m.content,
                m.summary,
                m.importance.as_str(),
                m.tier.as_str(),
                m.scope.as_str(),
                m.topic_key,
                m.review_state.as_str(),
                m.prompt,
                estimate_tokens(&m.content),
                m.created_at,
                m.updated_at,
                m.accessed_at,
            ],
        )?;
        Ok(nueva)
    }

    /// Actualiza una memoria existente preservando su id y su fecha de creación (RF-MEM-05).
    /// Devuelve `true` si existía. El trigger de FTS reindexa automáticamente.
    pub fn update_memory(&self, id: &str, m: &NewMemory) -> rusqlite::Result<bool> {
        let n = self.conn.execute(
            "UPDATE memories
                SET project=?2, type=?3, title=?4, what=?5, why=?6, where_=?7,
                    learned=?8, content=?9, summary=?10, scope=?11, topic_key=?12,
                    prompt=COALESCE(?13, prompt), token_est=?14, updated_at=?15
             WHERE id=?1",
            params![
                id,
                m.project,
                m.kind.as_str(),
                m.title,
                m.what,
                m.why,
                m.where_,
                m.learned,
                m.content,
                m.summary,
                m.scope.as_str(),
                m.topic_key,
                m.prompt,
                estimate_tokens(&m.content),
                now_ms(),
            ],
        )?;
        Ok(n > 0)
    }

    /// Memorias más recientes en modo índice, para el contexto de sesión sin consulta (RF-REC-08).
    pub fn recent_index(
        &self,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        // Incluye SIEMPRE las memorias personales (transversales al usuario), igual que la búsqueda.
        let mut stmt = self.conn.prepare(
            "SELECT id, title, type, summary, 0.0 AS score,
                    (review_state = 'needs_review') AS needs_review
             FROM memories
             WHERE (?1 IS NULL OR project = ?1 OR scope = 'personal')
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_index_row)?;
        rows.collect()
    }

    /// Inicia una sesión y devuelve su identificador (ULID), en estado `open` (RF-SES-01).
    pub fn start_session(&self, s: &NewSession) -> rusqlite::Result<String> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO sessions (id, project, agent_id, task, started_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'open')",
            params![id, s.project, s.agent_id, s.task, now_ms()],
        )?;
        Ok(id)
    }

    /// Recupera una sesión por id.
    pub fn get_session(&self, id: &str) -> rusqlite::Result<Option<Session>> {
        self.conn
            .query_row(
                "SELECT id, project, agent_id, task, summary, started_at, ended_at, status
                 FROM sessions WHERE id = ?1",
                params![id],
                map_session,
            )
            .optional()
    }

    /// Cierra una sesión abierta: fija `ended_at`, `status='closed'` y el resumen dado
    /// (RF-SES-02). Devuelve la sesión cerrada, o `None` si no existe o ya estaba cerrada.
    pub fn close_session(&self, id: &str, summary: &str) -> rusqlite::Result<Option<Session>> {
        let cambiadas = self.conn.execute(
            "UPDATE sessions SET ended_at = ?2, status = 'closed', summary = ?3
             WHERE id = ?1 AND status = 'open'",
            params![id, now_ms(), summary],
        )?;
        if cambiadas == 0 {
            return Ok(None);
        }
        self.get_session(id)
    }

    /// Sesiones más recientes (para la consulta de sesiones previas).
    pub fn recent_sessions(
        &self,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project, agent_id, task, summary, started_at, ended_at, status
             FROM sessions
             WHERE (?1 IS NULL OR project = ?1)
             ORDER BY started_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_session)?;
        rows.collect()
    }

    /// Marca de inicio de la sesión más reciente de un proyecto (RF-TOK-04), o `None` si no hay.
    pub fn last_session_start(&self, project: &str) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT started_at FROM sessions WHERE project=?1 ORDER BY started_at DESC LIMIT 1",
                params![project],
                |r| r.get(0),
            )
            .optional()
    }

    /// Marca de tiempo del evento más reciente de un tipo en un proyecto (p. ej. el último guardado),
    /// o `None` si no hay ninguno. La usa el nudge de guardado del hook (paridad funcional).
    pub fn last_event_time(
        &self,
        project: &str,
        kind: turtle_core::event::EventKind,
    ) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT created_at FROM events WHERE project=?1 AND kind=?2 \
                 ORDER BY created_at DESC LIMIT 1",
                params![project, kind.as_str()],
                |r| r.get(0),
            )
            .optional()
    }

    /// Guarda un checkpoint de trabajo en curso y devuelve su id (RF-SES-04).
    pub fn save_checkpoint(&self, project: &str, content: &str) -> rusqlite::Result<String> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO checkpoints (id, project, content, created_at) VALUES (?1,?2,?3,?4)",
            params![id, project, content, now_ms()],
        )?;
        Ok(id)
    }

    /// El checkpoint más reciente de un proyecto, o `None` (RF-SES-04).
    pub fn latest_checkpoint(&self, project: &str) -> rusqlite::Result<Option<Checkpoint>> {
        self.conn
            .query_row(
                "SELECT id, project, content, created_at FROM checkpoints
                 WHERE project=?1 ORDER BY created_at DESC LIMIT 1",
                params![project],
                map_checkpoint,
            )
            .optional()
    }

    /// Títulos de las memorias creadas en un proyecto desde `from_ms` (inclusive), en orden
    /// cronológico. Sirve para construir un resumen local de "lo realizado" (RF-SES-02).
    pub fn memory_titles_since(
        &self,
        project: &str,
        from_ms: i64,
    ) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT title FROM memories
             WHERE project = ?1 AND created_at >= ?2
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![project, from_ms], |r| r.get::<_, String>(0))?;
        rows.collect()
    }

    /// Registra o actualiza un agente, identificado por `(project, label)` (RF-AGN-01). Lo deja
    /// en estado `working` y refresca tarea, rama y `last_seen_at`. Devuelve el agente resultante.
    pub fn register_agent(&self, a: &NewAgent) -> rusqlite::Result<Agent> {
        let now = now_ms();
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO agents (id, project, label, status, task, branch, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, 'working', ?4, ?5, ?6, ?6)
             ON CONFLICT(project, label) DO UPDATE SET
                 status = 'working',
                 task = excluded.task,
                 branch = excluded.branch,
                 last_seen_at = excluded.last_seen_at",
            params![id, a.project, a.label, a.task, a.branch, now],
        )?;
        self.get_agent(&a.project, &a.label)
            .map(|o| o.expect("el agente recién registrado debe existir"))
    }

    /// Marca un agente como `idle` (sin sesión activa) y refresca `last_seen_at`.
    pub fn set_agent_idle(&self, project: &str, label: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE agents SET status = 'idle', last_seen_at = ?3 WHERE project = ?1 AND label = ?2",
            params![project, label, now_ms()],
        )?;
        Ok(())
    }

    fn get_agent(&self, project: &str, label: &str) -> rusqlite::Result<Option<Agent>> {
        self.conn
            .query_row(
                "SELECT id, project, label, status, task, branch, created_at, last_seen_at
                 FROM agents WHERE project = ?1 AND label = ?2",
                params![project, label],
                map_agent,
            )
            .optional()
    }

    /// Lista los agentes registrados, del más recientemente visto al más antiguo (RF-N-01).
    pub fn list_agents(&self, project: Option<&str>, limit: u32) -> rusqlite::Result<Vec<Agent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project, label, status, task, branch, created_at, last_seen_at
             FROM agents
             WHERE (?1 IS NULL OR project = ?1)
             ORDER BY last_seen_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_agent)?;
        rows.collect()
    }

    /// Registra un evento en el bus de actividad (RF-COM-01, RF-BD-05).
    pub fn record_event(&self, e: &NewEvent) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO events (id, project, agent, kind, target_id, summary, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                ulid::Ulid::new().to_string(),
                e.project,
                e.agent,
                e.kind.as_str(),
                e.target_id,
                e.summary,
                now_ms(),
            ],
        )?;
        Ok(())
    }

    /// Lista los eventos más recientes (feed de actividad), del más nuevo al más antiguo.
    pub fn list_events(&self, project: Option<&str>, limit: u32) -> rusqlite::Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project, agent, kind, target_id, summary, created_at
             FROM events
             WHERE (?1 IS NULL OR project = ?1)
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_event)?;
        rows.collect()
    }

    /// Envía un mensaje a la bandeja (RF-COM-03). `to_agent` `None` es difusión.
    pub fn send_message(&self, m: &NewMessage) -> rusqlite::Result<String> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO messages (id, project, from_agent, to_agent, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, m.project, m.from_agent, m.to_agent, m.body, now_ms()],
        )?;
        Ok(id)
    }

    /// Bandeja de un destinatario: mensajes dirigidos a su rótulo o por difusión (RF-COM-06).
    /// Con `only_pending`, solo los no entregados.
    pub fn inbox(
        &self,
        project: &str,
        recipient: &str,
        only_pending: bool,
        limit: u32,
    ) -> rusqlite::Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project, from_agent, to_agent, body, created_at, read_at
             FROM messages
             WHERE project = ?1
               AND (to_agent = ?2 OR to_agent IS NULL)
               AND (?3 = 0 OR read_at IS NULL)
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![project, recipient, only_pending as i64, limit],
            map_message,
        )?;
        rows.collect()
    }

    /// Marca como entregados los mensajes pendientes de un destinatario. Devuelve cuántos.
    pub fn mark_delivered(&self, project: &str, recipient: &str) -> rusqlite::Result<usize> {
        let n = self.conn.execute(
            "UPDATE messages SET read_at = ?3
             WHERE project = ?1 AND (to_agent = ?2 OR to_agent IS NULL) AND read_at IS NULL",
            params![project, recipient, now_ms()],
        )?;
        Ok(n)
    }

    /// Candidatos a duplicado/conflicto: memorias del proyecto que matchean `query` por FTS5,
    /// excluyendo `exclude_id`, ordenadas por relevancia (RF-CNF-01).
    pub fn find_candidates(
        &self,
        query: &str,
        project: Option<&str>,
        exclude_id: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<MemoryIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.title, m.type, m.summary, bm25(memories_fts) AS score,
                    (m.review_state = 'needs_review') AS needs_review
             FROM memories_fts
             JOIN memories m ON m.rowid = memories_fts.rowid
             WHERE memories_fts MATCH ?1
               AND (?2 IS NULL OR m.project = ?2)
               AND m.id != ?3
             ORDER BY rank
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(
            params![query, project, exclude_id, limit],
            map_index_full_row(false),
        )?;
        rows.collect()
    }

    /// Registra una relación entre dos memorias y devuelve su id (RF-CNF-02).
    pub fn add_relation(&self, r: &NewRelation) -> rusqlite::Result<String> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO relations (id, from_id, to_id, kind, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, r.from_id, r.to_id, r.kind.as_str(), r.note, now_ms()],
        )?;
        Ok(id)
    }

    /// Relaciones que tocan una memoria (como origen o destino), de la más nueva a la más vieja.
    pub fn list_relations(&self, memory_id: &str) -> rusqlite::Result<Vec<Relation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_id, to_id, kind, note, created_at
             FROM relations
             WHERE from_id = ?1 OR to_id = ?1
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![memory_id], map_relation)?;
        rows.collect()
    }

    /// Inserta o actualiza una skill por (proyecto, nombre, tipo); devuelve su id (RF-SKL-02/05).
    /// Hace la reingesta de `skills/`/`agents/` idempotente: reescanear no duplica.
    pub fn upsert_skill(&self, s: &NewSkill) -> rusqlite::Result<String> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO skills
                (id, project, name, kind, when_to_use, content, tags, source, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)
             ON CONFLICT(project, name, kind) DO UPDATE SET
                when_to_use=excluded.when_to_use,
                content=excluded.content,
                tags=excluded.tags,
                source=excluded.source,
                updated_at=excluded.updated_at",
            params![
                id,
                s.project,
                s.name,
                s.kind.as_str(),
                s.when_to_use,
                s.content,
                s.tags,
                s.source,
                now_ms(),
            ],
        )?;
        // Devuelve el id real (el nuevo si fue alta, o el existente si hubo conflicto).
        self.conn.query_row(
            "SELECT id FROM skills WHERE project=?1 AND name=?2 AND kind=?3",
            params![s.project, s.name, s.kind.as_str()],
            |r| r.get(0),
        )
    }

    /// Búsqueda de skills en modo índice barato (RF-SKL-03): id, nombre, tipo y cuándo usarla.
    /// Incluye siempre las skills globales (proyecto vacío) además de las del proyecto dado.
    pub fn search_skills(
        &self,
        query: &str,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<SkillIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.name, s.kind, s.when_to_use, bm25(skills_fts) AS score
             FROM skills_fts
             JOIN skills s ON s.rowid = skills_fts.rowid
             WHERE skills_fts MATCH ?1
               AND (?2 IS NULL OR s.project = ?2 OR s.project = '')
             ORDER BY rank
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![query, project, limit], map_skill_row)?;
        rows.collect()
    }

    /// Skills más recientes en modo índice (sin consulta), para listar (RF-SKL-03).
    pub fn recent_skills(
        &self,
        project: Option<&str>,
        limit: u32,
    ) -> rusqlite::Result<Vec<SkillIndexRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, when_to_use, 0.0 AS score
             FROM skills
             WHERE (?1 IS NULL OR project = ?1 OR project = '')
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_skill_row)?;
        rows.collect()
    }

    /// Carga el contenido completo de una skill por id (RF-SKL-04).
    pub fn get_skill(&self, id: &str) -> rusqlite::Result<Option<Skill>> {
        self.conn
            .query_row(
                "SELECT id, project, name, kind, when_to_use, content, tags, source,
                        intensity, created_at, updated_at
                 FROM skills WHERE id=?1",
                params![id],
                map_skill,
            )
            .optional()
    }

    /// Cambia la intensidad de activación de una skill (RF-SKL-07). Devuelve `true` si existía.
    pub fn set_skill_intensity(&self, id: &str, intensidad: Intensidad) -> rusqlite::Result<bool> {
        let n = self.conn.execute(
            "UPDATE skills SET intensity=?2, updated_at=?3 WHERE id=?1",
            params![id, intensidad.as_str(), now_ms()],
        )?;
        Ok(n > 0)
    }

    /// Skills de comportamiento activas (intensidad distinta de `off`) del proyecto y globales,
    /// para inyectarlas en el contexto de sesión (RF-SKL-07).
    pub fn active_behavior_skills(
        &self,
        project: &str,
        limit: u32,
    ) -> rusqlite::Result<Vec<Skill>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project, name, kind, when_to_use, content, tags, source,
                    intensity, created_at, updated_at
             FROM skills
             WHERE kind='behavior' AND intensity != 'off' AND (project=?1 OR project='')
             ORDER BY updated_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit], map_skill)?;
        rows.collect()
    }

    /// Cuenta las skills almacenadas.
    pub fn count_skills(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
    }
}

fn map_skill_row(r: &Row) -> rusqlite::Result<SkillIndexRow> {
    Ok(SkillIndexRow {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: SkillKind::parse(&r.get::<_, String>(2)?).unwrap_or(SkillKind::Knowledge),
        when_to_use: r.get(3)?,
        score: r.get(4)?,
    })
}

fn map_skill(r: &Row) -> rusqlite::Result<Skill> {
    Ok(Skill {
        id: r.get(0)?,
        project: r.get(1)?,
        name: r.get(2)?,
        kind: SkillKind::parse(&r.get::<_, String>(3)?).unwrap_or(SkillKind::Knowledge),
        when_to_use: r.get(4)?,
        content: r.get(5)?,
        tags: r.get(6)?,
        source: r.get(7)?,
        intensity: Intensidad::parse(&r.get::<_, String>(8)?).unwrap_or(Intensidad::Off),
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

fn map_relation(r: &Row) -> rusqlite::Result<Relation> {
    Ok(Relation {
        id: r.get(0)?,
        from_id: r.get(1)?,
        to_id: r.get(2)?,
        kind: RelationKind::parse(&r.get::<_, String>(3)?).unwrap_or(RelationKind::RelatesTo),
        note: r.get(4)?,
        created_at: r.get(5)?,
    })
}

fn map_message(r: &Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        project: r.get(1)?,
        from_agent: r.get(2)?,
        to_agent: r.get(3)?,
        body: r.get(4)?,
        created_at: r.get(5)?,
        read_at: r.get(6)?,
    })
}

fn map_event(r: &Row) -> rusqlite::Result<Event> {
    Ok(Event {
        id: r.get(0)?,
        project: r.get(1)?,
        agent: r.get(2)?,
        kind: EventKind::parse(&r.get::<_, String>(3)?).unwrap_or(EventKind::MemorySaved),
        target_id: r.get(4)?,
        summary: r.get(5)?,
        created_at: r.get(6)?,
    })
}

fn map_agent(r: &Row) -> rusqlite::Result<Agent> {
    Ok(Agent {
        id: r.get(0)?,
        project: r.get(1)?,
        label: r.get(2)?,
        status: AgentStatus::parse(&r.get::<_, String>(3)?).unwrap_or(AgentStatus::Idle),
        task: r.get(4)?,
        branch: r.get(5)?,
        created_at: r.get(6)?,
        last_seen_at: r.get(7)?,
    })
}

/// Mapea una fila del índice de búsqueda que trae el flag `needs_review` en la columna 5 y,
/// opcionalmente, el contenido completo en la columna 6 (perfiles compacto/completo). Devuelve un
/// closure para reusarlo en `search_index`/`search_index_full`/`find_candidates`.
fn map_index_full_row(con_cuerpo: bool) -> impl Fn(&Row) -> rusqlite::Result<MemoryIndexRow> {
    move |r: &Row| {
        Ok(MemoryIndexRow {
            id: r.get(0)?,
            title: r.get(1)?,
            kind: MemoryKind::parse(&r.get::<_, String>(2)?).unwrap_or(MemoryKind::Note),
            summary: r.get(3)?,
            score: r.get(4)?,
            needs_review: r.get::<_, i64>(5)? != 0,
            cuerpo: if con_cuerpo {
                Some(r.get::<_, String>(6)?)
            } else {
                None
            },
        })
    }
}

fn map_index_row(r: &Row) -> rusqlite::Result<MemoryIndexRow> {
    Ok(MemoryIndexRow {
        id: r.get(0)?,
        title: r.get(1)?,
        kind: MemoryKind::parse(&r.get::<_, String>(2)?).unwrap_or(MemoryKind::Note),
        summary: r.get(3)?,
        score: r.get(4)?,
        // Columna 5 (cuando está): 1 si la memoria está marcada needs_review.
        needs_review: r.get::<_, Option<i64>>(5).unwrap_or(None).unwrap_or(0) != 0,
        cuerpo: None,
    })
}

fn map_checkpoint(r: &Row) -> rusqlite::Result<Checkpoint> {
    Ok(Checkpoint {
        id: r.get(0)?,
        project: r.get(1)?,
        content: r.get(2)?,
        created_at: r.get(3)?,
    })
}

fn map_session(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        project: r.get(1)?,
        agent_id: r.get(2)?,
        task: r.get(3)?,
        summary: r.get(4)?,
        started_at: r.get(5)?,
        ended_at: r.get(6)?,
        status: SessionStatus::parse(&r.get::<_, String>(7)?).unwrap_or(SessionStatus::Open),
    })
}

fn map_memory(r: &Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        project: r.get(1)?,
        kind: MemoryKind::parse(&r.get::<_, String>(2)?).unwrap_or(MemoryKind::Note),
        title: r.get(3)?,
        what: r.get(4)?,
        why: r.get(5)?,
        where_: r.get(6)?,
        learned: r.get(7)?,
        content: r.get(8)?,
        summary: r.get(9)?,
        importance: Importance::parse(&r.get::<_, String>(10)?).unwrap_or(Importance::Normal),
        tier: Tier::parse(&r.get::<_, String>(11)?).unwrap_or(Tier::Hot),
        scope: Scope::parse(&r.get::<_, String>(12)?).unwrap_or(Scope::Project),
        topic_key: r.get(13)?,
        review_state: ReviewState::parse(&r.get::<_, String>(14)?).unwrap_or(ReviewState::Active),
        prompt: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        accessed_at: r.get(18)?,
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Estimación heurística de tokens (~4 bytes/token, decisión A-4 de la arquitectura).
fn estimate_tokens(s: &str) -> i64 {
    s.len().div_ceil(4) as i64
}

/// Ruta de la base por convención de cada sistema operativo (RNF-INS-05).
/// `None` si no se puede resolver el directorio de datos del usuario.
pub fn default_db_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "turtle").map(|d| d.data_dir().join("turtle.db"))
}

/// Migración v1: memorias + índice FTS5 (con triggers de sincronía) + sesiones.
const MIGRATION_V1: &str = r#"
CREATE TABLE memories (
  id          TEXT PRIMARY KEY,
  project     TEXT NOT NULL,
  type        TEXT NOT NULL,
  title       TEXT NOT NULL,
  what        TEXT,
  why         TEXT,
  where_      TEXT,
  learned     TEXT,
  content     TEXT NOT NULL,
  summary     TEXT,
  importance  TEXT NOT NULL DEFAULT 'normal',
  tier        TEXT NOT NULL DEFAULT 'hot',
  archived    TEXT,
  token_est   INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  accessed_at INTEGER NOT NULL
);
CREATE INDEX idx_mem_project ON memories(project);

CREATE VIRTUAL TABLE memories_fts USING fts5(
  title, content, summary,
  content='memories', content_rowid='rowid',
  tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, title, content, summary)
  VALUES (new.rowid, new.title, new.content, new.summary);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, title, content, summary)
  VALUES ('delete', old.rowid, old.title, old.content, old.summary);
END;
CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, title, content, summary)
  VALUES ('delete', old.rowid, old.title, old.content, old.summary);
  INSERT INTO memories_fts(rowid, title, content, summary)
  VALUES (new.rowid, new.title, new.content, new.summary);
END;

CREATE TABLE sessions (
  id         TEXT PRIMARY KEY,
  project    TEXT NOT NULL,
  agent_id   TEXT,
  task       TEXT,
  summary    TEXT,
  started_at INTEGER NOT NULL,
  ended_at   INTEGER,
  status     TEXT NOT NULL DEFAULT 'open'
);
"#;

/// Migración v2: registro de agentes (RF-AGN-*). Identidad por `(project, label)`.
const MIGRATION_V2: &str = r#"
CREATE TABLE agents (
  id           TEXT PRIMARY KEY,
  project      TEXT NOT NULL,
  label        TEXT NOT NULL,
  status       TEXT NOT NULL DEFAULT 'idle',
  task         TEXT,
  branch       TEXT,
  created_at   INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  UNIQUE(project, label)
);
CREATE INDEX idx_agents_project ON agents(project);
"#;

/// Migración v3: bus de actividad (RF-COM-01, RF-BD-05). Eventos atribuidos a un agente.
const MIGRATION_V3: &str = r#"
CREATE TABLE events (
  id         TEXT PRIMARY KEY,
  project    TEXT NOT NULL,
  agent      TEXT,
  kind       TEXT NOT NULL,
  target_id  TEXT,
  summary    TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_events_project ON events(project, created_at);
"#;

/// Migración v4: bandeja de mensajes entre agentes (RF-COM-03..06).
const MIGRATION_V4: &str = r#"
CREATE TABLE messages (
  id         TEXT PRIMARY KEY,
  project    TEXT NOT NULL,
  from_agent TEXT,
  to_agent   TEXT,
  body       TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  read_at    INTEGER
);
CREATE INDEX idx_messages_inbox ON messages(project, to_agent, read_at);
"#;

/// Migración v5: relaciones entre memorias (deduplicación y conflictos, RF-CNF-02).
const MIGRATION_V5: &str = r#"
CREATE TABLE relations (
  id         TEXT PRIMARY KEY,
  from_id    TEXT NOT NULL,
  to_id      TEXT NOT NULL,
  kind       TEXT NOT NULL,
  note       TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_relations_from ON relations(from_id);
CREATE INDEX idx_relations_to ON relations(to_id);
"#;

/// Migración v6: skills (capa de skills, RF-SKL-01/02) + índice FTS5 con triggers de sincronía.
/// Ingiere `skills/` y `agents/` del ecosistema; `project` vacío = global.
const MIGRATION_V6: &str = r#"
CREATE TABLE skills (
  id          TEXT PRIMARY KEY,
  project     TEXT NOT NULL DEFAULT '',
  name        TEXT NOT NULL,
  kind        TEXT NOT NULL,
  when_to_use TEXT,
  content     TEXT NOT NULL,
  tags        TEXT,
  source      TEXT,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  UNIQUE(project, name, kind)
);
CREATE INDEX idx_skills_project ON skills(project);

CREATE VIRTUAL TABLE skills_fts USING fts5(
  name, when_to_use, content, tags,
  content='skills', content_rowid='rowid',
  tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER skills_ai AFTER INSERT ON skills BEGIN
  INSERT INTO skills_fts(rowid, name, when_to_use, content, tags)
  VALUES (new.rowid, new.name, new.when_to_use, new.content, new.tags);
END;
CREATE TRIGGER skills_ad AFTER DELETE ON skills BEGIN
  INSERT INTO skills_fts(skills_fts, rowid, name, when_to_use, content, tags)
  VALUES ('delete', old.rowid, old.name, old.when_to_use, old.content, old.tags);
END;
CREATE TRIGGER skills_au AFTER UPDATE ON skills BEGIN
  INSERT INTO skills_fts(skills_fts, rowid, name, when_to_use, content, tags)
  VALUES ('delete', old.rowid, old.name, old.when_to_use, old.content, old.tags);
  INSERT INTO skills_fts(rowid, name, when_to_use, content, tags)
  VALUES (new.rowid, new.name, new.when_to_use, new.content, new.tags);
END;
"#;

/// Migración v7: intensidad de activación de skills de comportamiento (RF-SKL-07).
const MIGRATION_V7: &str = r#"
ALTER TABLE skills ADD COLUMN intensity TEXT NOT NULL DEFAULT 'off';
"#;

/// Migración v8: checkpoints de trabajo en curso (supervivencia a la compactación, RF-SES-04).
const MIGRATION_V8: &str = r#"
CREATE TABLE checkpoints (
  id         TEXT PRIMARY KEY,
  project    TEXT NOT NULL,
  content    TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_checkpoints_project ON checkpoints(project, created_at);
"#;

/// Migración v9: paridad funcional en cuatro dimensiones de la memoria.
///
/// - `scope` (project|personal): una memoria `personal` es transversal al usuario y se incluye en
///   todos los proyectos. Default `project` = el comportamiento de siempre.
/// - `topic_key`: clave estable de tema evolutivo; guardar con un `topic_key` ya existente
///   (mismo project+scope) hace UPSERT. El índice único parcial garantiza unicidad solo cuando hay
///   clave (las memorias sin tema no chocan entre sí).
/// - `review_state` (active|needs_review): ciclo de vida; el escalonamiento a frío marca
///   `needs_review` (contexto añejo).
/// - `prompt`: prompt del usuario que originó la memoria (best-effort).
///
/// Todas las columnas son nullable o traen default, así que cada `ADD COLUMN` es instantáneo y NO
/// toca el índice FTS5 de contenido externo (sus triggers solo proyectan title/content/summary, que
/// no cambian). La tabla `last_prompts` guarda, liviana, el último prompt por proyecto para que un
/// `memory_save` sin `prompt` explícito pueda adjuntar el último no consumido (cierra el círculo con
/// el hook prompt-submit). Sin contenido externo: no necesita índice FTS.
const MIGRATION_V9: &str = r#"
ALTER TABLE memories ADD COLUMN scope        TEXT NOT NULL DEFAULT 'project';
ALTER TABLE memories ADD COLUMN topic_key    TEXT;
ALTER TABLE memories ADD COLUMN review_state TEXT NOT NULL DEFAULT 'active';
ALTER TABLE memories ADD COLUMN prompt       TEXT;

CREATE UNIQUE INDEX idx_mem_topic
  ON memories(project, scope, topic_key)
  WHERE topic_key IS NOT NULL;

CREATE INDEX idx_mem_scope ON memories(scope);

CREATE TABLE last_prompts (
  id         TEXT PRIMARY KEY,
  project    TEXT NOT NULL,
  session_id TEXT,
  prompt     TEXT NOT NULL,
  consumed   INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_last_prompts_project ON last_prompts(project, created_at);
"#;

/// Migración v10: historial temporal de temas evolutivos (versionado temporal de temas). Cuando un
/// `memory_save` con un `topic_key` existente actualiza la memoria viva, la versión anterior se
/// archiva en `memory_versions` con su intervalo de validez. La tabla NO tiene contenido externo
/// (no necesita FTS): el historial no aparece en la búsqueda normal, solo bajo demanda por id.
const MIGRATION_V10: &str = r#"
CREATE TABLE memory_versions (
  id         TEXT PRIMARY KEY,
  memory_id  TEXT NOT NULL,
  project    TEXT NOT NULL,
  type       TEXT NOT NULL,
  title      TEXT NOT NULL,
  what       TEXT,
  why        TEXT,
  where_     TEXT,
  learned    TEXT,
  content    TEXT NOT NULL,
  summary    TEXT,
  valid_from INTEGER NOT NULL,
  valid_to   INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX idx_memver_memory ON memory_versions(memory_id, valid_to);
"#;

/// Migración v11: semántica **opt-in**. `settings` (clave/valor) guarda si la semántica está
/// prendida y con qué modelo; `memory_embeddings` guarda el vector por memoria (BLOB f32 LE). Ambas
/// vacías/sin uso salvo que el usuario corra `turtle semantic on`: no afectan a quien usa solo FTS.
const MIGRATION_V11: &str = r#"
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE memory_embeddings (
  memory_id  TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
  model      TEXT NOT NULL,
  dim        INTEGER NOT NULL,
  vec        BLOB NOT NULL,
  updated_at INTEGER NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::{Db, SCHEMA_VERSION};
    use turtle_core::memory::{MemoryKind, NewMemory, Scope};

    fn ejemplo() -> NewMemory {
        NewMemory {
            what: Some("El núcleo se implementa en Rust".into()),
            why: Some("Tipado fuerte y binarios autocontenidos".into()),
            summary: Some("Núcleo en Rust".into()),
            ..NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Decision,
                "Usar Rust para el núcleo".into(),
                "Decidimos Rust por el tipado y por la búsqueda vectorial.".into(),
            )
        }
    }

    #[test]
    fn migracion_e_insercion_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_memory(&ejemplo()).unwrap();
        let got = db
            .get_memory(&id)
            .unwrap()
            .expect("la memoria debe existir");
        assert_eq!(got.title, "Usar Rust para el núcleo");
        assert_eq!(got.kind, MemoryKind::Decision);
        assert_eq!(
            got.why.as_deref(),
            Some("Tipado fuerte y binarios autocontenidos")
        );
        assert!(got.created_at > 0);
        assert!(got.accessed_at >= got.created_at);
    }

    #[test]
    fn get_inexistente_es_none() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.get_memory("no-existe").unwrap().is_none());
    }

    #[test]
    fn busqueda_indice_es_barata() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_memory(&ejemplo()).unwrap();
        // "tipado" aparece en el contenido: prueba que el FTS indexa el cuerpo.
        let rows = db.search_index("tipado", None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].title, "Usar Rust para el núcleo");
    }

    #[test]
    fn filtro_por_proyecto() {
        let db = Db::open_in_memory().unwrap();
        db.insert_memory(&ejemplo()).unwrap();
        assert!(db
            .search_index("Rust", Some("otro-proyecto"), 10)
            .unwrap()
            .is_empty());
        assert_eq!(
            db.search_index("Rust", Some("turtle"), 10).unwrap().len(),
            1
        );
    }

    #[test]
    fn borrar_quita_del_indice_fts() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_memory(&ejemplo()).unwrap();
        assert!(db.delete_memory(&id).unwrap());
        assert!(!db.delete_memory(&id).unwrap());
        assert!(db.search_index("Rust", None, 10).unwrap().is_empty());
        assert_eq!(db.count_memories().unwrap(), 0);
    }

    #[test]
    fn rebuild_fts_resincroniza_indice_desincronizado() {
        // Cubre la ruta de reparación de `turtle doctor --reparar` (RF-DIA-03): si el índice FTS
        // queda desincronizado de la tabla base, reconstruirlo los vuelve a igualar.
        let db = Db::open_in_memory().unwrap();
        db.insert_memory(&ejemplo()).unwrap();
        db.insert_memory(&ejemplo()).unwrap();
        assert_eq!(db.count_memories().unwrap(), 2);
        assert_eq!(db.count_table("memories_fts").unwrap(), 2);

        // Desincronizar a propósito: quito el trigger de inserción y agrego una memoria a la tabla
        // base. El índice FTS real queda con una entrada de MENOS que la base.
        db.conn.execute("DROP TRIGGER memories_ai", []).unwrap();
        db.insert_memory(&ejemplo()).unwrap();
        assert_eq!(db.count_memories().unwrap(), 3, "la base subió a 3");
        // `count(*)` sobre el FTS de contenido externo lee la base: NO detecta la desincronización.
        assert_eq!(db.count_table("memories_fts").unwrap(), 3);
        // El tamaño real del índice (shadow `_docsize`) sí la refleja — esto es lo que mira doctor.
        assert_eq!(
            db.count_fts_docs("memories_fts").unwrap(),
            2,
            "el índice real quedó desincronizado (le falta la última memoria)"
        );

        // Reparar: reconstruir el índice desde la tabla base lo vuelve a sincronizar.
        db.rebuild_fts_memories().unwrap();
        assert_eq!(
            db.count_fts_docs("memories_fts").unwrap(),
            db.count_memories().unwrap(),
            "tras reparar, índice real y base coinciden"
        );
        assert_eq!(db.count_fts_docs("memories_fts").unwrap(), 3);
    }

    #[test]
    fn concurrencia_dos_conexiones_wal_sin_perdida() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turtle.db");
        let a = Db::open(&path).unwrap();
        let b = Db::open(&path).unwrap();
        for _ in 0..50 {
            a.insert_memory(&ejemplo()).unwrap();
            b.insert_memory(&ejemplo()).unwrap();
        }
        assert_eq!(a.count_memories().unwrap(), 100);
        assert_eq!(b.count_memories().unwrap(), 100);
    }

    #[test]
    fn record_event_inserta_y_es_legible() {
        use turtle_core::event::{EventKind, NewEvent};
        let db = Db::open_in_memory().unwrap();
        db.record_event(&NewEvent {
            project: "turtle".into(),
            agent: Some("backend".into()),
            kind: EventKind::ToolUsed,
            target_id: None,
            summary: Some("Read".into()),
        })
        .unwrap();
        let eventos = db.list_events(Some("turtle"), 10).unwrap();
        assert_eq!(eventos.len(), 1);
        assert_eq!(eventos[0].kind, EventKind::ToolUsed);
        assert_eq!(eventos[0].summary.as_deref(), Some("Read"));
    }

    #[test]
    fn reabrir_base_no_remigra_y_conserva_version() {
        // Cubre la salida temprana de `migrate`: abrir una base ya migrada deja la versión intacta
        // y no corrompe el esquema (el hot path del arranque depende de esto).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turtle.db");
        {
            let a = Db::open(&path).unwrap();
            a.insert_memory(&ejemplo()).unwrap();
            assert_eq!(a.schema_version().unwrap(), SCHEMA_VERSION);
        }
        // Segunda apertura: migrate ve version == SCHEMA_VERSION y retorna temprano.
        let b = Db::open(&path).unwrap();
        assert_eq!(b.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(b.count_memories().unwrap(), 1);
        assert_eq!(b.search_index("Rust", None, 10).unwrap().len(), 1);
    }

    #[test]
    fn reabrir_base_de_version_futura_no_la_degrada() {
        // Si la base fue creada por un binario MÁS NUEVO (user_version > SCHEMA_VERSION), reabrir con
        // este binario NO debe degradar el número de versión: antes el `pragma_update` final lo bajaba
        // al nuestro, dejando un user_version mentiroso. Ahora migrate sale temprano y la deja intacta.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("turtle.db");
        {
            let a = Db::open(&path).unwrap();
            a.insert_memory(&ejemplo()).unwrap();
        }
        let futura = SCHEMA_VERSION + 1;
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", futura).unwrap();
        }
        let b = Db::open(&path).unwrap();
        assert_eq!(
            b.schema_version().unwrap(),
            futura,
            "no se degrada la versión"
        );
        assert_eq!(b.count_memories().unwrap(), 1);
    }

    fn nueva_sesion() -> turtle_core::session::NewSession {
        turtle_core::session::NewSession {
            project: "turtle".into(),
            task: Some("implementar sesiones".into()),
            agent_id: Some("dev".into()),
        }
    }

    #[test]
    fn iniciar_y_cerrar_sesion() {
        use turtle_core::session::SessionStatus;
        let db = Db::open_in_memory().unwrap();
        let id = db.start_session(&nueva_sesion()).unwrap();

        let abierta = db.get_session(&id).unwrap().expect("debe existir");
        assert_eq!(abierta.status, SessionStatus::Open);
        assert_eq!(abierta.task.as_deref(), Some("implementar sesiones"));
        assert!(abierta.ended_at.is_none());
        assert!(abierta.summary.is_none());

        let cerrada = db
            .close_session(&id, "Se implementó M4.")
            .unwrap()
            .expect("debe cerrarse");
        assert_eq!(cerrada.status, SessionStatus::Closed);
        assert_eq!(cerrada.summary.as_deref(), Some("Se implementó M4."));
        assert!(cerrada.ended_at.unwrap() >= cerrada.started_at);
    }

    #[test]
    fn cerrar_inexistente_o_repetida_es_none() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.close_session("no-existe", "x").unwrap().is_none());
        let id = db.start_session(&nueva_sesion()).unwrap();
        assert!(db.close_session(&id, "primera").unwrap().is_some());
        // Cerrar de nuevo no cambia nada y devuelve None.
        assert!(db.close_session(&id, "segunda").unwrap().is_none());
        assert_eq!(
            db.get_session(&id).unwrap().unwrap().summary.as_deref(),
            Some("primera")
        );
    }

    #[test]
    fn titulos_de_memorias_desde_marca() {
        let db = Db::open_in_memory().unwrap();
        let corte = super::now_ms();
        let id = db.insert_memory(&ejemplo()).unwrap();
        let titulos = db.memory_titles_since("turtle", corte).unwrap();
        assert_eq!(titulos, vec!["Usar Rust para el núcleo".to_string()]);
        // Otro proyecto no aparece.
        assert!(db.memory_titles_since("otro", corte).unwrap().is_empty());
        let _ = id;
    }

    #[test]
    fn registrar_agente_es_idempotente_por_proyecto_y_rotulo() {
        use turtle_core::agent::{AgentStatus, NewAgent};
        let db = Db::open_in_memory().unwrap();
        let a1 = db
            .register_agent(&NewAgent {
                project: "turtle".into(),
                label: "backend".into(),
                task: Some("tarea 1".into()),
                branch: Some("main".into()),
            })
            .unwrap();
        assert_eq!(a1.status, AgentStatus::Working);

        // Reregistrar con el mismo (proyecto, rótulo) actualiza, no duplica, y conserva el id.
        let a2 = db
            .register_agent(&NewAgent {
                project: "turtle".into(),
                label: "backend".into(),
                task: Some("tarea 2".into()),
                branch: Some("feat/x".into()),
            })
            .unwrap();
        assert_eq!(a2.id, a1.id);
        assert_eq!(a2.task.as_deref(), Some("tarea 2"));
        assert_eq!(a2.branch.as_deref(), Some("feat/x"));
        assert_eq!(db.list_agents(Some("turtle"), 10).unwrap().len(), 1);

        db.set_agent_idle("turtle", "backend").unwrap();
        let lista = db.list_agents(None, 10).unwrap();
        assert_eq!(lista[0].status, AgentStatus::Idle);
    }

    #[test]
    fn registrar_y_listar_eventos() {
        use turtle_core::event::{EventKind, NewEvent};
        let db = Db::open_in_memory().unwrap();
        db.record_event(&NewEvent {
            project: "turtle".into(),
            agent: Some("backend".into()),
            kind: EventKind::SessionStarted,
            target_id: Some("s1".into()),
            summary: Some("una tarea".into()),
        })
        .unwrap();
        db.record_event(&NewEvent {
            project: "otro".into(),
            agent: None,
            kind: EventKind::MemorySaved,
            target_id: None,
            summary: None,
        })
        .unwrap();

        // El más reciente primero; filtra por proyecto.
        let turtle = db.list_events(Some("turtle"), 10).unwrap();
        assert_eq!(turtle.len(), 1);
        assert_eq!(turtle[0].kind, EventKind::SessionStarted);
        assert_eq!(turtle[0].agent.as_deref(), Some("backend"));
        assert_eq!(db.list_events(None, 10).unwrap().len(), 2);
    }

    #[test]
    fn bandeja_dirigidos_difusion_y_entrega() {
        use turtle_core::message::NewMessage;
        let db = Db::open_in_memory().unwrap();
        // Dirigido a backend.
        db.send_message(&NewMessage {
            project: "turtle".into(),
            from_agent: Some("frontend".into()),
            to_agent: Some("backend".into()),
            body: "revisa el endpoint".into(),
        })
        .unwrap();
        // Difusión (to_agent = None): la ve cualquiera.
        db.send_message(&NewMessage {
            project: "turtle".into(),
            from_agent: None,
            to_agent: None,
            body: "deploy a las 18h".into(),
        })
        .unwrap();
        // Dirigido a otro rol: backend no lo ve.
        db.send_message(&NewMessage {
            project: "turtle".into(),
            from_agent: None,
            to_agent: Some("infra".into()),
            body: "no es para backend".into(),
        })
        .unwrap();

        let pend = db.inbox("turtle", "backend", true, 50).unwrap();
        assert_eq!(pend.len(), 2); // el dirigido + la difusión

        let entregados = db.mark_delivered("turtle", "backend").unwrap();
        assert_eq!(entregados, 2);
        assert!(db.inbox("turtle", "backend", true, 50).unwrap().is_empty());
        // Pero con only_pending=false siguen estando.
        assert_eq!(db.inbox("turtle", "backend", false, 50).unwrap().len(), 2);
    }

    #[test]
    fn candidatos_y_relaciones() {
        use turtle_core::relation::{NewRelation, RelationKind};
        let db = Db::open_in_memory().unwrap();
        let a = db.insert_memory(&ejemplo()).unwrap();
        let mut otra = ejemplo();
        otra.title = "Rust sigue siendo el núcleo".into();
        otra.content = "Confirmamos Rust como lenguaje.".into();
        let b = db.insert_memory(&otra).unwrap();

        // find_candidates por "Rust", excluyendo a, encuentra a b (parecida) y no a sí misma.
        let cands = db.find_candidates("Rust", Some("turtle"), &a, 10).unwrap();
        assert!(cands.iter().any(|c| c.id == b));
        assert!(!cands.iter().any(|c| c.id == a));

        // Registrar que b reemplaza a a; aparece para ambas.
        db.add_relation(&NewRelation {
            from_id: b.clone(),
            to_id: a.clone(),
            kind: RelationKind::Replaces,
            note: Some("más nueva".into()),
        })
        .unwrap();
        let rels = db.list_relations(&a).unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].kind, RelationKind::Replaces);
        assert_eq!(db.list_relations(&b).unwrap().len(), 1);
    }

    #[test]
    fn skills_upsert_busqueda_y_carga() {
        use turtle_core::skill::{NewSkill, SkillKind};
        let db = Db::open_in_memory().unwrap();
        let nueva = |name: &str, kind, cuando: &str, content: &str| NewSkill {
            project: String::new(),
            name: name.to_string(),
            kind,
            when_to_use: Some(cuando.to_string()),
            content: content.to_string(),
            tags: Some("git".into()),
            source: Some("skills/x/SKILL.md".into()),
        };
        let id1 = db
            .upsert_skill(&nueva(
                "ponytail",
                SkillKind::Tool,
                "para ramas de git",
                "Contenido inicial.",
            ))
            .unwrap();
        // Reingesta idempotente: mismo (proyecto, nombre, tipo) → mismo id, sin duplicar.
        let id1b = db
            .upsert_skill(&nueva(
                "ponytail",
                SkillKind::Tool,
                "para ramas de git",
                "Contenido nuevo.",
            ))
            .unwrap();
        assert_eq!(id1, id1b);
        assert_eq!(db.count_skills().unwrap(), 1);

        db.upsert_skill(&nueva(
            "revisor",
            SkillKind::Agent,
            "revisa PRs",
            "Subagente.",
        ))
        .unwrap();
        // Búsqueda FTS5 barata.
        let hits = db.search_skills("ponytail", None, 10).unwrap();
        assert!(hits.iter().any(|h| h.id == id1 && h.name == "ponytail"));
        // Carga completa: refleja la última actualización.
        let full = db.get_skill(&id1).unwrap().unwrap();
        assert_eq!(full.content, "Contenido nuevo.");
        assert_eq!(full.kind, SkillKind::Tool);
    }

    #[test]
    fn activacion_de_skills_de_comportamiento() {
        use turtle_core::skill::{Intensidad, NewSkill, SkillKind};
        let db = Db::open_in_memory().unwrap();
        let comp = db
            .upsert_skill(&NewSkill {
                project: "demo".into(),
                name: "ponytail".into(),
                kind: SkillKind::Behavior,
                when_to_use: Some("anti sobre-ingeniería".into()),
                content: "contenido".into(),
                tags: None,
                source: None,
            })
            .unwrap();
        // Por defecto off: no figura entre las activas.
        assert!(db.active_behavior_skills("demo", 10).unwrap().is_empty());
        assert!(db.set_skill_intensity(&comp, Intensidad::Full).unwrap());
        let activas = db.active_behavior_skills("demo", 10).unwrap();
        assert_eq!(activas.len(), 1);
        assert_eq!(activas[0].intensity, Intensidad::Full);
    }

    #[test]
    fn diagnostico_basico() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), super::SCHEMA_VERSION);
        assert_eq!(db.integrity_check().unwrap(), "ok");
        db.insert_memory(&ejemplo()).unwrap();
        // FTS sincronizado: misma cantidad en la tabla base y en el índice.
        assert_eq!(
            db.count_memories().unwrap(),
            db.count_table("memories_fts").unwrap()
        );
        assert_eq!(db.duplicate_memory_groups().unwrap(), 0);
        // Otra memoria con el mismo (proyecto, título) crea un grupo duplicado.
        db.insert_memory(&ejemplo()).unwrap();
        assert_eq!(db.duplicate_memory_groups().unwrap(), 1);
    }

    #[test]
    fn topic_key_hace_upsert_y_no_duplica() {
        let db = Db::open_in_memory().unwrap();
        let con_tema = |titulo: &str, content: &str| NewMemory {
            topic_key: Some("infra/migraciones".into()),
            ..NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Decision,
                titulo.into(),
                content.into(),
            )
        };
        let id1 = db.insert_memory(&con_tema("v1", "esquema v1")).unwrap();
        let id2 = db
            .insert_memory(&con_tema("v2", "esquema v2 evolucionado"))
            .unwrap();
        // Mismo tema (project+scope+topic_key) → UPSERT: mismo id, una sola memoria.
        assert_eq!(id1, id2);
        assert_eq!(db.count_memories().unwrap(), 1);
        let m = db.get_memory(&id1).unwrap().unwrap();
        assert_eq!(m.title, "v2");
        assert_eq!(m.content, "esquema v2 evolucionado");
        assert_eq!(m.topic_key.as_deref(), Some("infra/migraciones"));

        // Otro topic_key NO pisa: crea una memoria distinta.
        let otro = NewMemory {
            topic_key: Some("infra/indices".into()),
            ..NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Decision,
                "índices".into(),
                "x".into(),
            )
        };
        let id3 = db.insert_memory(&otro).unwrap();
        assert_ne!(id3, id1);
        assert_eq!(db.count_memories().unwrap(), 2);

        // Sin topic_key: inserto normal, no choca aunque tenga el mismo título.
        db.insert_memory(&NewMemory::nueva(
            "turtle".into(),
            MemoryKind::Note,
            "v2".into(),
            "sin tema".into(),
        ))
        .unwrap();
        assert_eq!(db.count_memories().unwrap(), 3);
    }

    #[test]
    fn upsert_de_tema_archiva_la_version_anterior() {
        let db = Db::open_in_memory().unwrap();
        let con_tema = |titulo: &str, content: &str| NewMemory {
            topic_key: Some("api/contrato".into()),
            ..NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Decision,
                titulo.into(),
                content.into(),
            )
        };
        // Primer alta: no hay versión anterior que archivar.
        let id = db.insert_memory(&con_tema("v1", "contrato v1")).unwrap();
        assert!(
            db.memory_versions(&id).unwrap().is_empty(),
            "sin historial al crear"
        );

        // Segunda actualización: archiva v1.
        let id2 = db.insert_memory(&con_tema("v2", "contrato v2")).unwrap();
        assert_eq!(id, id2);
        let h1 = db.memory_versions(&id).unwrap();
        assert_eq!(h1.len(), 1, "una versión archivada");
        assert_eq!(h1[0].title, "v1");
        assert_eq!(h1[0].content, "contrato v1");
        assert_eq!(h1[0].memory_id, id);
        assert!(h1[0].valid_to >= h1[0].valid_from);

        // Tercera: archiva v2; la viva es v3; historial de 2 (más reciente primero).
        db.insert_memory(&con_tema("v3", "contrato v3")).unwrap();
        let h2 = db.memory_versions(&id).unwrap();
        assert_eq!(h2.len(), 2);
        assert_eq!(h2[0].title, "v2", "más reciente primero");
        assert_eq!(h2[1].title, "v1");
        // La memoria viva es v3 y sigue siendo una sola (el historial no infla el conteo).
        assert_eq!(db.get_memory(&id).unwrap().unwrap().title, "v3");
        assert_eq!(db.count_memories().unwrap(), 1);
    }

    #[test]
    fn alta_sin_tema_no_genera_historial() {
        let db = Db::open_in_memory().unwrap();
        let id = db
            .insert_memory(&NewMemory::nueva(
                "turtle".into(),
                MemoryKind::Note,
                "suelta".into(),
                "x".into(),
            ))
            .unwrap();
        assert!(db.memory_versions(&id).unwrap().is_empty());
    }

    #[test]
    fn topic_key_separa_por_proyecto_y_scope() {
        let db = Db::open_in_memory().unwrap();
        let mem = |proj: &str, scope: Scope| NewMemory {
            scope,
            topic_key: Some("area/tema".into()),
            ..NewMemory::nueva(proj.into(), MemoryKind::Note, "t".into(), "c".into())
        };
        // Mismo topic_key pero proyectos distintos: NO se pisan.
        db.insert_memory(&mem("a", Scope::Project)).unwrap();
        db.insert_memory(&mem("b", Scope::Project)).unwrap();
        // Mismo proyecto pero scope distinto: tampoco se pisan.
        db.insert_memory(&mem("a", Scope::Personal)).unwrap();
        assert_eq!(db.count_memories().unwrap(), 3);
    }

    #[test]
    fn memorias_personales_se_ven_en_todos_los_proyectos() {
        let db = Db::open_in_memory().unwrap();
        // Personal en proyecto "uno".
        db.insert_memory(&NewMemory {
            scope: Scope::Personal,
            ..NewMemory::nueva(
                "uno".into(),
                MemoryKind::Convention,
                "Preferencia de estilo".into(),
                "Siempre español latino neutro.".into(),
            )
        })
        .unwrap();
        // De proyecto en "dos".
        db.insert_memory(&NewMemory::nueva(
            "dos".into(),
            MemoryKind::Note,
            "Algo de dos".into(),
            "Solo de español dos.".into(),
        ))
        .unwrap();

        // Filtrando por "dos": aparece la personal de "uno" + la de proyecto "dos".
        let r = db.search_index("español", Some("dos"), 50).unwrap();
        assert_eq!(r.len(), 2, "la personal cruza el filtro de proyecto");
        assert!(r.iter().any(|x| x.title == "Preferencia de estilo"));

        // Filtrando por "uno": NO aparece la de proyecto "dos" (sigue acotado).
        let r2 = db.search_index("español", Some("uno"), 50).unwrap();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].title, "Preferencia de estilo");

        // recent_index respeta lo mismo.
        let rec = db.recent_index(Some("tres"), 50).unwrap();
        assert!(rec.iter().any(|x| x.title == "Preferencia de estilo"));
        assert!(!rec.iter().any(|x| x.title == "Algo de dos"));
    }

    #[test]
    fn escalonar_a_frio_marca_needs_review_y_mark_reviewed_lo_limpia() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_memory(&ejemplo()).unwrap();
        // Primero a tibio (corte futuro), luego a frío: cold cutoff futuro fuerza el pase.
        let now = super::now_ms();
        db.escalonar_tiers("turtle", now + DIA_FUTURO, now + DIA_FUTURO)
            .unwrap();
        let r = db.search_index("Rust", Some("turtle"), 10).unwrap();
        assert_eq!(r.len(), 1);
        assert!(r[0].needs_review, "al pasar a frío se marca needs_review");
        // Aparece en la lista de por revisar.
        let lista = db.needs_review_index("turtle", 10).unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, id);

        // mark_reviewed la vuelve a vigente.
        assert!(db.mark_reviewed(&id).unwrap());
        assert!(db.needs_review_index("turtle", 10).unwrap().is_empty());
        let r2 = db.search_index("Rust", Some("turtle"), 10).unwrap();
        assert!(!r2[0].needs_review);
        // mark_reviewed de un id inexistente devuelve false.
        assert!(!db.mark_reviewed("no-existe").unwrap());
    }

    #[test]
    fn last_prompt_se_registra_y_se_consume_una_vez() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.take_last_prompt("turtle").unwrap().is_none());
        db.record_last_prompt("turtle", Some("s1"), "primer prompt")
            .unwrap();
        db.record_last_prompt("turtle", Some("s1"), "segundo prompt")
            .unwrap();
        // Toma el más reciente y lo consume; la segunda vez no hay más sin consumir.
        assert_eq!(
            db.take_last_prompt("turtle").unwrap().as_deref(),
            Some("segundo prompt")
        );
        // Queda "primer prompt" sin consumir.
        assert_eq!(
            db.take_last_prompt("turtle").unwrap().as_deref(),
            Some("primer prompt")
        );
        assert!(db.take_last_prompt("turtle").unwrap().is_none());
        // Otro proyecto no comparte prompts.
        assert!(db.take_last_prompt("otro").unwrap().is_none());
    }

    /// Días en ms hacia adelante: vuelve el corte de antigüedad futuro, así toda memoria cuenta
    /// como "no accedida desde antes del corte" y el escalonamiento la mueve.
    const DIA_FUTURO: i64 = 24 * 60 * 60 * 1000;

    #[test]
    fn exportar_e_importar_memorias() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_memory(&ejemplo()).unwrap();
        let exportadas = db.all_memories(Some("turtle")).unwrap();
        assert_eq!(exportadas.len(), 1);

        // Importar en otra base reproduce la memoria con el mismo id.
        let db2 = Db::open_in_memory().unwrap();
        assert!(db2.import_memory(&exportadas[0]).unwrap()); // nueva
        let traida = db2.get_memory(&id).unwrap().unwrap();
        assert_eq!(traida.title, exportadas[0].title);

        // Reimportar la actualiza (no nueva), no duplica y deja el FTS en sincronía.
        assert!(!db2.import_memory(&exportadas[0]).unwrap());
        assert_eq!(db2.count_memories().unwrap(), 1);
        assert_eq!(
            db2.count_memories().unwrap(),
            db2.count_table("memories_fts").unwrap()
        );
    }
}
