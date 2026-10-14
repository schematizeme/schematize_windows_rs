//! **processos** — a costura [`Processos`] do Windows: `Get-CimInstance` + `tasklist`.
//!
//! **O quê:** `vivo` (tasklist, pela lib), `contar`/`pids_por_argv`/`em_dir` pela listagem
//! `Win32_Process` (PID, nome, linha de comando), RAM livre por `Win32_OperatingSystem`.
//!
//! **Limitações (documentadas, nunca inventadas):**
//! - **cwd:** o Windows não revela o diretório de trabalho de outro processo sem API
//!   privilegiada. `em_dir` e `pids_por_argv_em` aproximam por "a LINHA DE COMANDO cita o
//!   diretório". Processo que não cita o próprio cwd na linha de comando não casa (falso
//!   negativo; nunca falso positivo).
//! - `carga1`: o Windows não tem load average → sempre `None`.
//! - Se o PowerShell falhar, as consultas devolvem vazio/`None` ("não sei"), como as Unix.
//!
//! **Onde:** `Plataforma::windows().processos`: trava/supervisor do overdev e orçamento de agentes.

use crate::exec::rodar_ps;
use crate::processos_parse::{
    cita_dir, e_o_processo, parse_memoria_mb, parse_processos, pids_por_trechos, Proc,
    SCRIPT_LISTAR, SCRIPT_MEMORIA,
};
use schematize_plataforma::seams::processos::{
    ExecutorReal, Modo, ProcReal, ProcessosCom, Sistema,
};
use schematize_plataforma::seams::Processos;
use std::path::Path;
use std::sync::Arc;

/// Quem executa scripts PowerShell (injetável para os testes).
pub trait ExecPs: Send + Sync {
    /// **O quê:** roda `script` e devolve o stdout, ou `None` se falhou.
    /// **Onde:** [`ProcessosWindows`].
    fn rodar(&self, script: &str) -> Option<String>;
}

/// Executor real: o `powershell.exe`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ExecPsReal;

impl ExecPs for ExecPsReal {
    /// **O quê:** [`rodar_ps`], com a falha vista como "desconhecido". **Onde:** produção.
    fn rodar(&self, script: &str) -> Option<String> {
        rodar_ps(script).ok()
    }
}

/// Os processos do Windows.
#[derive(Clone)]
pub struct ProcessosWindows {
    base: ProcessosCom,
    ps: Arc<dyn ExecPs>,
}

impl ProcessosWindows {
    /// **O quê:** a implementação real (tasklist + PowerShell). **Onde:** `Plataforma::windows()`.
    pub fn real() -> Self {
        Self::novo(
            ProcessosCom::novo(Sistema::Windows, Arc::new(ProcReal), Arc::new(ExecutorReal)),
            Arc::new(ExecPsReal),
        )
    }

    /// **O quê:** com `base` (vivo/contar de reserva) e o executor injetados. **Onde:** testes.
    pub fn novo(base: ProcessosCom, ps: Arc<dyn ExecPs>) -> Self {
        Self { base, ps }
    }

    /// **O quê:** a lista de processos; `None` se o PowerShell falhou. **Onde:** as consultas.
    fn listar(&self) -> Option<Vec<Proc>> {
        self.ps.rodar(SCRIPT_LISTAR).map(|t| parse_processos(&t))
    }
}

impl Processos for ProcessosWindows {
    /// **O quê:** o PID existe (tasklist, pela lib). **Onde:** dono de trava.
    fn vivo(&self, pid: u32) -> bool {
        self.base.vivo(pid)
    }

    /// **O quê:** quantos processos são `nome` (imagem ou node rodando `nome`); cai no tasklist
    /// da lib se a listagem CIM falhar. **Onde:** orçamento de agentes.
    fn contar(&self, nome: &str) -> usize {
        match self.listar() {
            Some(l) => l.iter().filter(|p| e_o_processo(p, nome)).count(),
            None => self.base.contar(nome),
        }
    }

