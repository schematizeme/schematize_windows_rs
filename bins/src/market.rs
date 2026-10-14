//! **schematize-market** (Windows) — instalar e atualizar linguagens, ferramentas e os apps da casa.
//!
//! Casca fina (ADR-0026): monta a `Plataforma` Windows e as `Receitas` Windows e entrega ao core
//! (`market::cli::run`), que é o mesmo código do Linux/macOS/FreeBSD.

use plataforma_windows::PlataformaWindowsExt;
use schematize_plataforma::seams::Plataforma;

/// Ponto de entrada: costuras reais do Windows + receitas Windows; o código de saída é o da CLI.
fn main() {
    let plat = Plataforma::windows();
    let receitas = market_windows::ReceitasWindows::detectar();
    let code = market::cli::run(std::env::args_os(), &plat, &receitas);
    std::process::exit(code);
}
