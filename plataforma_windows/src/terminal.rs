//! **terminal** — a costura [`Terminal`] do Windows: Windows Terminal ou `cmd /c start`.
//!
//! **O quê:** os scripts que o host entrega (`ScriptTerminal`) são em **bash** (o cabeçalho
//! `#!/bin/sh`, `exec`, `$HOME`… é do contrato Unix). No Windows eles rodam no **Git Bash**
//! (`bash.exe` do Git for Windows — o Claude Code já o exige), aberto numa aba do Windows
//! Terminal (`wt.exe`) ou numa janela nova (`cmd /c start`). O `bash.exe` de `System32` é o
//! launcher do WSL e é IGNORADO de propósito. Sem Git Bash, só o script trivial
//! `exec <comando>` (o que o market gera) é traduzido para PowerShell (`-NoExit -File x.ps1`);
//! qualquer outro corpo recebe um erro acionável. O planejamento é PURO e testado no Linux.
//!
//! **Onde:** `Plataforma::windows().terminal`: "abrir terminal no projeto", `vps` do market,
//! lançamento do agente no overdev.

use crate::exec::{resolver_alias_no_ambiente, resolver_no_ambiente};
use crate::ps::arg_win;
use schematize_plataforma::seams::terminal::{gravar_script, Entrega, ScriptTerminal};
use schematize_plataforma::seams::Terminal;
use std::path::{Path, PathBuf};

/// O que existe nesta máquina para abrir um terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ambiente {
    /// `wt.exe` (Windows Terminal), se instalado.
    pub wt: Option<PathBuf>,
    /// `bash.exe` do Git for Windows, se instalado.
    pub git_bash: Option<PathBuf>,
}

/// Um lançamento: programa + argv (ou linha crua para o `cmd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lancamento {
    /// Programa a iniciar.
    pub prog: PathBuf,
    /// Argumentos normais (quoting do Rust). Vazio se `bruto` for `Some`.
    pub args: Vec<String>,
    /// Linha de comando CRUA (já com quoting do Windows), para `cmd /c start`.
    pub bruto: Option<String>,
}

/// O plano completo de abrir um script: arquivos a gravar e o lançamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plano {
    /// `(caminho, corpo)` extras a gravar (o `.ps1` traduzido). O script do host é gravado à parte.
    pub escrever: Vec<(PathBuf, String)>,
    /// Como iniciar.
    pub lancar: Lancamento,
}

/// **O quê:** acha o `bash.exe` do Git for Windows: `ProgramFiles`, `ProgramW6432`,
/// `ProgramFiles(x86)` e `%LOCALAPPDATA%\Programs` + `\Git\bin\bash.exe`, depois a pasta do
/// `git.exe` do PATH (`…\cmd\git.exe` → `…\bin\bash.exe`). Nunca devolve o de `System32` (WSL).
/// PURA a partir de `get` e `existe`. **Onde:** [`Ambiente::detectar`] e os testes.
pub fn achar_git_bash(
    get: &dyn Fn(&str) -> Option<String>,
    existe: &dyn Fn(&Path) -> bool,
    git_exe: Option<&Path>,
) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = Vec::new();
    for k in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(v) = get(k).filter(|v| !v.trim().is_empty()) {
            cands.push(PathBuf::from(v).join("Git").join("bin").join("bash.exe"));
        }
    }
    if let Some(v) = get("LOCALAPPDATA").filter(|v| !v.trim().is_empty()) {
        cands.push(PathBuf::from(v).join("Programs").join("Git").join("bin").join("bash.exe"));
    }
    if let Some(g) = git_exe {
        if let Some(raiz) = g.parent().and_then(Path::parent) {
            cands.push(raiz.join("bin").join("bash.exe"));
        }
    }
    cands.into_iter().find(|c| existe(c) && !eh_bash_do_wsl(c))
}

/// **O quê:** `p` é o `bash.exe` do WSL (em `System32`/`SysNative`)? PURA. **Onde:**
/// [`achar_git_bash`].
pub fn eh_bash_do_wsl(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase().replace('/', "\\");
    s.contains("\\system32\\") || s.contains("\\sysnative\\")
}

impl Ambiente {
    /// **O quê:** o ambiente desta máquina (disco e PATH reais). **Onde:** `TerminalWindows`.
    pub fn detectar() -> Self {
        let git = resolver_no_ambiente("git.exe");
        Ambiente {
            wt: resolver_alias_no_ambiente("wt.exe"),
            git_bash: achar_git_bash(&|k| std::env::var(k).ok(), &|p| p.is_file(), git.as_deref()),
        }
    }
}

