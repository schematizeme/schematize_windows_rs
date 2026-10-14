//! **dirs** — diretórios de binários do Windows que o `PATH` de um processo pode não ter.
//!
//! **O quê:** (1) onde o OpenSSH do Windows mora (`System32\OpenSSH`, `SysNative\OpenSSH`,
//! `%ProgramFiles%\OpenSSH`, `%LOCALAPPDATA%\OpenSSH`) — vem com o Windows 10 1803+ e NÃO está no
//! `PATH` de todo perfil; (2) os extras de `PATH` do filho do agente
//! (`~\.local\bin`, `~\.cargo\bin`, `~\.claude\local`) para `overdev_casa::agentrun::montar_path(';', …)`.
//! Eram o bloco `cfg(windows)` de `fallback_bin_dirs` que saiu dos cores.
//!
//! **Onde:** PURAS e prontas, mas os cores ainda não têm uma costura que as receba (ver "lacunas
//! de API" no relatório da F5): ficam aqui, testadas, para o dia em que tiverem.

use std::path::{Path, PathBuf};

/// **O quê:** os diretórios do OpenSSH do Windows, na ordem de preferência. `get` lê o ambiente.
/// PURA. **Onde:** quem resolve `ssh` (vps do hub).
pub fn openssh_dirs(get: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(sr) = get("SystemRoot").filter(|s| !s.trim().is_empty()) {
        let sr = PathBuf::from(sr);
        v.push(sr.join("System32").join("OpenSSH"));
        v.push(sr.join("SysNative").join("OpenSSH"));
    }
    for k in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(d) = get(k).filter(|s| !s.trim().is_empty()) {
            v.push(PathBuf::from(d).join("OpenSSH"));
        }
    }
    v
}

/// **O quê:** os extras (strings, com `\`) que o filho do agente precisa no `PATH` do Windows.
/// PURA. **Onde:** `montar_path(';', &base, &extras_path_windows(home))`.
pub fn extras_path_windows(home: &Path) -> Vec<String> {
    [".local\\bin", ".cargo\\bin", ".claude\\local"]
        .iter()
        .map(|rel| format!("{}\\{rel}", home.to_string_lossy().trim_end_matches(['\\', '/'])))
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: os dirs do OpenSSH saem na ordem e variável ausente/vazia é pulada.
    /// Onde: `openssh_dirs`.
    #[test]
    fn openssh() {
        let get = |k: &str| match k {
            "SystemRoot" => Some("C:/Windows".to_string()),
            "ProgramFiles" => Some("C:/Program Files".to_string()),
            "LOCALAPPDATA" => Some("  ".to_string()),
            _ => None,
        };
        let v = openssh_dirs(&get);
        assert_eq!(v.len(), 3);
        assert!(v[0].ends_with(Path::new("System32").join("OpenSSH")));
        assert!(v[1].ends_with(Path::new("SysNative").join("OpenSSH")));
        assert!(v[2].starts_with("C:/Program Files"));
        assert!(openssh_dirs(&|_| None).is_empty());
    }

    /// O quê: extras de PATH do agente, sem barra duplicada. Onde: `extras_path_windows`.
    #[test]
    fn extras() {
        let e = extras_path_windows(Path::new("C:\\Users\\ana\\"));
        assert_eq!(e[1], "C:\\Users\\ana\\.cargo\\bin");
        assert_eq!(e.len(), 3);
    }
}
