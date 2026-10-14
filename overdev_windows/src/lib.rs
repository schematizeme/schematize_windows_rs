//! **overdev_windows** — `HooksWindows`, o impl Windows de [`overdev_casa::hooks::Hooks`].
//!
//! **O quê:** o comando que o `settings.json` do Claude Code guarda para os hooks `check`/`guard`
//! do overdev, escrito para funcionar quando o Claude Code roda o hook pelo **Git Bash**, pelo
//! **cmd** ou pelo **PowerShell**.
//!
//! ## A forma única: `C:/…/schematize-overdev.exe check`
//!
//! Caminho ABSOLUTO do `.exe`, com `/`, **sem aspas** (ou o nome puro `schematize-overdev.exe`,
//! resolvido pelo `PATH`, se o caminho tem espaço ou metacaractere). Por que NÃO a forma entre
//! aspas: `"C:\x\schematize-overdev.exe" check` é uma EXPRESSÃO de string no PowerShell (erro de
//! parse); o conserto `& "…"` é erro de sintaxe no bash e separador de comandos no cmd; embrulhar
//! em `powershell -Command` perde o exit code 2 (o Claude Code usa 2 para BLOQUEAR) e custa
//! ~300 ms por chamada. Sem aspas e com `/`, o MESMO texto roda igual nos três: o bash não trata
//! `/` nem `:` (e `\` seria escape); cmd e PowerShell aceitam `C:/…/x.exe` como caminho.
//!
//! ## Reconhecimento: `eh_nosso` por parse, não por substring
//!
//! O grupo é identificado pelo basename do executável (`schematize-overdev`, com ou sem `.exe`,
//! com ou sem aspas ou `& `) + o subcomando. Reconhece a forma atual E as antigas (sem `.exe`,
//! entre aspas com `\`, o hub `schematize overdev check`, o one-liner Unix), para `enable`
//! migrar sem duplicar e `disable` não deixar grupo órfão.
//!
//! **Onde:** o `main` de `schematize-overdev.exe` e `schematize-overdev-gui.exe` (`crate bins`).

use overdev_casa::hooks::{GrupoHook, Hooks};
use std::path::Path;

/// Nome do binário (sem extensão).
pub const NOME_BIN: &str = "schematize-overdev";

/// Nome do arquivo executável.
pub const NOME_EXE: &str = "schematize-overdev.exe";

/// Os hooks do Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HooksWindows {
    /// Prefixo do comando: caminho absoluto do `.exe` (`C:/Users/ana/.cargo/bin/schematize-overdev.exe`)
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

/// **O quê:** o prefixo do comando para o diretório `dir` do binário: `<dir>/schematize-overdev.exe`
/// com `/`; cai no nome puro se o caminho não for seguro sem aspas. PURA. **Onde:**
/// [`HooksWindows::com_dir`].
pub fn prefixo_de(dir: Option<&Path>) -> String {
    let Some(d) = dir else { return NOME_EXE.to_string() };
    let p = format!("{}/{NOME_EXE}", d.to_string_lossy().replace('\\', "/").trim_end_matches('/'));
    if seguro_sem_aspas(&p) {
        p
    } else {
        NOME_EXE.to_string()
    }
}

/// **O quê:** o subcomando de `grupo` (`check`, `guard`, `vps guard`). PURA.
/// **Onde:** [`eh_comando_do_grupo`].
pub fn sub_do_grupo(grupo: GrupoHook) -> &'static str {
    match grupo {
        GrupoHook::Check => "check",
        GrupoHook::Guard => "guard",
        GrupoHook::VpsGuard => "vps guard",
    }
}

/// **O quê:** separa o primeiro token de `cmd` (entre aspas simples/duplas ou até o espaço) do
/// resto, pulando um `& ` inicial (forma PowerShell). PURA. **Onde:** [`eh_comando_do_grupo`].
fn primeiro_token(cmd: &str) -> (String, &str) {
    let c = cmd.trim_start();
    let c = c.strip_prefix('&').map_or(c, str::trim_start);
    match c.chars().next() {
        Some(q @ ('"' | '\'')) => match c[1..].find(q) {
            Some(i) => (c[1..1 + i].to_string(), &c[i + 2..]),
            None => (c[1..].to_string(), ""),
        },
        _ => match c.find(char::is_whitespace) {
            Some(i) => (c[..i].to_string(), &c[i..]),
            None => (c.to_string(), ""),
        },
    }
}

/// **O quê:** `cmd` é o comando do `grupo`? Parse: basename do executável (`schematize-overdev`
/// com/sem `.exe`, qualquer caixa, `/` ou `\`) + resto == subcomando; também o hub
/// (`schematize overdev check`). O one-liner Unix (alheio) é reconhecido pela marca textual
/// antiga, para o `disable` não deixá-lo órfão. PURA. **Onde:** `HooksWindows::eh_nosso`.
pub fn eh_comando_do_grupo(cmd: &str, grupo: GrupoHook) -> bool {
    let sub = sub_do_grupo(grupo);
    let (tok, resto) = primeiro_token(cmd);
    let base = tok.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    let base = base.strip_suffix(".exe").unwrap_or(&base);
    let resto = resto.split_whitespace().collect::<Vec<_>>().join(" ");
    if base == NOME_BIN && resto == sub {
        return true;
    }
    if base == "schematize" && resto == format!("overdev {sub}") {
        return true;
    }
    eh_unix(cmd) && cmd.contains(&format!("overdev {sub}"))
}