/// **O quê:** se `corpo` é só `[#!shebang]` + UMA linha `exec <comando>` (o script do market),
/// devolve `<comando>`; senão `None`. PURA. **Onde:** [`planejar`] sem Git Bash.
pub fn traduzir_exec(corpo: &str) -> Option<String> {
    let mut uteis = corpo.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    let linha = uteis.next()?;
    if uteis.next().is_some() {
        return None;
    }
    let cmd = linha.strip_prefix("exec ")?.trim();
    if cmd.is_empty() {
        return None;
    }
    Some(cmd.to_string())
}

/// **O quê:** o corpo do `.ps1`: o comando, com `&` na frente se começa por aspas (senão o
/// PowerShell leria uma string solta). PURA. **Onde:** [`planejar`].
pub fn corpo_ps1(comando: &str) -> String {
    let c = comando.trim();
    let c = if c.starts_with(['"', '\'']) { format!("& {c}") } else { c.to_string() };
    format!("\u{feff}{c}\r\n")
}

/// **O quê:** recusa caminho que o `cmd /c start` reinterpretaria (`%`, `"`, `!`, `^`, `&`).
/// PURA. **Onde:** [`planejar`].
fn caminho_seguro_para_cmd(p: &Path) -> Result<(), String> {
    let s = p.to_string_lossy();
    if s.contains(['%', '"', '!', '^', '&', '\0', '\n', '\r']) {
        return Err(format!(
            "o caminho {s} tem caracteres que o terminal do Windows não aceita (% \" ! ^ &)."
        ));
    }
    Ok(())
}

/// **O quê:** monta o lançamento do `programa`+`args` na aba do Windows Terminal (se houver) ou
/// numa janela do `cmd /c start`. PURA. **Onde:** [`planejar`].
fn lancar(
    amb: &Ambiente,
    prog: &Path,
    args: &[String],
    titulo: &str,
) -> Result<Lancamento, String> {
    let sem_ponto_e_virgula =
        !prog.to_string_lossy().contains(';') && args.iter().all(|a| !a.contains(';'));
    if let (Some(wt), true) = (&amb.wt, sem_ponto_e_virgula) {
        let mut a: Vec<String> = vec!["new-tab".into(), "--title".into(), titulo.into()];
        a.push(prog.to_string_lossy().into_owned());
        a.extend(args.iter().cloned());
        return Ok(Lancamento { prog: wt.clone(), args: a, bruto: None });
    }
    caminho_seguro_para_cmd(prog)?;
    let mut linha = format!("/c start \"\" {}", arg_win(&prog.to_string_lossy()));
    for a in args {
        if a.contains(['%', '!', '^', '&', '\0']) {
            return Err(format!("argumento com caractere que o cmd reinterpretaria: {a}"));
        }
        linha.push(' ');
        linha.push_str(&arg_win(a));
    }
    Ok(Lancamento { prog: PathBuf::from("cmd.exe"), args: vec![], bruto: Some(linha) })
}

/// **O quê:** decide COMO abrir `s`: Git Bash se houver; senão tradução PowerShell do script
/// trivial; senão erro acionável. PURA (o script do host é gravado por quem executa o plano).
/// **Onde:** [`TerminalWindows::abrir_script`] e os testes.
pub fn planejar(s: &ScriptTerminal, amb: &Ambiente) -> Result<Plano, String> {
    if let Some(bash) = &amb.git_bash {
        let args: Vec<String> = match &s.entrega {
            Entrega::Arquivo { caminho, .. } => {
                vec!["-l".into(), caminho.to_string_lossy().into_owned()]
            }
            Entrega::Inline(c) => vec!["-lc".into(), c.clone()],
        };
        let lancar = lancar(amb, bash, &args, "Schematize")?;
        return Ok(Plano { escrever: vec![], lancar });
    }
    let (caminho, corpo) = match &s.entrega {
        Entrega::Arquivo { caminho, corpo, .. } => (caminho, corpo),
        Entrega::Inline(_) => return Err(SEM_GIT_BASH.to_string()),
    };
    let comando = traduzir_exec(corpo).ok_or_else(|| SEM_GIT_BASH.to_string())?;
    let ps1 = caminho.with_extension("ps1");
    let args: Vec<String> =
        ["-NoExit", "-ExecutionPolicy", "Bypass", "-File", ps1.to_string_lossy().as_ref()]
            .iter()
            .map(|x| x.to_string())
            .collect();
    let lancar = lancar(amb, Path::new("powershell.exe"), &args, "Schematize")?;
    Ok(Plano { escrever: vec![(ps1, corpo_ps1(&comando))], lancar })
}

/// Mensagem quando não há como rodar um script bash.
pub const SEM_GIT_BASH: &str = "preciso do Git for Windows (bash) para abrir este terminal. \
Instale com: winget install --id Git.Git -e   (depois abra um novo terminal).";

