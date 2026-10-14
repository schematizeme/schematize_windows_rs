//! **abridor** — a costura [`Abridor`] do Windows: `explorer.exe`.
//!
//! **O quê:** abre URL, arquivo ou pasta no app padrão chamando `explorer.exe <alvo>` direto
//! (sem `cmd /c start`): o `explorer` recebe o alvo como UM argumento e nenhum shell o
//! reinterpreta, então `&`, `^`, `%VAR%` e aspas no alvo não viram comando. O `explorer` sai
//! com código 1 mesmo quando abriu — por isso só o ERRO DE SPAWN conta.
//!
//! **Onde:** `Plataforma::windows().abridor`: "abrir no navegador", "abrir pasta" da GUI.

use schematize_plataforma::seams::abridor::validar_alvo;
use schematize_plataforma::seams::Abridor;
use std::path::PathBuf;

/// **O quê:** valida o alvo e devolve o argumento do explorer. Rejeita o que o `validar_alvo` da
/// lib rejeita (vazio, NUL, `-opção`) e também `/opção` (o explorer lê `/select,…`, `/e,…`
/// como comando). PURA. **Onde:** [`AbridorWindows::abrir`].
pub fn plano(alvo: &str) -> Result<String, String> {
    validar_alvo(alvo)?;
    if alvo.starts_with('/') {
        return Err(format!("alvo inválido (começa com `/`, seria lido como opção): {alvo}"));
    }
    if alvo.chars().any(char::is_control) {
        return Err("alvo inválido: contém caractere de controle.".to_string());
    }
    Ok(alvo.to_string())
}

/// **O quê:** o `explorer.exe` em `SystemRoot`, ou o nome simples. **Onde:** abrir.
fn explorer() -> PathBuf {
    match std::env::var_os("SystemRoot") {
        Some(sr) => PathBuf::from(sr).join("explorer.exe"),
        None => PathBuf::from("explorer.exe"),
    }
}

/// O abridor do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct AbridorWindows;

impl Abridor for AbridorWindows {
    /// **O quê:** abre `alvo` no app padrão. **Onde:** GUI e CLI (`abrir`).
    fn abrir(&self, alvo: &str) -> Result<(), String> {
        let arg = plano(alvo)?;
        std::process::Command::new(explorer())
            .arg(&arg)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("não consegui abrir com o explorer ({e}). Abra à mão: {alvo}"))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: alvos legítimos passam intactos (URL com `&`, caminho com espaço). Onde: `plano`.
    #[test]
    fn alvos_validos() {
        assert_eq!(plano("https://x/y?a=1&b=2"), Ok("https://x/y?a=1&b=2".into()));
        assert_eq!(plano("C:\\Program Files\\x"), Ok("C:\\Program Files\\x".into()));
    }

    /// O quê: opções, NUL, controle e vazio são recusados. Onde: `plano`.
    #[test]
    fn alvos_hostis() {
        for ruim in ["", "  ", "-x", "/select,C:\\x", "a\0b", "a\nb"] {
            assert!(plano(ruim).is_err(), "{ruim:?}");
        }
    }
}
