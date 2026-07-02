//! Export/import de memorias en formato JSON abierto (RF-SYN): respaldo y traslado entre
//! máquinas, con la base local como fuente de verdad. Sin nube; la portabilidad es un archivo.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use turtle_core::memory::{Importance, Memory, MemoryKind, ReviewState, Scope, Tier};
use turtle_service::MemoryService;

/// Versión del formato de exportación. Se valida al importar.
const FORMATO: u32 = 1;

/// Sobre del archivo de exportación.
#[derive(Serialize, Deserialize)]
struct Sobre {
    turtle_export: u32,
    // Metadato informativo: el import lo ignora. `default` para no exigirlo, así un JSON hecho a
    // mano o por otra herramienta (la portabilidad "es un archivo") importa aunque no lo traiga.
    #[serde(default)]
    exportado_en: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    proyecto: Option<String>,
    memorias: Vec<MemoriaJson>,
}

/// Una memoria en el formato abierto (claves en español, neutrales al esquema interno).
#[derive(Serialize, Deserialize)]
struct MemoriaJson {
    id: String,
    proyecto: String,
    tipo: String,
    titulo: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    que: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    porque: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    donde: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    aprendido: Option<String>,
    contenido: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    resumen: Option<String>,
    importancia: String,
    nivel: String,
    // Campos de paridad funcional. `default` para que un export viejo (sin estas claves) siga
    // importando: scope→project, review_state→active, topic_key/prompt→ausentes.
    #[serde(default = "scope_default")]
    alcance: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tema: Option<String>,
    #[serde(default = "review_default")]
    estado_revision: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    prompt: Option<String>,
    creado_en: i64,
    actualizado_en: i64,
    accedido_en: i64,
}

fn scope_default() -> String {
    Scope::Project.as_str().to_string()
}

fn review_default() -> String {
    ReviewState::Active.as_str().to_string()
}

impl From<&Memory> for MemoriaJson {
    fn from(m: &Memory) -> Self {
        MemoriaJson {
            id: m.id.clone(),
            proyecto: m.project.clone(),
            tipo: m.kind.as_str().to_string(),
            titulo: m.title.clone(),
            que: m.what.clone(),
            porque: m.why.clone(),
            donde: m.where_.clone(),
            aprendido: m.learned.clone(),
            contenido: m.content.clone(),
            resumen: m.summary.clone(),
            importancia: m.importance.as_str().to_string(),
            nivel: m.tier.as_str().to_string(),
            alcance: m.scope.as_str().to_string(),
            tema: m.topic_key.clone(),
            estado_revision: m.review_state.as_str().to_string(),
            prompt: m.prompt.clone(),
            creado_en: m.created_at,
            actualizado_en: m.updated_at,
            accedido_en: m.accessed_at,
        }
    }
}

impl MemoriaJson {
    fn a_memoria(self) -> Memory {
        Memory {
            id: self.id,
            project: self.proyecto,
            kind: MemoryKind::parse(&self.tipo).unwrap_or(MemoryKind::Note),
            title: self.titulo,
            what: self.que,
            why: self.porque,
            where_: self.donde,
            learned: self.aprendido,
            content: self.contenido,
            summary: self.resumen,
            importance: Importance::parse(&self.importancia).unwrap_or(Importance::Normal),
            tier: Tier::parse(&self.nivel).unwrap_or(Tier::Hot),
            scope: Scope::parse(&self.alcance).unwrap_or(Scope::Project),
            topic_key: self.tema,
            review_state: ReviewState::parse(&self.estado_revision).unwrap_or(ReviewState::Active),
            prompt: self.prompt,
            created_at: self.creado_en,
            updated_at: self.actualizado_en,
            accessed_at: self.accedido_en,
        }
    }
}

/// Exporta las memorias (de un proyecto o de todos) a una cadena JSON.
pub fn exportar(servicio: &MemoryService, proyecto: Option<&str>) -> Result<String, String> {
    let mems = servicio
        .export_memories(proyecto)
        .map_err(|e| e.to_string())?;
    let sobre = Sobre {
        turtle_export: FORMATO,
        exportado_en: now_ms(),
        proyecto: proyecto.map(str::to_string),
        memorias: mems.iter().map(MemoriaJson::from).collect(),
    };
    serde_json::to_string_pretty(&sobre).map_err(|e| e.to_string())
}