/// O terminal do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalWindows;

impl Terminal for TerminalWindows {
    /// **O quê:** grava o script (se houver), planeja e inicia o terminal SEM esperar.
    /// **Onde:** abrir terminal no projeto / vps / agente.
    fn abrir_script(&self, s: &ScriptTerminal) -> Result<(), String> {
        let plano = planejar(s, &Ambiente::detectar())?;
        if let Entrega::Arquivo { caminho, corpo, modo } = &s.entrega {
            if plano.escrever.is_empty() {
                gravar_script(caminho, corpo, *modo)?;
            }
        }
        for (p, c) in &plano.escrever {
            if let Some(pai) = p.parent() {
                std::fs::create_dir_all(pai)
                    .map_err(|e| format!("não consegui criar {}: {e}", pai.display()))?;
            }
            std::fs::write(p, c)
                .map_err(|e| format!("não consegui gravar {}: {e}", p.display()))?;
        }
        iniciar(&plano.lancar)
    }
}

/// **O quê:** inicia o processo do lançamento e o deixa solto. **Onde:** `abrir_script`.
fn iniciar(l: &Lancamento) -> Result<(), String> {
    let mut c = std::process::Command::new(&l.prog);
    if let Some(b) = &l.bruto {
        raw(&mut c, b);
    } else {
        c.args(&l.args);
    }
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("abrir terminal `{}`: {e}", l.prog.display()))
}

/// Anexa a linha crua ao `Command` (só existe no Windows).
#[cfg(windows)]
fn raw(c: &mut std::process::Command, linha: &str) {
    use std::os::windows::process::CommandExt;
    c.raw_arg(linha);
}

/// Fora do Windows não há `raw_arg`; o caminho nunca é percorrido (o plano é só testado).
#[cfg(not(windows))]
fn raw(c: &mut std::process::Command, linha: &str) {
    c.arg(linha);
}

#[cfg(test)]
mod testes {
    use super::*;
    use schematize_plataforma::seams::terminal::Emuladores;

    fn arquivo(corpo: &str) -> ScriptTerminal {
        ScriptTerminal {
            entrega: Entrega::Arquivo {
                caminho: PathBuf::from("C:\\u\\run\\term-1.sh"),
                corpo: corpo.into(),
                modo: 0o700,
            },
            emuladores: Emuladores::Casa,
        }
    }

    fn amb(wt: bool, bash: bool) -> Ambiente {
        Ambiente {
            wt: wt.then(|| PathBuf::from("C:\\wt\\wt.exe")),
            git_bash: bash.then(|| PathBuf::from("C:\\Program Files\\Git\\bin\\bash.exe")),
        }
    }

    /// O quê: o Git Bash é achado nos locais padrão e o do WSL é recusado. Onde: `achar_git_bash`.
    #[test]
    fn git_bash_e_wsl() {
        let get = |k: &str| match k {
            "ProgramFiles" => Some("C:\\Program Files".to_string()),
            _ => None,
        };
        let existe = |p: &Path| p.to_string_lossy().contains("Git");
        let b = achar_git_bash(&get, &existe, None).expect("achou");
        assert!(b.ends_with("bash.exe") && b.to_string_lossy().contains("Git"));
        let so_wsl = |p: &Path| p.to_string_lossy().contains("System32");
        let git = PathBuf::from("C:\\Windows\\System32\\cmd\\git.exe");
        assert!(achar_git_bash(&|_| None, &so_wsl, Some(&git)).is_none());
        assert!(eh_bash_do_wsl(Path::new("C:\\Windows\\System32\\bash.exe")));
        assert!(eh_bash_do_wsl(Path::new("c:/windows/sysnative/bash.exe")));
        assert!(!eh_bash_do_wsl(Path::new("C:\\Program Files\\Git\\bin\\bash.exe")));
    }

    /// O quê: a partir do `git.exe` do PATH acha o bash irmão. Onde: `achar_git_bash`.
    #[test]
    fn bash_pelo_git_do_path() {
        let git = PathBuf::from("D:/Dev/Git/cmd/git.exe");
        let existe = |p: &Path| p == Path::new("D:/Dev/Git/bin/bash.exe");
        let b = achar_git_bash(&|_| None, &existe, Some(&git)).expect("achou");
        assert_eq!(b, Path::new("D:/Dev/Git").join("bin").join("bash.exe"));
    }

