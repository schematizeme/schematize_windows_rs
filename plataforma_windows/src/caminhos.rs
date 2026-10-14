//! **caminhos** — a costura [`Caminhos`] do Windows.
//!
//! **O quê:** `PATH` separado por `;`; os extras do `PATH` do agente (`~\.local\bin`,
//! `~\.cargo\bin`, `~\.claude\local`); e os diretórios de fallback para achar binário fora do
//! `PATH`: esses mesmos mais o OpenSSH do Windows ([`crate::dirs::openssh_dirs`]), que vem com o
//! Windows 10 1803+ mas não está no `PATH` de todo perfil.
//!
//! **Onde:** `Plataforma::windows().caminhos`: PTY do agente do overdev e `resolve_bin`.

use crate::dirs::{extras_path_windows, openssh_dirs};
use schematize_plataforma::seams::Caminhos;
use std::path::{Path, PathBuf};

/// **O quê:** junta os extras do home com os dirs do OpenSSH, sem repetir. PURA (o ambiente vem
/// de `get`). **Onde:** [`CaminhosWindows::dirs_fallback_bin`] e os testes.
pub fn fallback_com(home: &Path, get: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = extras_path_windows(home).into_iter().map(PathBuf::from).collect();
    for d in openssh_dirs(get) {
        if !v.contains(&d) {
            v.push(d);
        }
    }
    v
}

/// Os caminhos do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct CaminhosWindows;

impl Caminhos for CaminhosWindows {
    /// **O quê:** `;`. **Onde:** montagem do `PATH` do filho.
    fn separador_path(&self) -> char {
        ';'
    }

    /// **O quê:** [`extras_path_windows`] como `PathBuf`. **Onde:** spawn do `claude`.
    fn extras_path_agente(&self, home: &Path) -> Vec<PathBuf> {
        extras_path_windows(home).into_iter().map(PathBuf::from).collect()
    }

    /// **O quê:** [`fallback_com`] com o ambiente real. **Onde:** `resolve_bin`.
    fn dirs_fallback_bin(&self, home: &Path) -> Vec<PathBuf> {
        fallback_com(home, &|k| std::env::var(k).ok())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: separador e extras do agente. Onde: `CaminhosWindows`.
    #[test]
    fn separador_e_extras() {
        let c = CaminhosWindows;
        assert_eq!(c.separador_path(), ';');
        let e = c.extras_path_agente(Path::new("C:\\Users\\ana"));
        assert_eq!(e.len(), 3);
        assert_eq!(e[1], PathBuf::from("C:\\Users\\ana\\.cargo\\bin"));
    }

    /// O quê: o fallback traz extras + OpenSSH, sem duplicar. Onde: `fallback_com`.
    #[test]
    fn fallback_inclui_openssh() {
        let get = |k: &str| (k == "SystemRoot").then(|| "C:/Windows".to_string());
        let v = fallback_com(Path::new("C:\\h"), &get);
        assert_eq!(v.len(), 5);
        assert!(v.iter().any(|p| p.ends_with(Path::new("System32").join("OpenSSH"))));
        assert!(v[0].to_string_lossy().ends_with(".local\\bin"));
    }
}
