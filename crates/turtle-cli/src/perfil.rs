//! `turtle perfil` — perfiles de modelo por fase para el flujo SDD.
//!
//! Hermano de `turtle modelos`: en vez de elegir el modelo persona por persona, aplica un **perfil
//! nombrado** (`cheap`/`balanced`/`premium`) que asigna un alias de modelo a cada *tier* de fase y
//! resuelve el modelo de cada persona con la regla "el más fuerte gana" (ver
//! `turtle_service::perfil_resolver`). La receta (`#:perfil`/`#:fase`) se persiste en
//! `~/.turtle/models.conf` junto a las líneas resueltas `slug = modelo`, y se aplica reescribiendo
//! los sub-agentes con la misma maquinaria de `turtle modelos` (`setup::instalar_subagentes`).
//!
//! Provider-agnóstico: no rutea APIs ni usa gateway; solo escribe hints `model:` que Claude Code
//! respeta para SUS sub-agentes. En CLIs single-agent (Codex/OpenCode) es informativo.

use std::collections::BTreeMap;

use turtle_service::Perfil;

use crate::modelos::{self, Config};
use crate::AccionPerfil;

/// Despacha `turtle perfil [acción]`. Sin acción muestra el perfil activo y el mapa efectivo.
pub(crate) fn ejecutar(accion: Option<AccionPerfil>) -> Result<(), String> {
    match accion {
        None | Some(AccionPerfil::Mostrar) => mostrar(),
        Some(AccionPerfil::Cheap) => aplicar("cheap"),
        Some(AccionPerfil::Balanced) => aplicar("balanced"),
        Some(AccionPerfil::Premium) => aplicar("premium"),
        Some(AccionPerfil::Fase { fase, modelo }) => aplicar_fase(&fase, &modelo),
        Some(AccionPerfil::Reset) => reset(),
    }
}

// ─── Núcleo puro (sin IO): testeable sin tocar el disco ───

/// Aplica un perfil nombrado sobre una config (PURO): fija `#:perfil`, **limpia** los overrides de
/// fase (aplicar un perfil parte de cero), re-resuelve y reescribe las líneas de las personas del
/// flujo. Las personas fuera del flujo (p. ej. `botticelli`) se preservan. Determinista e idempotente.
fn aplicar_a_config(mut cfg: Config, perfil: &Perfil) -> Config {
    cfg.perfil = Some(perfil.nombre.to_string());
    cfg.fases.clear();
    fundir_resueltos(&mut cfg, perfil);
    cfg
}

