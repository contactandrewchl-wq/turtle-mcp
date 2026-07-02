//! Detector de términos ambiguos ("palabras comadreja") en especificaciones y requisitos.
//!
//! Un requisito con palabras vagas ("rápido", "escalable", "fácil de usar") **no es verificable**:
//! IEEE 29148 exige requisitos no ambiguos y verificables, y una palabra sin métrica hace que cada
//! persona entienda algo distinto y que nada se pueda probar ni estimar. Este módulo marca esos
//! términos y sugiere cómo concretarlos.
//!
//! Es el hermano de [`crate::strings::regionalismo_en`]: misma mecánica (comparación por palabra
//! completa para las de un término, subcadena para las frases), otra lista. Sin IA: determinista y
//! testeable. Lo consumen el tool MCP `spec_lint`, la CLI `turtle spec-lint` y la skill de Discovery.

/// Un término ambiguo del catálogo: la palabra/frase, su categoría y la pregunta que fuerza a
/// concretarlo por algo verificable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ambiguo {
    /// El término tal como se cataloga (en minúsculas).
    pub termino: &'static str,
    /// Categoría del problema: cualitativo, cuantificador, verbo-paraguas o escape.
    pub categoria: &'static str,
    /// Pregunta o pista para reemplazarlo por algo medible/observable.
    pub pista: &'static str,
}

