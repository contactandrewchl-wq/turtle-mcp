//! `turtle spec-lint` — marca términos ambiguos ("palabras comadreja") en un texto de spec.
//!
//! Un requisito con palabras vagas no es verificable (IEEE 29148). Este comando lee un archivo (o la
//! entrada estándar) y lista los términos ambiguos con la pregunta para concretar cada uno. No toca
//! la base ni la config: es puro texto. El detector vive en `turtle_core::spec_lint`.

use std::io::Read;
use std::path::PathBuf;

/// Ejecuta `turtle spec-lint [archivo]`: lee el texto, detecta las comadrejas e imprime el informe.
pub(crate) fn ejecutar(archivo: Option<PathBuf>) -> Result<(), String> {
    let texto = leer(archivo)?;
    let hits = turtle_core::spec_lint::terminos_ambiguos_en(&texto);
    if hits.is_empty() {
        println!("Sin términos ambiguos: el texto se lee verificable.");
        return Ok(());
    }
    println!(
        "Se encontraron {} término(s) ambiguo(s) que conviene concretar:\n",
        hits.len()
    );
    for a in &hits {
        println!("  - {:<22} [{}]  {}", a.termino, a.categoria, a.pista);
    }
    println!(
        "\nUn requisito con estas palabras no es verificable. Reemplázalas por números o criterios \
         observables antes de cerrar el spec."
    );
    Ok(())
}

/// Lee el texto a revisar: del archivo si se dio uno, o de la entrada estándar.
fn leer(archivo: Option<PathBuf>) -> Result<String, String> {
    match archivo {
        Some(ruta) => std::fs::read_to_string(&ruta)
            .map_err(|e| format!("no se pudo leer {}: {e}", ruta.display())),
        None => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| format!("no se pudo leer la entrada estándar: {e}"))?;
            Ok(s)
        }
    }
}
