//! **overdev_windows** — `HooksWindows`, o impl Windows de [`overdev_casa::hooks::Hooks`].
//!
//! **O quê:** o comando que o `settings.json` do Claude Code guarda para os hooks `check`/`guard`
//! do overdev, escrito para funcionar tanto quando o Claude Code roda o hook pelo **Git Bash**
//! quanto pelo **cmd** ou **PowerShell**: caminho ABSOLUTO do `schematize-overdev` com barras
//! `/`, SEM aspas e SEM `.exe` (o Windows completa a extensão por `PATHEXT`; o Git Bash também).
//!
//! **Por que sem aspas e sem `.exe`:** (1) aspas na frente de um comando quebram o PowerShell
//! (vira expressão de string) e o `\` é escape no bash; `/` funciona nos três; (2) o overdev
//! reconhece seus grupos por SUBSTRING (`overdev check` / `overdev guard`, `MARCAS` em
//! `settings.rs`): `…/schematize-overdev check` contém a marca, `…/schematize-overdev.exe check`
//! NÃO. Sem isso `enable` duplicaria o hook e `disable` o deixaria órfão.
//!
//! **Caminho com espaço ou metacaractere:** não dá para escrever sem aspas — cai no nome puro
//! `schematize-overdev` (resolvido pelo `PATH`, que o `install.ps1` ajusta).
//!
//! **Onde:** o `main` de `schematize-overdev.exe` e `schematize-overdev-gui.exe` (`crate bins`).

use overdev_casa::hooks::Hooks;
use std::path::Path;

/// Nome do binário (sem extensão), também a marca que o overdev procura.
pub const NOME_BIN: &str = "schematize-overdev";

/// Os hooks do Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HooksWindows {
    /// Prefixo do comando: caminho absoluto sem extensão (`C:/Users/ana/.cargo/bin/schematize-overdev`)
    /// ou só o nome (resolvido pelo `PATH`).
    prefixo: String,
}

/// **O quê:** `s` é seguro para ir NU (sem aspas) em bash, cmd e PowerShell? Só letras, dígitos
/// e `_ . - : / + ~ @ ,`. PURA. **Onde:** [`prefixo_de`].
pub fn seguro_sem_aspas(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || "_.-:/+~@,".contains(c)
                || !c.is_ascii() && c.is_alphanumeric()
        })
}

/// **O quê:** o prefixo do comando para o diretório `dir` do binário: `<dir>/schematize-overdev`
/// com `/`; cai no nome puro se o caminho não for seguro sem aspas. PURA. **Onde:**
/// [`HooksWindows::com_dir`].
pub fn prefixo_de(dir: Option<&Path>) -> String {
    let Some(d) = dir else { return NOME_BIN.to_string() };
    let p = format!("{}/{NOME_BIN}", d.to_string_lossy().replace('\\', "/").trim_end_matches('/'));
    if seguro_sem_aspas(&p) {
        p
    } else {
        NOME_BIN.to_string()
    }
}

impl HooksWindows {
    /// **O quê:** hooks apontando para `dir` (onde mora o `schematize-overdev.exe`); `None` = pelo
    /// `PATH`. **Onde:** testes e [`HooksWindows::novo`].
    pub fn com_dir(dir: Option<&Path>) -> Self {
        HooksWindows { prefixo: prefixo_de(dir) }
    }

    /// **O quê:** hooks para a instalação padrão: `%USERPROFILE%\.cargo\bin` se o binário estiver
    /// lá; senão pelo `PATH`. Determinístico (não depende de onde o processo atual foi iniciado),
    /// para `hooks_atualizados` não trocar de resposta. **Onde:** o `main` dos `.exe`.
    pub fn novo() -> Self {
        let dir = schematize_plataforma::bin::cargo_bin_dir()
            .filter(|d| d.join(format!("{NOME_BIN}.exe")).is_file());
        Self::com_dir(dir.as_deref())
    }
}

impl Default for HooksWindows {
    /// **O quê:** [`HooksWindows::novo`]. **Onde:** conveniência.
    fn default() -> Self {
        Self::novo()
    }
}

impl Hooks for HooksWindows {
    /// **O quê:** `<prefixo> <sub>` (UMA linha, segura para JSON). O binário ausente faz o shell
    /// falhar com "não encontrado" — o Claude Code trata exit != 2 como erro não bloqueante, então
    /// a sessão segue. **Onde:** gravação no `settings.json` e "está atualizado?".
    fn comando(&self, sub: &str) -> String {
        format!("{} {sub}", self.prefixo)
    }

    /// **O quê:** formato que esta plataforma sabe julgar? Não é o one-liner POSIX do Unix (esse
    /// tem `$HOME`/`command -v`/`exec "$c"`): comando Unix é alheio e não é reescrito.
    /// **Onde:** `settings::comando_quebrado`.
    fn reconhece(&self, cmd: &str) -> bool {
        !(cmd.contains("$HOME") || cmd.contains("command -v") || cmd.contains("exec \""))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: o comando leva a marca de reconhecimento do overdev. Onde: `comando`.
    #[test]
    fn comando_tem_as_marcas() {
        let h = HooksWindows::com_dir(Some(Path::new("C:\\Users\\ana\\.cargo\\bin")));
        assert_eq!(h.comando("check"), "C:/Users/ana/.cargo/bin/schematize-overdev check");
        assert!(h.comando("check").contains("overdev check"));
        assert!(h.comando("guard").contains("overdev guard"));
        assert!(!h.comando("check").contains(['\n', '"', '\\']));
    }

    /// O quê: caminho com espaço/metacaractere cai no nome puro (ainda com a marca).
    /// Onde: `prefixo_de`.
    #[test]
    fn caminho_inseguro_usa_o_nome() {
        for ruim in ["C:\\Users\\Ana Maria\\.cargo\\bin", "C:\\a&b\\bin", "C:\\a%x%\\b", "C:\\a'b"]
        {
            let h = HooksWindows::com_dir(Some(Path::new(ruim)));
            assert_eq!(h.comando("check"), "schematize-overdev check", "{ruim}");
        }
        assert_eq!(HooksWindows::com_dir(None).comando("guard"), "schematize-overdev guard");
        assert!(prefixo_de(Some(Path::new("C:\\ünï\\bin"))).starts_with("C:/ünï"));
    }

    /// O quê: comando Unix é alheio; o nosso é reconhecido. Onde: `reconhece`.
    #[test]
    fn reconhece_so_o_que_e_nosso() {
        let h = HooksWindows::com_dir(None);
        assert!(h.reconhece("C:/x/.cargo/bin/schematize-overdev check"));
        assert!(h.reconhece("schematize overdev check"));
        assert!(!h.reconhece(&overdev_casa::hooks::comando_unix("check")));
    }
}