/// Términos ambiguos de UNA sola palabra (se comparan por token). La lista no es exhaustiva; se
/// amplía conforme aparecen casos. Se acentúan como se escriben para no chocar con formas neutras.
pub const PALABRAS_AMBIGUAS: &[Ambiguo] = &[
    // Cualitativos sin métrica.
    Ambiguo {
        termino: "rápido",
        categoria: "cualitativo",
        pista: "¿cuánto? define un umbral (p. ej. p95 < 200 ms)",
    },
    Ambiguo {
        termino: "lento",
        categoria: "cualitativo",
        pista: "¿respecto de qué umbral medible?",
    },
    Ambiguo {
        termino: "escalable",
        categoria: "cualitativo",
        pista: "¿a cuántos usuarios/req concurrentes sin degradar?",
    },
    Ambiguo {
        termino: "eficiente",
        categoria: "cualitativo",
        pista: "¿en qué métrica (CPU, memoria, tiempo) y cuánto?",
    },
    Ambiguo {
        termino: "óptimo",
        categoria: "cualitativo",
        pista: "¿óptimo según qué criterio medible?",
    },
    Ambiguo {
        termino: "robusto",
        categoria: "cualitativo",
        pista: "¿ante qué fallas concretas debe resistir?",
    },
    Ambiguo {
        termino: "seguro",
        categoria: "cualitativo",
        pista: "¿contra qué amenaza y con qué control?",
    },
    Ambiguo {
        termino: "fácil",
        categoria: "cualitativo",
        pista: "¿criterio observable (pasos, tiempo, sin ayuda)?",
    },
    Ambiguo {
        termino: "simple",
        categoria: "cualitativo",
        pista: "¿medible cómo (pasos, opciones)?",
    },
    Ambiguo {
        termino: "intuitivo",
        categoria: "cualitativo",
        pista: "¿un usuario nuevo completa la tarea sin ayuda?",
    },
    Ambiguo {
        termino: "amigable",
        categoria: "cualitativo",
        pista: "¿qué criterio de usabilidad observable?",
    },
    Ambiguo {
        termino: "flexible",
        categoria: "cualitativo",
        pista: "¿qué debe poder variar, exactamente?",
    },
    Ambiguo {
        termino: "moderno",
        categoria: "cualitativo",
        pista: "¿qué versión o estándar concreto?",
    },
    Ambiguo {
        termino: "potente",
        categoria: "cualitativo",
        pista: "¿qué capacidad medible?",
    },
    Ambiguo {
        termino: "confiable",
        categoria: "cualitativo",
        pista: "¿qué disponibilidad/tasa de error (p. ej. 99.9%)?",
    },
    // Cuantificadores vagos.
    Ambiguo {
        termino: "algunos",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "varios",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "muchos",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "pocos",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "casi",
        categoria: "cuantificador",
        pista: "¿qué valor o rango exacto?",
    },
    Ambiguo {
        termino: "aproximadamente",
        categoria: "cuantificador",
        pista: "¿qué valor con tolerancia (p. ej. 100 ± 5)?",
    },
    // Verbos-paraguas.
    Ambiguo {
        termino: "manejar",
        categoria: "verbo-paraguas",
        pista: "¿qué operaciones exactas (crear/leer/actualizar/borrar)?",
    },
    Ambiguo {
        termino: "gestionar",
        categoria: "verbo-paraguas",
        pista: "¿qué acciones concretas?",
    },
    Ambiguo {
        termino: "soportar",
        categoria: "verbo-paraguas",
        pista: "¿qué debe hacer exactamente con eso?",
    },
    Ambiguo {
        termino: "procesar",
        categoria: "verbo-paraguas",
        pista: "¿qué transformación concreta produce?",
    },
    Ambiguo {
        termino: "mejorar",
        categoria: "verbo-paraguas",
        pista: "¿de qué valor a qué valor?",
    },
    Ambiguo {
        termino: "optimizar",
        categoria: "verbo-paraguas",
        pista: "¿qué métrica y hasta cuánto?",
    },
    // Escapes / cajón de sastre.
    Ambiguo {
        termino: "etc",
        categoria: "escape",
        pista: "enumera los casos o márcalo como incógnita abierta",
    },
    Ambiguo {
        termino: "etcétera",
        categoria: "escape",
        pista: "enumera los casos o márcalo como incógnita abierta",
    },
    // Inglés: los specs reales mezclan idiomas ("debe ser fast y user friendly") o llegan enteros
    // en inglés; las mismas comadrejas aplican. "simple" y "flexible" se escriben igual en ambos
    // idiomas y ya están arriba (no se duplican). Las pistas siguen en español (es-419).
    Ambiguo {
        termino: "fast",
        categoria: "cualitativo",
        pista: "¿cuánto? define un umbral (p. ej. p95 < 200 ms)",
    },
    Ambiguo {
        termino: "slow",
        categoria: "cualitativo",
        pista: "¿respecto de qué umbral medible?",
    },
    Ambiguo {
        termino: "scalable",
        categoria: "cualitativo",
        pista: "¿a cuántos usuarios/req concurrentes sin degradar?",
    },
    Ambiguo {
        termino: "efficient",
        categoria: "cualitativo",
        pista: "¿en qué métrica (CPU, memoria, tiempo) y cuánto?",
    },
    Ambiguo {
        termino: "optimal",
        categoria: "cualitativo",
        pista: "¿óptimo según qué criterio medible?",
    },
    Ambiguo {
        termino: "robust",
        categoria: "cualitativo",
        pista: "¿ante qué fallas concretas debe resistir?",
    },
    Ambiguo {
        termino: "secure",
        categoria: "cualitativo",
        pista: "¿contra qué amenaza y con qué control?",
    },
    Ambiguo {
        termino: "easy",
        categoria: "cualitativo",
        pista: "¿criterio observable (pasos, tiempo, sin ayuda)?",
    },
    Ambiguo {
        termino: "intuitive",
        categoria: "cualitativo",
        pista: "¿un usuario nuevo completa la tarea sin ayuda?",
    },
    Ambiguo {
        termino: "modern",
        categoria: "cualitativo",
        pista: "¿qué versión o estándar concreto?",
    },
    Ambiguo {
        termino: "powerful",
        categoria: "cualitativo",
        pista: "¿qué capacidad medible?",
    },
    Ambiguo {
        termino: "reliable",
        categoria: "cualitativo",
        pista: "¿qué disponibilidad/tasa de error (p. ej. 99.9%)?",
    },
    Ambiguo {
        termino: "some",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "several",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "many",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "few",
        categoria: "cuantificador",
        pista: "¿cuántos exactamente?",
    },
    Ambiguo {
        termino: "approximately",
        categoria: "cuantificador",
        pista: "¿qué valor con tolerancia (p. ej. 100 ± 5)?",
    },
    Ambiguo {
        termino: "handle",
        categoria: "verbo-paraguas",
        pista: "¿qué operaciones exactas (crear/leer/actualizar/borrar)?",
    },
    Ambiguo {
        termino: "manage",
        categoria: "verbo-paraguas",
        pista: "¿qué acciones concretas?",
    },
    Ambiguo {
        termino: "improve",
        categoria: "verbo-paraguas",
        pista: "¿de qué valor a qué valor?",
    },
    Ambiguo {
        termino: "optimize",
        categoria: "verbo-paraguas",
        pista: "¿qué métrica y hasta cuánto?",
    },
];

