//! **exec** — execução real de processos no Windows (`cfg(windows)`) e resolução de programa.
//!
//! **O quê:** achar um executável via `PATH` + `PATHEXT` (a regra do Windows: `.exe`, `.cmd`…),
//! e rodar `powershell.exe` com um script. A resolução é PURA (recebe o `PATH`, o `PATHEXT` e um
//! predicado de existência) e roda no Linux nos testes; só o spawn é `cfg(windows)`.
//!
//! **Onde:** `shell` (existe/executar), `rede`, `atalhos`, `pathsetup`, `processos`.

use std::path::{Path, PathBuf};

/// `PATHEXT` padrão do Windows quando a variável não existe.
pub const PATHEXT_PADRAO: &str = ".COM;.EXE;.BAT;.CMD";

/// **O quê:** as extensões de `PATHEXT` (minúsculas, com ponto), na ordem; vazio cai no padrão.
///
/// **Onde:** [`resolver_em`].
pub fn extensoes(pathext: &str) -> Vec<String> {
    let src = if pathext.trim().is_empty() { PATHEXT_PADRAO } else { pathext };
    src.split(';').map(|e| e.trim().to_lowercase()).filter(|e| e.starts_with('.')).collect()
}

/// **O quê:** acha `bin` nos diretórios de `path` (separador `;`) tentando cada extensão de
/// `pathext`; `existe` diz se o arquivo existe. `bin` com separador de caminho NÃO é procurado
/// (devolve `None`): quem tem caminho já sabe onde está. PURA.
///
/// **Onde:** `ShellWindows::existe`/`executar` e [`resolver_no_ambiente`].
pub fn resolver_em(
    bin: &str,
    path: &str,
    pathext: &str,
    existe: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if bin.is_empty() || bin.contains(['/', '\\', ':', '\0']) {
        return None;
    }
    let exts = extensoes(pathext);
    let tem_ext = Path::new(bin)
        .extension()
        .is_some_and(|e| exts.contains(&format!(".{}", e.to_string_lossy().to_lowercase())));
    for dir in path.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let dir = dir.trim_matches('"');
        let base = Path::new(dir).join(bin);
        if tem_ext && existe(&base) {
            return Some(base);
        }
        for e in &exts {
            let cand = Path::new(dir).join(format!("{bin}{e}"));
            if existe(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

/// **O quê:** [`resolver_em`] com o `PATH`/`PATHEXT` do processo e o disco real.
///
/// **Onde:** `ShellWindows`, `RedeWindows`, `TerminalWindows`.
pub fn resolver_no_ambiente(bin: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let ext = std::env::var("PATHEXT").unwrap_or_default();
    resolver_em(bin, &path, &ext, &|p| p.is_file())
}

/// **O quê:** como [`resolver_no_ambiente`], mas aceita REPARSE POINTS (os aliases de execução de
/// app, como o `wt.exe` em `WindowsApps`, não passam em `is_file`).
///
/// **Onde:** detecção do Windows Terminal.
pub fn resolver_alias_no_ambiente(bin: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let ext = std::env::var("PATHEXT").unwrap_or_default();
    resolver_em(bin, &path, &ext, &|p| std::fs::symlink_metadata(p).is_ok())
}

/// **O quê:** o `powershell.exe` (Windows PowerShell 5.1, presente em todo Windows 10+) pelo
/// caminho absoluto em `SystemRoot`, ou o nome simples se a variável faltar.
///
/// **Onde:** toda execução de script.
pub fn powershell_exe() -> PathBuf {
    match std::env::var_os("SystemRoot") {
        Some(sr) => PathBuf::from(sr).join("System32/WindowsPowerShell/v1.0/powershell.exe"),
        None => PathBuf::from("powershell.exe"),
    }
}

/// **O quê:** roda um script INTERNO (`-EncodedCommand`) e devolve o stdout (UTF-8). `Err` com o
/// stderr se saiu não-zero ou se o PowerShell não iniciou.
///
/// **Onde:** atalhos, PATH, processos e sondas — tudo que precisa da saída do script.
pub fn rodar_ps(script: &str) -> Result<String, String> {
    let out = std::process::Command::new(powershell_exe())
        .args(crate::ps::argv_encoded(script))
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("não consegui executar o PowerShell: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "PowerShell falhou ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn existe_em<'a>(arqs: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
        move |p| arqs.iter().any(|a| Path::new(a) == p)
    }

    /// O quê: `.exe` é achado sem digitar a extensão, na ordem do PATH. Onde: `resolver_em`.
    #[test]
    fn acha_exe_sem_extensao() {
        let f = existe_em(&["C:/b/git.exe"]);
        let r = resolver_em("git", "C:/a;C:/b", ".COM;.EXE", &f);
        assert_eq!(r, Some(Path::new("C:/b").join("git.exe")));
    }

    /// O quê: `.cmd` (shim do npm) é achado; extensão explícita também. Onde: `resolver_em`.
    #[test]
    fn acha_cmd_e_extensao_explicita() {
        let f = existe_em(&["C:/n/codex.cmd", "C:/n/x.exe"]);
        assert!(resolver_em("codex", "C:/n", ".EXE;.CMD", &f).is_some());
        assert!(resolver_em("x.exe", "C:/n", ".EXE;.CMD", &f).is_some());
        assert!(resolver_em("nada", "C:/n", ".EXE;.CMD", &f).is_none());
    }

    /// O quê: nome com caminho/NUL/vazio nunca é procurado (fail-closed). Onde: `resolver_em`.
    #[test]
    fn nome_hostil_nao_resolve() {
        let f = |_: &Path| true;
        for b in ["", "a\\b", "a/b", "C:x", "a\0b"] {
            assert!(resolver_em(b, "C:\\a", ".EXE", &f).is_none(), "{b:?}");
        }
    }

    /// O quê: PATH com aspas e entradas vazias é tolerado; PATHEXT vazio cai no padrão.
    /// Onde: `resolver_em` / `extensoes`.
    #[test]
    fn path_com_aspas_e_pathext_vazio() {
        let f = existe_em(&["C:/p q/t.exe"]);
        assert!(resolver_em("t", ";\"C:/p q\";", "", &f).is_some());
        assert_eq!(extensoes(""), vec![".com", ".exe", ".bat", ".cmd"]);
    }
}
