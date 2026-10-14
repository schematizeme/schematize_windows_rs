//! **plataforma_windows** — os adaptadores Windows das costuras de `schematize_plataforma`.
//!
//! **O quê:** ADR-0026 separa o que muda por SO: os cores (market, overdev) são Unix e falam com
//! um `Plataforma` de traits. Este crate é o `Plataforma` do Windows — PowerShell (`powershell.exe`
//! 5.1), `curl.exe`, `winget`, Windows Terminal, atalhos `.lnk`, PATH de usuário no registro.
//! A lógica (montar comando, parsear saída, decidir) é PURA e testada no Linux; só o spawn toca
//! o SO.
//!
//! **Onde:** o `main` de cada `.exe` (`crate bins`) chama [`PlataformaWindowsExt::windows`].
//!
//! ## Seams e estratégia
//!
//! | costura | Windows |
//! |---|---|
//! | `Shell` | `powershell.exe -NoProfile -NonInteractive -Command`; `executar` por argv; `existe` por PATH+PATHEXT |
//! | `Rede` | `curl.exe` (Win10+) senão `Invoke-WebRequest`; `.part` + rename |
//! | `Substituicao` | `.exe` em uso vira `.old`; limpa na próxima execução; `taskkill /F /IM` |
//! | `Atalhos` | `.lnk` no Menu Iniciar via `WScript.Shell`; `.ico` a partir dos PNGs |
//! | `PathSetup` | `HKCU\Environment\Path` (idempotente) + `WM_SETTINGCHANGE` |
//! | `Terminal` | `wt.exe` ou `cmd /c start`; script bash roda no Git Bash |
//! | `Abridor` | `explorer.exe <alvo>` (sem shell no meio) |
//! | `Processos` | `Get-CimInstance Win32_Process` + `tasklist` |

pub mod abridor;
pub mod atalhos;
pub mod dirs;
pub mod exec;
pub mod ico;
pub mod pathsetup;
pub mod processos;
pub mod processos_parse;
pub mod ps;
pub mod rede;
pub mod shell;
pub mod substituicao;
pub mod terminal;

use schematize_plataforma::seams::Plataforma;

/// **O quê:** a `Plataforma` do Windows, com as implementações reais, e a limpeza das sobras
/// `.old` de trocas anteriores ao lado do executável atual (avisa em stderr o que não conseguiu).
///
/// **Onde:** o `main` de cada `.exe`. (Um `impl Plataforma { fn windows() }` direto é impossível:
/// a regra órfã proíbe `impl` inerente de tipo alheio — por isso o trait de extensão.)
pub fn plataforma_windows() -> Plataforma {
    limpar_olds_do_exe_atual();
    Plataforma {
        shell: Box::new(shell::ShellWindows),
        rede: Box::new(rede::RedeWindows),
        substituicao: Box::new(substituicao::SubstituicaoWindows),
        atalhos: Box::new(atalhos::AtalhosWindows),
        path: Box::new(pathsetup::PathSetupWindows),
        terminal: Box::new(terminal::TerminalWindows),
        abridor: Box::new(abridor::AbridorWindows),
        processos: Box::new(processos::ProcessosWindows::real()),
    }
}

/// **O quê:** apaga os `schematize*.old` do diretório do executável em execução (a limpeza
/// "na próxima execução" da troca de `.exe`). Erros reais vão ao stderr; "em uso" é esperado.
///
/// **Onde:** [`plataforma_windows`].
fn limpar_olds_do_exe_atual() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else { return };
    for e in substituicao::limpar_old(dir) {
        eprintln!("aviso: {e}");
    }
}

/// Extensão que dá o nome `Plataforma::windows()` pedido pelo ADR (traga o trait ao escopo).
pub trait PlataformaWindowsExt {
    /// **O quê:** [`plataforma_windows`]. **Onde:** `main` dos `.exe`.
    fn windows() -> Self;
}

impl PlataformaWindowsExt for Plataforma {
    /// **O quê:** delega a [`plataforma_windows`]. **Onde:** `Plataforma::windows()`.
    fn windows() -> Self {
        plataforma_windows()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: a composição monta todas as costuras e as puras respondem sem tocar o SO.
    /// Onde: `plataforma_windows` (no Linux só as consultas inofensivas).
    #[test]
    fn monta_e_responde() {
        let p = Plataforma::windows();
        assert!(!p.shell.existe("binario-inexistente-42"));
        assert!(p.abridor.abrir("").is_err());
        assert!(p.abridor.abrir("-x").is_err());
        assert!(p.substituicao.tornar_executavel(std::path::Path::new("x")).is_ok());
        assert_eq!(p.processos.carga1(), None);
    }
}