/// Términos ambiguos de VARIAS palabras (se detectan por subcadena en minúsculas).
pub const FRASES_AMBIGUAS: &[Ambiguo] = &[
    Ambiguo {
        termino: "easy to use",
        categoria: "cualitativo",
        pista: "¿qué criterio observable de usabilidad?",
    },
    Ambiguo {
        termino: "user friendly",
        categoria: "cualitativo",
        pista: "¿qué criterio de usabilidad observable?",
    },
    Ambiguo {
        termino: "user-friendly",
        categoria: "cualitativo",
        pista: "¿qué criterio de usabilidad observable?",
    },
    Ambiguo {
        termino: "as needed",
        categoria: "escape",
        pista: "¿bajo qué condición exacta?",
    },
    Ambiguo {
        termino: "if applicable",
        categoria: "escape",
        pista: "¿aplica o no? decídelo",
    },
    Ambiguo {
        termino: "and so on",
        categoria: "escape",
        pista: "enumera los casos",
    },
    Ambiguo {
        termino: "among others",
        categoria: "escape",
        pista: "enumera los casos",
    },
    Ambiguo {
        termino: "fácil de usar",
        categoria: "cualitativo",
        pista: "¿qué criterio observable de usabilidad?",
    },
    Ambiguo {
        termino: "de forma segura",
        categoria: "cualitativo",
        pista: "¿contra qué amenaza y con qué control?",
    },
    Ambiguo {
        termino: "según sea necesario",
        categoria: "escape",
        pista: "¿bajo qué condición exacta?",
    },
    Ambiguo {
        termino: "cuando corresponda",
        categoria: "escape",
        pista: "¿cuándo, exactamente?",
    },
    Ambiguo {
        termino: "si aplica",
        categoria: "escape",
        pista: "¿aplica o no? decídelo",
    },
    Ambiguo {
        termino: "entre otros",
        categoria: "escape",
        pista: "enumera los casos",
    },
    Ambiguo {
        termino: "y demás",
        categoria: "escape",
        pista: "enumera los casos",
    },
    Ambiguo {
        termino: "más o menos",
        categoria: "cuantificador",
        pista: "¿qué valor con tolerancia?",
    },
    Ambiguo {
        termino: "la mayoría",
        categoria: "cuantificador",
        pista: "¿qué porcentaje o cuáles?",
    },
];

/// Quita las tildes y la diéresis (á→a, ñ→n, ü→u) para comparar sin depender de que la persona
/// escriba los acentos: en specs reales conviven "rápido" y "rapido". La `ñ` se mapea a `n` solo
/// para el cotejo (ninguna comadreja del catálogo la usa como distintivo). Pública porque también
/// normaliza claves estables (p. ej. el `topic_key` sugerido: "decisión" y "decision" deben dar
/// la misma clave).
pub fn sin_acentos(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            otro => otro,
        })
        .collect()
}

