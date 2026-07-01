//! Catálogo central de cadenas dirigidas a la persona, en **español latino neutro (es-419)**.
//!
//! Regla estricta de Turtle (RNF-LOC-01/02, CC-11): todo lo que el sistema muestra a la persona y
//! toda la guía que Turtle entrega a los LLMs se expresa en español latino neutro, SIN voseo, SIN
//! regionalismos y SIN modismos de ningún país. Los identificadores técnicos (nombres de variables,
//! comandos, rutas) se mantienen en su idioma original. La regla es verificable por inspección
//! automatizada: `regionalismo_en` detecta patrones prohibidos y varias pruebas la aplican (aquí, en
//! el protocolo que se inyecta a cada cliente y en las skills semilla). Los logs técnicos NO son
//! salida a la persona: pueden quedar en inglés por convención (RNF-LOC-03).

/// Devuelve todas las cadenas de interfaz dirigidas a la persona.
///
/// La prueba `localizacion_sin_regionalismos` recorre este catálogo y rechaza voseo,
/// regionalismos y modismos atados a un país.
pub fn user_facing_catalog() -> &'static [&'static str] {
    &[
        "Memoria guardada.",
        "Memoria actualizada.",
        "Memoria eliminada.",
        "No se encontraron memorias.",
        "Sesión iniciada.",
        "Sesión cerrada.",
        "No hay un proyecto activo.",
    ]
}

/// Palabras sueltas prohibidas por la regla de español latino neutro (RNF-LOC-01): conjugaciones e
/// imperativos voseo (acentuados) y modismos regionales de un solo término. Se comparan como
/// PALABRA COMPLETA (no subcadena), para no confundir "guarda" con "guardá" ni marcar "tiempo,"
/// como "po". La lista no es exhaustiva; se amplía conforme aparecen nuevos casos.
pub const PALABRAS_PROHIBIDAS: &[&str] = &[
    // Voseo — conjugaciones (presente).
    "tenés",
    "querés",
    "podés",
    "sabés",
    "hacés",
    "ponés",
    "decís",
    "vivís",
    "venís",
    "salís",
    "vos",
    // Voseo — imperativos afirmativos (el caso que más se filtra en prompts y comentarios).
    "guardá",
    "buscá",
    "cargá",
    "usá",
    "delegá",
    "revisá",
    "llamá",
    "respondé",
    "resolvé",
    "mirá",
    "fijate",
    "vení",
    "andá",
    "traé",
    "mandá",
    "dejá",
    "sumá",
    "agregá",
    "entregá",
    "escribí",
    "leé",
    "corré",
    "poné",
    "tené",
    "hacé",
    "decí",
    "salí",
    "seguí",
    "elegí",
    "abrí",
    "subí",
    "recuperá",
    "empezá",
    "arrancá",
    "mostrá",
    // Modismos regionales (varios países). Se excluyen a propósito términos que colisionan con
    // español neutro legítimo (p. ej. "posta" = puesto sanitario, "capo" = cejilla de guitarra).
    "órale",
    "ándale",
    "chido",
    "chévere",
    "bacán",
    "guay",
    "ahorita",
    "porfa",
    "che",
    "po",
    "laburo",
    "pibe",
    "chamba",
];

/// Frases (varias palabras) prohibidas: modismos que no son un solo término. Se detectan por
/// subcadena en minúsculas. NO se incluye "de una" ni "capaz que": colisionan con prosa neutra
/// habitual ("la forma de una respuesta", "antes de una sola línea", "es capaz de").
pub const FRASES_PROHIBIDAS: &[&str] = &["al tiro", "ni ahí"];

/// Devuelve el primer patrón prohibido (voseo o modismo regional) presente en `texto`, o `None` si
/// el texto respeta la regla de español latino neutro. Comparación insensible a mayúsculas: las
/// palabras se buscan por token (delimitadas por caracteres no alfabéticos) y las frases por
/// subcadena. La usan el guardián del catálogo y el del protocolo inyectado a los clientes.
pub fn regionalismo_en(texto: &str) -> Option<&'static str> {
    let baja = texto.to_lowercase();
    if let Some(f) = FRASES_PROHIBIDAS.iter().copied().find(|f| baja.contains(f)) {
        return Some(f);
    }
    for token in baja.split(|c: char| !c.is_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        if let Some(p) = PALABRAS_PROHIBIDAS.iter().copied().find(|w| *w == token) {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{regionalismo_en, user_facing_catalog};

    #[test]
    fn localizacion_sin_regionalismos() {
        for cadena in user_facing_catalog() {
            assert_eq!(
                regionalismo_en(cadena),
                None,
                "La cadena {cadena:?} contiene un patrón prohibido (RNF-LOC-01)"
            );
        }
    }

    #[test]
    fn detecta_voseo_e_ignora_neutro() {
        // Voseo (imperativos y conjugaciones) se detecta…
        assert!(regionalismo_en("Guardá tu trabajo con checkpoint_save").is_some());
        assert!(regionalismo_en("Respondé a la persona").is_some());
        assert!(regionalismo_en("Si tenés dudas, buscá primero").is_some());
        // …y las formas neutras equivalentes pasan.
        assert_eq!(
            regionalismo_en("Guarda tu trabajo con checkpoint_save"),
            None
        );
        assert_eq!(regionalismo_en("Responde a la persona"), None);
        assert_eq!(regionalismo_en("Si tienes dudas, busca primero"), None);
    }

    #[test]
    fn no_marca_palabras_neutras_que_contienen_un_patron() {
        // "po" y "che" solo se marcan como palabra completa: no deben disparar dentro de otras.
        assert_eq!(
            regionalismo_en("En poco tiempo, el grupo cerró la noche."),
            None
        );
        assert_eq!(
            regionalismo_en("El campo de texto y el cuerpo del mensaje."),
            None
        );
        // "usa/carga/revisa" neutras no deben confundirse con el voseo acentuado.
        assert_eq!(
            regionalismo_en("El usuario usa y carga y revisa el archivo."),
            None
        );
    }

    #[test]
    fn detecta_modismos_regionales() {
        assert!(regionalismo_en("dale al tiro").is_some());
        assert!(regionalismo_en("qué chévere").is_some());
        assert!(regionalismo_en("órale").is_some());
    }
}