/// **O quê:** `cmd` tem cara do one-liner POSIX do Unix (`$HOME`, `command -v`, `exec "$c"`)? PURA.
/// **Onde:** [`eh_comando_do_grupo`] e `HooksWindows::reconhece`.
fn eh_unix(cmd: &str) -> bool {
    cmd.contains("$HOME") || cmd.contains("command -v") || cmd.contains("exec \"")
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
        let dir =
            schematize_plataforma::bin::cargo_bin_dir().filter(|d| d.join(NOME_EXE).is_file());
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
    /// **O quê:** `<prefixo> <sub>` (UMA linha, segura para JSON). Binário ausente: o shell falha
    /// com "não encontrado" (exit != 2 = erro não bloqueante no Claude Code; a sessão segue).
    /// **Onde:** gravação no `settings.json` e "está atualizado?".
    fn comando(&self, sub: &str) -> String {
        format!("{} {sub}", self.prefixo)
    }

    /// **O quê:** formato que esta plataforma sabe julgar? Não é o one-liner POSIX do Unix: esse
    /// é alheio e não é reescrito. **Onde:** `settings::comando_quebrado`.
    fn reconhece(&self, cmd: &str) -> bool {
        !eh_unix(cmd)
    }

    /// **O quê:** [`eh_comando_do_grupo`]. **Onde:** `settings` (enable/disable/diagnóstico/reparo).
    fn eh_nosso(&self, cmd: &str, grupo: GrupoHook) -> bool {
        eh_comando_do_grupo(cmd, grupo)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: forma final do comando. Onde: `comando`.
    #[test]
    fn comando_forma_final() {
        let h = HooksWindows::com_dir(Some(Path::new("C:\\Users\\ana\\.cargo\\bin")));
        assert_eq!(h.comando("check"), "C:/Users/ana/.cargo/bin/schematize-overdev.exe check");
        assert_eq!(
            h.comando("vps guard"),
            "C:/Users/ana/.cargo/bin/schematize-overdev.exe vps guard"
        );
        assert!(!h.comando("check").contains(['\n', '"', '\\', '&']));
    }

    /// O quê: caminho com espaço/metacaractere cai no nome puro. Onde: `prefixo_de`.
    #[test]
    fn caminho_inseguro_usa_o_nome() {
        for ruim in ["C:\\Users\\Ana Maria\\.cargo\\bin", "C:\\a&b\\bin", "C:\\a%x%\\b", "C:\\a'b"]
        {
            let h = HooksWindows::com_dir(Some(Path::new(ruim)));
            assert_eq!(h.comando("check"), "schematize-overdev.exe check", "{ruim}");
        }
        assert_eq!(HooksWindows::com_dir(None).comando("guard"), "schematize-overdev.exe guard");
        assert!(prefixo_de(Some(Path::new("C:\\ünï\\bin"))).starts_with("C:/ünï"));
    }

    /// O quê: `eh_nosso` reconhece a forma atual e as antigas, e só o grupo certo.
    /// Onde: `eh_comando_do_grupo`.
    #[test]
    fn eh_nosso_formas() {
        use GrupoHook::*;
        let h = HooksWindows::com_dir(None);
        for c in [
            "C:/u/.cargo/bin/schematize-overdev.exe check",
            "C:/u/.cargo/bin/schematize-overdev check",
            "\"C:\\Users\\a b\\.cargo\\bin\\schematize-overdev.exe\" check",
            "& 'C:\\x\\Schematize-Overdev.EXE' check",
            "schematize-overdev check",
            "schematize overdev check",
            &overdev_casa::hooks::comando_unix("check"),
        ] {
            assert!(h.eh_nosso(c, Check), "{c}");
            assert!(!h.eh_nosso(c, Guard), "{c}");
        }
        assert!(h.eh_nosso("C:/x/schematize-overdev.exe vps guard", VpsGuard));
        assert!(!h.eh_nosso("C:/x/schematize-overdev.exe guard", VpsGuard));
        for alheio in
            ["echo overdev-check", "C:/x/outro.exe check", "schematize-overdev checkout", ""]
        {
            assert!(!h.eh_nosso(alheio, Check), "{alheio}");
        }
    }

    /// O quê: comando Unix é alheio; o nosso é reconhecido. Onde: `reconhece`.
    #[test]
    fn reconhece_so_o_que_e_nosso() {
        let h = HooksWindows::com_dir(None);
        assert!(h.reconhece("C:/x/.cargo/bin/schematize-overdev.exe check"));
        assert!(!h.reconhece(&overdev_casa::hooks::comando_unix("check")));
    }
}