/// Devuelve los términos ambiguos presentes en `texto`, sin repetir, primero las frases y luego las
/// palabras (en el orden del catálogo, determinista). Las palabras se comparan por token (delimitado
/// por caracteres no alfabéticos) para no marcar subcadenas —p. ej. "casi" no dispara dentro de
/// "casino"—; las frases, por subcadena. Comparación insensible a mayúsculas Y a acentos (así "rápido"
/// y "rapido" se detectan por igual).
pub fn terminos_ambiguos_en(texto: &str) -> Vec<&'static Ambiguo> {
    let baja = sin_acentos(&texto.to_lowercase());
    let mut out: Vec<&'static Ambiguo> = Vec::new();
    for f in FRASES_AMBIGUAS {
        if baja.contains(&sin_acentos(f.termino)) {
            out.push(f);
        }
    }
    let tokens: std::collections::BTreeSet<&str> = baja
        .split(|c: char| !c.is_alphabetic())
        .filter(|t| !t.is_empty())
        .collect();
    for p in PALABRAS_AMBIGUAS {
        if tokens.contains(sin_acentos(p.termino).as_str()) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::terminos_ambiguos_en;

    #[test]
    fn detecta_comadrejas_y_da_categoria() {
        let hits = terminos_ambiguos_en("El sistema debe ser rápido, escalable y fácil de usar.");
        let terminos: Vec<&str> = hits.iter().map(|a| a.termino).collect();
        assert!(terminos.contains(&"rápido"), "{terminos:?}");
        assert!(terminos.contains(&"escalable"), "{terminos:?}");
        assert!(terminos.contains(&"fácil de usar"), "{terminos:?}");
        // Toda comadreja trae categoría y pista no vacías.
        assert!(hits
            .iter()
            .all(|a| !a.categoria.is_empty() && !a.pista.is_empty()));
    }

    #[test]
    fn ignora_prosa_concreta_y_verificable() {
        // Un requisito ya concreto no dispara nada.
        let hits = terminos_ambiguos_en(
            "El endpoint /checkout responde en p95 < 200 ms con 5000 req/min y devuelve 201.",
        );
        assert!(
            hits.is_empty(),
            "no debería marcar prosa concreta: {hits:?}"
        );
    }

    #[test]
    fn no_marca_subcadenas_por_token() {
        // "casi" no debe dispararse dentro de "casino"; "seguro" no dentro de "seguridad".
        assert!(terminos_ambiguos_en("Abrimos un casino con seguridad perimetral.").is_empty());
    }

    #[test]
    fn detecta_verbos_paraguas_y_cuantificadores() {
        let hits = terminos_ambiguos_en("Debe manejar varios usuarios, según sea necesario.");
        let t: Vec<&str> = hits.iter().map(|a| a.termino).collect();
        assert!(t.contains(&"manejar"));
        assert!(t.contains(&"varios"));
        assert!(t.contains(&"según sea necesario"));
    }

    #[test]
    fn detecta_comadrejas_en_ingles() {
        // Specs en inglés (o mezclados) traen las mismas comadrejas; se detectan igual.
        let t: Vec<&str> = terminos_ambiguos_en(
            "The system must be fast, scalable and user-friendly. It should handle several \
             payments as needed.",
        )
        .iter()
        .map(|a| a.termino)
        .collect();
        assert!(t.contains(&"fast"), "{t:?}");
        assert!(t.contains(&"scalable"), "{t:?}");
        assert!(t.contains(&"user-friendly"), "{t:?}");
        assert!(t.contains(&"handle"), "{t:?}");
        assert!(t.contains(&"several"), "{t:?}");
        assert!(t.contains(&"as needed"), "{t:?}");
        // La prosa concreta en inglés no dispara nada.
        assert!(terminos_ambiguos_en(
            "The /checkout endpoint responds in p95 < 200 ms at 5000 req/min and returns 201."
        )
        .is_empty());
    }

    #[test]
    fn insensible_a_acentos() {
        // Sin tildes (como suele escribir la gente) se detecta igual que con tildes.
        let t: Vec<&str> =
            terminos_ambiguos_en("Debe ser rapido, facil y optimo. Y la mayoria funciona.")
                .iter()
                .map(|a| a.termino)
                .collect();
        assert!(t.contains(&"rápido"), "{t:?}");
        assert!(t.contains(&"fácil"), "{t:?}");
        assert!(t.contains(&"óptimo"), "{t:?}");
        assert!(t.contains(&"la mayoría"), "{t:?}");
    }
}
