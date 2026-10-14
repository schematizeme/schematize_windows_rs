//! **shell** — a costura [`Shell`] do Windows: PowerShell.
//!
//! **O quê:** `run`/`run_herdado` executam o texto do plano de receita no
//! `powershell.exe -NoProfile -NonInteractive -Command`; `executar` roda um argv direto
//! (resolvendo `PATH`+`PATHEXT`, para achar `npm.cmd`, `winget.exe`…); `existe` consulta o
//! `PATH` pela regra do Windows.
//!
//! **Onde:** `Plataforma::windows().shell`, usado por `market` (`exec_step`, detecção de
//! binários) e pelo overdev.

use crate::exec::{powershell_exe, resolver_no_ambiente};
use crate::ps::argv_command;
use schematize_plataforma::seams::Shell;
use std::io;
use std::process::{ExitStatus, Output, Stdio};

/// O shell do Windows: PowerShell 5.1.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShellWindows;

impl Shell for ShellWindows {
    /// **O quê:** roda `cmd` no PowerShell capturando a saída. **Onde:** `Plataforma.shell.run`.
    fn run(&self, cmd: &str) -> io::Result<Output> {
        std::process::Command::new(powershell_exe())
            .args(argv_command(cmd))
            .stdin(Stdio::null())
            .output()
    }

    /// **O quê:** `bin` está no `PATH` (com `PATHEXT`)? **Onde:** detecção de runtimes/ferramentas.
    fn existe(&self, bin: &str) -> bool {
        resolver_no_ambiente(bin).is_some()
    }

    /// **O quê:** executa `argv` direto, sem shell, resolvendo o programa por `PATH`+`PATHEXT`
    /// (o `Command::new` do Rust só tenta `.exe`). **Onde:** `Plataforma.shell.executar`.
    fn executar(&self, argv: &[&str], herdar_stdio: bool) -> io::Result<ExitStatus> {
        let (prog, args) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "argv vazio"))?;
        let alvo = resolver_no_ambiente(prog).unwrap_or_else(|| std::path::PathBuf::from(prog));
        let mut c = std::process::Command::new(alvo);
        c.args(args);
        if herdar_stdio {
            c.stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        } else {
            c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        }
        c.status()
    }

    /// **O quê:** roda `cmd` no PowerShell herdando o terminal (saída ao vivo, prompts do
    /// winget/UAC). **Onde:** `ReceitasWindows::exec_step`.
    fn run_herdado(&self, cmd: &str) -> io::Result<ExitStatus> {
        let ps = powershell_exe();
        let ps_s = ps.to_string_lossy().into_owned();
        let args = argv_command(cmd);
        let mut v: Vec<&str> = vec![ps_s.as_str()];
        v.extend(args.iter().map(String::as_str));
        self.executar(&v, true)
    }
}