/// Importa memorias desde una cadena JSON. Devuelve `(nuevas, actualizadas)`.
pub fn importar(servicio: &MemoryService, json: &str) -> Result<(usize, usize), String> {
    let sobre: Sobre = serde_json::from_str(json).map_err(|e| format!("JSON inválido: {e}"))?;
    if sobre.turtle_export != FORMATO {
        return Err(format!(
            "formato de exportación no soportado: {} (se esperaba {FORMATO}).",
            sobre.turtle_export
        ));
    }
    let mems: Vec<Memory> = sobre
        .memorias
        .into_iter()
        .map(MemoriaJson::a_memoria)
        .collect();
    servicio.import_memories(&mems).map_err(|e| e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Exporta cada memoria como un archivo `<id>.json` en `dir` (RF-SYN-02): un archivo por memoria,
/// versionable en git y **sin conflictos de fusión** (cada memoria vive en su propio archivo,
/// RF-SYN-03). Devuelve cuántos fragmentos se escribieron.
pub fn exportar_fragmentos(
    servicio: &MemoryService,
    proyecto: Option<&str>,
    dir: &Path,
) -> Result<usize, String> {
    let mems = servicio
        .export_memories(proyecto)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for m in &mems {
        let texto =
            serde_json::to_string_pretty(&MemoriaJson::from(m)).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(nombre_fragmento(&m.id)), texto).map_err(|e| e.to_string())?;
    }
    Ok(mems.len())
}

/// Nombre de archivo seguro para un fragmento. Los ids propios son ULID (alfanuméricos), pero el
/// import acepta ids arbitrarios de un JSON externo: un id con separadores de ruta (`..\x`) haría
/// que el export escriba FUERA del directorio de sync (traversal). Todo carácter no alfanumérico
/// se reemplaza por `_`; el nombre queda siempre dentro de `dir`.
fn nombre_fragmento(id: &str) -> String {
    let seguro: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{seguro}.json")
}

/// Importa todos los fragmentos `*.json` de un directorio (RF-SYN-02). Devuelve
/// `(nuevas, actualizadas)`. La base local sigue siendo la fuente de verdad.
pub fn importar_fragmentos(servicio: &MemoryService, dir: &Path) -> Result<(usize, usize), String> {
    let mut mems = Vec::new();
    let entradas = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for e in entradas.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let texto = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
        let j: MemoriaJson =
            serde_json::from_str(&texto).map_err(|e| format!("{}: {e}", p.display()))?;
        mems.push(j.a_memoria());
    }
    servicio.import_memories(&mems).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use turtle_data::Db;

    fn servicio() -> MemoryService {
        MemoryService::new(Db::open_in_memory().unwrap())
    }

    #[test]
    fn importa_aunque_falte_exportado_en() {
        // Un JSON hecho a mano (sin el metadato `exportado_en`) debe importar igual.
        let json = r#"{"turtle_export":1,"memorias":[
            {"id":"01000000000000000000000001","proyecto":"demo","tipo":"note",
             "titulo":"Hecha a mano","contenido":"rust sqlite fts5","importancia":"normal",
             "nivel":"hot","creado_en":1,"actualizado_en":1,"accedido_en":1}]}"#;
        let (nuevas, actualizadas) = importar(&servicio(), json).unwrap();
        assert_eq!((nuevas, actualizadas), (1, 0));
    }

    #[test]
    fn rechaza_formato_desconocido() {
        let json = r#"{"turtle_export":99,"exportado_en":0,"memorias":[]}"#;
        assert!(importar(&servicio(), json).is_err());
    }

    #[test]
    fn fragmento_con_id_hostil_no_escapa_del_directorio() {
        // Un id importado de un JSON externo puede traer separadores de ruta: el nombre del
        // fragmento debe quedar saneado (sin `\`, `/` ni `..`), siempre dentro del directorio.
        assert_eq!(
            nombre_fragmento(r"..\..\evil"),
            "______evil.json".to_string()
        );
        assert_eq!(nombre_fragmento("../otro/lado"), "___otro_lado.json");
        // Un ULID real queda intacto.
        assert_eq!(
            nombre_fragmento("01KWFXD3TBQV73XRS6XZYRK6CB"),
            "01KWFXD3TBQV73XRS6XZYRK6CB.json"
        );
    }

    #[test]
    fn exportar_fragmentos_sanea_ids_importados() {
        let s = servicio();
        // Importa una memoria con id hostil (como vendría de un JSON externo)...
        let json = r#"{"turtle_export":1,"memorias":[
            {"id":"..\\evil","proyecto":"demo","tipo":"note",
             "titulo":"Hostil","contenido":"x","importancia":"normal",
             "nivel":"hot","creado_en":1,"actualizado_en":1,"accedido_en":1}]}"#;
        importar(&s, json).unwrap();
        // ...y exporta los fragmentos: el archivo debe quedar DENTRO del directorio destino.
        let dir = std::env::temp_dir().join(format!("turtle_sync_qa_{}", std::process::id()));
        let escritos = exportar_fragmentos(&s, Some("demo"), &dir).unwrap();
        assert_eq!(escritos, 1);
        assert!(dir.join("___evil.json").exists());
        assert!(!dir.parent().unwrap().join("evil.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trip_exporta_e_importa() {
        let s = servicio();
        s.save(&turtle_core::memory::NewMemory::nueva(
            "demo".into(),
            turtle_core::memory::MemoryKind::Note,
            "Round trip".into(),
            "contenido de prueba".into(),
        ))
        .unwrap();
        let json = exportar(&s, Some("demo")).unwrap();
        let (nuevas, actualizadas) = importar(&servicio(), &json).unwrap();
        assert_eq!((nuevas, actualizadas), (1, 0));
    }
}