/// Registra un override por fase sobre una config (PURO). Valida fase y modelo. Si no hay perfil
/// activo (o el activo es inválido), asume `balanced` como base. Re-resuelve el flujo.
fn fase_en_config(mut cfg: Config, fase: &str, modelo: &str) -> Result<Config, String> {
    if !turtle_service::fase_existe(fase) {
        let nombres: Vec<&str> = turtle_service::FASES.iter().map(|f| f.nombre).collect();
        return Err(format!(
            "fase desconocida: '{fase}'. Opciones: {}.",
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
    let nombre = cfg.perfil.as_deref().unwrap_or("balanced");
    let perfil = turtle_service::perfil_por_nombre(nombre)
        .or_else(|| turtle_service::perfil_por_nombre("balanced"))
        .ok_or("no se pudo determinar un perfil base.")?;
    cfg.perfil = Some(perfil.nombre.to_string());
    cfg.fases.insert(fase.to_string(), modelo.to_string());
    fundir_resueltos(&mut cfg, perfil);
    Ok(cfg)
}

/// Re-resuelve el mapa persona→modelo del perfil y lo funde sobre los overrides: el perfil es
/// autoritativo sobre las personas del flujo (clobber predecible); las de fuera no se tocan.
fn fundir_resueltos(cfg: &mut Config, perfil: &Perfil) {
    for (slug, modelo) in turtle_service::perfil_resolver(perfil, &cfg.fases) {
        cfg.overrides.insert(slug, modelo);
    }
}

/// Quita la receta de perfil y los overrides de las personas del flujo (vuelven a su default del
/// bundle). Preserva los overrides de personas fuera del flujo.
fn reset_en_config(mut cfg: Config) -> Config {
    cfg.perfil = None;
    cfg.fases.clear();
    for slug in turtle_service::slugs_flujo() {
        cfg.overrides.remove(&slug);
    }
    cfg
}

// ─── Default de instalación: el reparto pedido aplicado sin correr comandos ───

/// Perfil que se aplica solo al instalar (paridad con el reparto por defecto): arquitecto SDD y
/// todo lo pensativo en opus 4.8, coding en sonnet 5, exploración/lectura en haiku.
pub(crate) const PERFIL_DEFAULT: &str = "balanced";

/// `true` si corresponde aplicar el perfil por defecto: solo cuando el usuario todavía no eligió
/// nada (ni perfil declarado ni overrides por persona). PURO: decide sobre una config, sin IO.
fn corresponde_default(cfg: &Config) -> bool {
    cfg.perfil.is_none() && cfg.overrides.is_empty()
}

/// Aplica el perfil por defecto SOLO si el usuario no configuró nada, y persiste la config.
/// No destructivo e idempotente: respeta cualquier elección previa (perfil u overrides). Devuelve
/// el nombre del perfil si lo aplicó, o `None` si ya había configuración. Lo usa `turtle setup`.
pub(crate) fn aplicar_default_si_vacio() -> Result<Option<&'static str>, String> {
    let cfg = modelos::leer_config();
    if !corresponde_default(&cfg) {
        return Ok(None);
    }
    let perfil = turtle_service::perfil_por_nombre(PERFIL_DEFAULT)
        .ok_or("no se encontró el perfil por defecto.")?;
    modelos::escribir_config(&aplicar_a_config(cfg, perfil))?;
    Ok(Some(perfil.nombre))
}

// ─── Cáscara con IO: lee/escribe models.conf y reescribe los sub-agentes ───

fn aplicar(nombre: &str) -> Result<(), String> {
    let perfil = turtle_service::perfil_por_nombre(nombre)
        .ok_or_else(|| format!("perfil desconocido: '{nombre}'."))?;
    let cfg = aplicar_a_config(modelos::leer_config(), perfil);
    modelos::escribir_config(&cfg)?;
    let n = crate::setup::instalar_subagentes(&cfg.overrides)?;
    println!("Perfil aplicado: {} ({}).", perfil.nombre, perfil.nota);
    imprimir_mapa(&cfg);
    println!("Sub-agentes reescritos: {n} (en ~/.claude/agents/).");
    aviso_provider();
    Ok(())
}

fn aplicar_fase(fase_arg: &str, modelo: &str) -> Result<(), String> {
    let cfg = fase_en_config(modelos::leer_config(), fase_arg, modelo)?;
    modelos::escribir_config(&cfg)?;
    let n = crate::setup::instalar_subagentes(&cfg.overrides)?;
    let perfil = cfg
        .perfil
        .as_deref()
        .and_then(turtle_service::perfil_por_nombre)
        .expect("fase_en_config deja un perfil válido");
    println!(
        "Override de fase: {fase_arg} = {modelo}  (perfil base: {}).",
        perfil.nombre
    );
    if turtle_service::FASES
        .iter()
        .any(|f| f.nombre == fase_arg && f.duenos.is_empty())
    {
        println!("(La fase '{fase_arg}' es advisory: se registró la directiva, no reescribe ninguna persona.)");
    }
    imprimir_mapa(&cfg);
    println!("Sub-agentes reescritos: {n} (en ~/.claude/agents/).");
    aviso_provider();
    Ok(())
}

fn reset() -> Result<(), String> {
    let cfg = reset_en_config(modelos::leer_config());
    modelos::escribir_config(&cfg)?;
    let n = crate::setup::instalar_subagentes(&cfg.overrides)?;
    println!(
        "Perfil borrado. Las personas del flujo SDD vuelven a su modelo por defecto. \
         Sub-agentes reescritos: {n}."
    );
    Ok(())
}

fn mostrar() -> Result<(), String> {
    let cfg = modelos::leer_config();
    match cfg
        .perfil
        .as_deref()
        .and_then(turtle_service::perfil_por_nombre)
    {
        None => {
            println!("Sin perfil activo. Aplicá uno con: turtle perfil <cheap|balanced|premium>.");
            for p in turtle_service::PERFILES {
                println!("  {:<9} {}", p.nombre, p.nota);
            }
        }
        Some(perfil) => {
            println!("Perfil activo: {} ({}).", perfil.nombre, perfil.nota);
            // Receta declarada pero sin overrides en el flujo (p. ej. tras `turtle modelos reset`):
            // el mapa de abajo es honesto (muestra los defaults), pero conviene avisar que el perfil
            // no está reflejado en las personas y cómo reaplicarlo.
            let hay_overrides_flujo = turtle_service::slugs_flujo()
                .iter()
                .any(|slug| cfg.overrides.contains_key(slug));
            if !hay_overrides_flujo {
                println!(
                    "Nota: perfil declarado sin overrides aplicados; reaplicá con `turtle perfil {}`.",
                    perfil.nombre
                );
            }
            if !cfg.fases.is_empty() {
                println!("Overrides por fase:");
                for (f, m) in &cfg.fases {
                    println!("  {f} = {m}");
                }
            }
            imprimir_mapa(&cfg);
            aviso_provider();
        }
    }
    Ok(())
}

/// Imprime el mapa persona→modelo REALMENTE aplicado a las personas del flujo SDD: para cada una,
/// el override de `models.conf` si existe, o el modelo por defecto del bundle (que es lo que
/// `instalar_subagentes` escribiría). Refleja el estado autoritativo, así un `turtle modelos set`
/// manual sobre un perfil se ve tal cual (no se re-resuelve y oculta).
fn imprimir_mapa(cfg: &Config) {
    let info = info_por_slug();
    println!("Mapa efectivo (persona → modelo en Claude Code):");
    for slug in turtle_service::slugs_flujo() {
        let (rol, default) = info
            .get(&slug)
            .map(|(r, d)| (r.as_str(), d.as_str()))
            .unwrap_or(("", "inherit"));
        let modelo = cfg
            .overrides
            .get(&slug)
            .map(String::as_str)
            .unwrap_or(default);
        println!("  {slug:<12} {rol:<14} {modelo}");
    }
    println!("(Personas fuera del flujo SDD, p. ej. botticelli/seo, no se tocan.)");
}

/// Aviso honesto: el perfil aplica a los sub-agentes de Claude Code; en CLIs single-agent es solo
/// informativo (no hay `model:` por rol que reescribir).
fn aviso_provider() {
    println!(
        "Nota: aplica a los sub-agentes de Claude Code; en CLIs single-agent (Codex/OpenCode) \
         el perfil es informativo."
    );
}

/// Mapa slug → (rol legible, modelo por defecto del bundle), tomado de las personas (para la salida).
fn info_por_slug() -> BTreeMap<String, (String, String)> {
    turtle_service::personas()
        .into_iter()
        .map(|p| (p.slug, (p.rol, p.modelo_default)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(s, m)| (s.to_string(), m.to_string()))
            .collect()
    }

    #[test]
    fn aplicar_balanced_resuelve_y_registra_la_receta() {
        let cfg = aplicar_a_config(Config::default(), perfil("balanced"));
        assert_eq!(cfg.perfil.as_deref(), Some("balanced"));
        assert!(cfg.fases.is_empty());
        assert_eq!(
            cfg.overrides.get("donatello").map(String::as_str),
            Some("opus")
        );
        assert_eq!(
            cfg.overrides.get("brunelleschi").map(String::as_str),
            Some("sonnet")
        );
        // botticelli no participa del flujo: no aparece.
        assert!(!cfg.overrides.contains_key("botticelli"));
    }

    #[test]
    fn aplicar_un_perfil_preserva_personas_fuera_del_flujo() {
        let base = Config {
            overrides: map(&[("botticelli", "haiku")]),
            ..Config::default()
        };
        let cfg = aplicar_a_config(base, perfil("premium"));
        // botticelli intacto; el flujo a opus.
        assert_eq!(
            cfg.overrides.get("botticelli").map(String::as_str),
            Some("haiku")
        );
        assert_eq!(
            cfg.overrides.get("galileo").map(String::as_str),
            Some("opus")
        );
    }

    #[test]
    fn aplicar_es_idempotente_en_bytes() {
        let perfil = perfil("balanced");
        let cfg1 = aplicar_a_config(Config::default(), perfil);
        let bytes1 = modelos::serializar_config(&cfg1);
        let cfg2 = aplicar_a_config(modelos::parsear_config(&bytes1), perfil);
        let bytes2 = modelos::serializar_config(&cfg2);
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn fase_design_sonnet_baja_a_donatello() {
        let cfg = fase_en_config(Config::default(), "design", "sonnet").unwrap();
        assert_eq!(cfg.perfil.as_deref(), Some("balanced")); // sin perfil previo → balanced
        assert_eq!(cfg.fases.get("design").map(String::as_str), Some("sonnet"));
        assert_eq!(
            cfg.overrides.get("donatello").map(String::as_str),
            Some("sonnet")
        );
    }

    #[test]
    fn fase_dominada_no_cambia_a_la_persona() {
        // Partimos de balanced; bajar spec (mid) no afecta a alberti (que tiene propose strong).
        let base = aplicar_a_config(Config::default(), perfil("balanced"));
        let cfg = fase_en_config(base, "spec", "haiku").unwrap();
        assert_eq!(
            cfg.overrides.get("alberti").map(String::as_str),
            Some("opus")
        );
    }

    #[test]
    fn fase_rechaza_fase_y_modelo_invalidos() {
        let e = fase_en_config(Config::default(), "noexiste", "opus").unwrap_err();
        assert!(e.contains("fase desconocida"), "{e}");
        let e = fase_en_config(Config::default(), "design", "gpt-5").unwrap_err();
        assert!(e.contains("modelo desconocido"), "{e}");
    }

    #[test]
    fn reset_borra_receta_y_overrides_del_flujo_pero_no_botticelli() {
        let mut cfg = aplicar_a_config(Config::default(), perfil("premium"));
        cfg.overrides
            .insert("botticelli".to_string(), "haiku".to_string());
        let limpio = reset_en_config(cfg);
        assert!(limpio.perfil.is_none());
        assert!(limpio.fases.is_empty());
        assert!(!limpio.overrides.contains_key("galileo"));
        assert!(!limpio.overrides.contains_key("donatello"));
        // El override de una persona fuera del flujo sobrevive al reset de perfil.
        assert_eq!(
            limpio.overrides.get("botticelli").map(String::as_str),
            Some("haiku")
        );
    }

    #[test]
    fn cambiar_de_perfil_reescribe_los_modelos_de_todo_el_flujo() {
        // Más allá de la idempotencia en bytes: aplicar `premium` y luego `cheap` debe reescribir
        // EFECTIVAMENTE el modelo de TODOS los slugs del flujo (no solo dejar el archivo igual).
        let tras_premium = aplicar_a_config(Config::default(), perfil("premium"));
        let flujo = turtle_service::slugs_flujo();
        // premium deja a cada persona del flujo en opus.
        for slug in &flujo {
            assert_eq!(
                tras_premium.overrides.get(slug).map(String::as_str),
                Some("opus"),
                "premium debería poner {slug} en opus"
            );
        }
        // Cambiar a cheap baja a TODAS a haiku (clobber del perfil sobre cada slug del flujo).
        let tras_cheap = aplicar_a_config(tras_premium, perfil("cheap"));
        assert_eq!(tras_cheap.perfil.as_deref(), Some("cheap"));
        for slug in &flujo {
            assert_eq!(
                tras_cheap.overrides.get(slug).map(String::as_str),
                Some("haiku"),
                "cheap debería reescribir {slug} a haiku (no quedar en el opus de premium)"
            );
        }
    }

    #[test]
    fn default_solo_aplica_con_config_vacia() {
        // Config virgen (ni perfil ni overrides): corresponde aplicar el default.
        assert!(corresponde_default(&Config::default()));
        // Con un perfil ya declarado: NO se pisa.
        let con_perfil = Config {
            perfil: Some("premium".to_string()),
            ..Config::default()
        };
        assert!(!corresponde_default(&con_perfil));
        // Con overrides por persona elegidos a mano (sin perfil): tampoco se pisa.
        let con_overrides = Config {
            overrides: map(&[("brunelleschi", "opus")]),
            ..Config::default()
        };
        assert!(!corresponde_default(&con_overrides));
    }

    #[test]
    fn el_perfil_default_es_el_reparto_pedido() {
        // El default debe existir y traducir el reparto: opus a lo pensativo, sonnet al coding,
        // haiku a la exploración (que brunelleschi/michelangelo —coding— queden en sonnet lo confirma).
        let perfil = turtle_service::perfil_por_nombre(PERFIL_DEFAULT).expect("default conocido");
        let cfg = aplicar_a_config(Config::default(), perfil);
        assert_eq!(
            cfg.overrides.get("donatello").map(String::as_str),
            Some("opus")
        );
        assert_eq!(
            cfg.overrides.get("brunelleschi").map(String::as_str),
            Some("sonnet")
        );
        assert_eq!(
            cfg.overrides.get("michelangelo").map(String::as_str),
            Some("sonnet")
        );
    }

    fn perfil(nombre: &str) -> &'static Perfil {
        turtle_service::perfil_por_nombre(nombre).unwrap()
    }
}