    /// O quê: tradução só do script trivial `exec`. Onde: `traduzir_exec`.
    #[test]
    fn traduz_somente_exec() {
        assert_eq!(traduzir_exec("#!/bin/sh\nexec ssh -t host\n"), Some("ssh -t host".into()));
        assert_eq!(traduzir_exec("exec x"), Some("x".into()));
        assert_eq!(traduzir_exec("#!/bin/sh\ncd /x\nexec y\n"), None);
        assert_eq!(traduzir_exec("#!/bin/sh\necho oi\n"), None);
        assert_eq!(traduzir_exec("exec "), None);
        assert_eq!(traduzir_exec(""), None);
    }

    /// O quê: `.ps1` com BOM e `&` quando o comando começa por aspas. Onde: `corpo_ps1`.
    #[test]
    fn ps1_corpo() {
        assert_eq!(corpo_ps1("ssh host"), "\u{feff}ssh host\r\n");
        assert!(corpo_ps1("\"C:\\a b\\x.exe\" y").contains("& \"C:\\a b\\x.exe\" y"));
    }

    /// O quê: com Git Bash + wt a aba roda `bash -l script`. Onde: `planejar`.
    #[test]
    fn plano_wt_com_bash() {
        let p = planejar(&arquivo("#!/bin/bash\necho\n"), &amb(true, true)).expect("plano");
        assert_eq!(p.lancar.prog, PathBuf::from("C:\\wt\\wt.exe"));
        assert_eq!(p.lancar.args[0], "new-tab");
        assert!(p.lancar.args.iter().any(|a| a.ends_with("bash.exe")));
        assert!(p.lancar.args.windows(2).any(|w| w[0] == "-l" && w[1].ends_with("term-1.sh")));
        assert!(p.escrever.is_empty() && p.lancar.bruto.is_none());
    }

    /// O quê: sem wt, usa `cmd /c start "" "bash" -l "script"` com quoting cru. Onde: `planejar`.
    #[test]
    fn plano_cmd_start() {
        let p = planejar(&arquivo("x"), &amb(false, true)).expect("plano");
        assert_eq!(p.lancar.prog, PathBuf::from("cmd.exe"));
        let b = p.lancar.bruto.expect("cru");
        assert!(
            b.starts_with("/c start \"\" \"C:\\Program Files\\Git\\bin\\bash.exe\" -l "),
            "{b}"
        );
        assert!(b.ends_with("term-1.sh"));
    }

    /// O quê: Inline vira `bash -lc <cmd>`. Onde: `planejar`.
    #[test]
    fn plano_inline() {
        let s = ScriptTerminal {
            entrega: Entrega::Inline("ls -la".into()),
            emuladores: Emuladores::Gui,
        };
        let p = planejar(&s, &amb(true, true)).expect("plano");
        assert!(p.lancar.args.windows(2).any(|w| w == ["-lc", "ls -la"]));
    }

    /// O quê: sem Git Bash, `exec` vira .ps1 e roda no powershell -NoExit. Onde: `planejar`.
    #[test]
    fn plano_powershell_sem_bash() {
        let p =
            planejar(&arquivo("#!/bin/sh\nexec ssh host\n"), &amb(false, false)).expect("plano");
        assert_eq!(p.escrever.len(), 1);
        assert!(p.escrever[0].0.to_string_lossy().ends_with("term-1.ps1"));
        let b = p.lancar.bruto.expect("cru");
        assert!(b.contains("powershell.exe -NoExit -ExecutionPolicy Bypass -File"), "{b}");
        let pw =
            planejar(&arquivo("#!/bin/sh\nexec ssh host\n"), &amb(true, false)).expect("plano");
        assert!(pw.lancar.args.iter().any(|a| a == "-NoExit"));
    }

    /// O quê: script bash arbitrário sem Git Bash é erro acionável (não culpa o usuário).
    /// Onde: `planejar`.
    #[test]
    fn sem_bash_script_complexo_e_erro() {
        let e =
            planejar(&arquivo("#!/bin/bash\ncd x\nexec y\n"), &amb(true, false)).expect_err("erro");
        assert!(e.contains("winget install --id Git.Git"));
        let s =
            ScriptTerminal { entrega: Entrega::Inline("ls".into()), emuladores: Emuladores::Gui };
        assert!(planejar(&s, &amb(true, false)).is_err());
    }

    /// O quê: caminho com `%`/`&` no cmd é recusado (injeção). Onde: `planejar`.
    #[test]
    fn cmd_recusa_caminho_hostil() {
        let mut s = arquivo("x");
        if let Entrega::Arquivo { caminho, .. } = &mut s.entrega {
            *caminho = PathBuf::from("C:\\u\\%USERNAME%\\t.sh");
        }
        assert!(planejar(&s, &amb(false, true)).is_err());
        // com wt o argv é passado sem shell: o mesmo caminho é aceito.
        assert!(planejar(&s, &amb(true, true)).is_ok());
    }
}
