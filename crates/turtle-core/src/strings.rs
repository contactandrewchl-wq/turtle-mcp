//! Catálogo central de cadenas dirigidas a la persona, en **español latino neutro (es-419)**.
//!
//! Toda salida visible para la persona (CLI, errores, ayuda) debe pasar por este
//! catálogo, para poder verificarla por inspección automatizada (RNF-LOC-01, T0-06/T0-07).
//! Los logs técnicos NO van aquí: pueden quedar en inglés por convención (RNF-LOC-03).

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

#[cfg(test)]
mod tests {
    use super::user_facing_catalog;

    /// Patrones prohibidos: voseo y modismos regionales (RNF-LOC-01).
    /// La lista no es exhaustiva; se amplía conforme crece el catálogo.
    const PROHIBIDOS: &[&str] = &[
        "tenés", "querés", "podés", "sabés", "vení", "andá", "fijate", "mirá", " vos ", "órale",
        "ándale", "chévere", "bacán", "guay", "al tiro", "ahorita", "porfa", "che,", "po,",
    ];

    #[test]
    fn localizacion_sin_regionalismos() {
        for cadena in user_facing_catalog() {
            let baja = cadena.to_lowercase();
            for prohibido in PROHIBIDOS {
                assert!(
                    !baja.contains(prohibido),
                    "La cadena {cadena:?} contiene el patrón prohibido {prohibido:?} (RNF-LOC-01)"
                );
            }
        }
    }
}
