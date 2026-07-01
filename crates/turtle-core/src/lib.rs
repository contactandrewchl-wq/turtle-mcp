//! `turtle-core` — Tipos del dominio y traits de Turtle (sin I/O).
//!
//! Capa base del workspace; no depende de ninguna otra (arquitectura §2).
//!
//! Define los tipos de memoria (`memory`), de sesión (`session`), de agente (`agent`), de evento
//! (`event`), de mensaje (`message`), de relación (`relation`), de skill (`skill`) y el catálogo
//! de localización (`strings`).

pub mod agent;
pub mod checkpoint;
pub mod event;
pub mod memory;
pub mod message;
pub mod relation;
pub mod session;
pub mod skill;
pub mod spec_lint;
pub mod strings;

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
