//! **schematize-overdev-gui** (Windows) — a janela do `schematize-overdev` (iced, tiny-skia).
//!
//! Subsistema `windows`: sem console atrás da janela. Casca fina (ADR-0026): injeta a
//! `Plataforma` e os `Hooks` do Windows em `overdev_casa::gui::run`.
#![windows_subsystem = "windows"]

use overdev_windows::HooksWindows;
use plataforma_windows::PlataformaWindowsExt;
use schematize_plataforma::seams::Plataforma;

/// Ponto de entrada: abre a janela; erro do iced vai ao stderr (que no subsistema `windows` pode
/// não existir) e vira código 1.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(e) =
        overdev_casa::gui::run(args, Plataforma::windows(), Box::new(HooksWindows::novo()))
    {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}
