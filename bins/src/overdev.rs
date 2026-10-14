//! **schematize-overdev** (Windows) — o motor de um run: fila, trava, supervisor e gate.
//!
//! Casca fina (ADR-0026): injeta a `Plataforma` Windows e os `Hooks` Windows na lib
//! (`overdev_casa::cli::run`), a mesma dos demais sistemas.

use overdev_windows::HooksWindows;
use plataforma_windows::PlataformaWindowsExt;
use schematize_plataforma::seams::Plataforma;

/// Ponto de entrada: o código de saída é o da CLI.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = overdev_casa::cli::run(args, Plataforma::windows(), Box::new(HooksWindows::novo()));
    std::process::exit(code);
}