    /// **O quê:** algum `nome` cuja linha de comando cita `dir` (ver limitação do módulo; `modo`
    /// não distingue exato/prefixo aqui). **Onde:** supervisor do overdev.
    fn em_dir(&self, nome: &str, dir: &Path, _modo: Modo) -> bool {
        let d = dir.to_string_lossy();
        self.listar().is_some_and(|l| l.iter().any(|p| e_o_processo(p, nome) && cita_dir(p, &d)))
    }

    /// **O quê:** pids (sem o próprio) cuja linha de comando contém todos os `trechos`.
    /// **Onde:** supervisor do overdev.
    fn pids_por_argv(&self, trechos: &[&str]) -> Vec<u32> {
        match self.listar() {
            Some(l) => pids_por_trechos(&l, trechos, std::process::id()),
            None => Vec::new(),
        }
    }

    /// **O quê:** como `pids_por_argv`, mantendo só os que citam `dir` na linha de comando.
    /// **Onde:** supervisor do overdev (argv E projeto).
    fn pids_por_argv_em(&self, trechos: &[&str], dir: &Path) -> Vec<u32> {
        let Some(l) = self.listar() else {
            return Vec::new();
        };
        let d = dir.to_string_lossy();
        let achados = pids_por_trechos(&l, trechos, std::process::id());
        l.iter().filter(|p| achados.contains(&p.pid) && cita_dir(p, &d)).map(|p| p.pid).collect()
    }

    /// **O quê:** RAM livre em MB (`FreePhysicalMemory`). **Onde:** orçamento de agentes.
    fn memoria_disponivel_mb(&self) -> Option<u64> {
        parse_memoria_mb(&self.ps.rodar(SCRIPT_MEMORIA)?)
    }

    /// **O quê:** `None` — o Windows não tem load average. **Onde:** orçamento de agentes.
    fn carga1(&self) -> Option<f64> {
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    struct Falso(Option<String>);
    impl ExecPs for Falso {
        fn rodar(&self, script: &str) -> Option<String> {
            if script == SCRIPT_MEMORIA {
                return Some("4194304".into());
            }
            self.0.clone()
        }
    }

    fn proc(csv: Option<&str>) -> ProcessosWindows {
        ProcessosWindows::novo(
            ProcessosCom::novo(Sistema::Windows, Arc::new(ProcReal), Arc::new(ExecutorReal)),
            Arc::new(Falso(csv.map(String::from))),
        )
    }

    const CSV: &str = "\"ProcessId\",\"Name\",\"CommandLine\"\r\n\
\"10\",\"claude.exe\",\"claude.exe -p x C:\\Proj\\a\"\r\n\
\"11\",\"claude.exe\",\"claude.exe\"\r\n\
\"12\",\"schematize-overdev.exe\",\"schematize-overdev.exe supervise C:\\Proj\\a\"\r\n";

    /// O quê: contar/em_dir/pids pela listagem. Onde: `ProcessosWindows`.
    #[test]
    fn consultas() {
        let p = proc(Some(CSV));
        assert_eq!(p.contar("claude"), 2);
        assert!(p.em_dir("claude", Path::new("C:\\proj\\A"), Modo::Prefixo));
        assert!(!p.em_dir("claude", Path::new("C:\\Outro"), Modo::Exato));
        assert_eq!(p.pids_por_argv(&["overdev", "supervise"]), vec![12]);
        assert_eq!(p.pids_por_argv_em(&["claude"], Path::new("C:\\Proj\\a")), vec![10]);
        assert!(p.pids_por_argv(&[]).is_empty());
    }

    /// O quê: falha do PowerShell vira vazio/None (nunca número inventado). Onde: `ProcessosWindows`.
    #[test]
    fn falha_vira_desconhecido() {
        let p = proc(None);
        assert!(p.pids_por_argv(&["x"]).is_empty());
        assert!(!p.em_dir("claude", Path::new("C:\\a"), Modo::Exato));
        assert_eq!(p.carga1(), None);
    }

    /// O quê: memória em MB e carga None. Onde: `memoria_disponivel_mb`.
    #[test]
    fn memoria_e_carga() {
        let p = proc(None);
        assert_eq!(p.memoria_disponivel_mb(), Some(4096));
        assert_eq!(p.carga1(), None);
    }
}
