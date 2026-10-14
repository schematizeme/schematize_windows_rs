//! **pathsetup** — a costura [`PathSetup`] do Windows: o `PATH` do USUÁRIO no registro.
//!
//! **O quê:** garante que um diretório esteja no `PATH` de usuário (`HKCU\Environment\Path`),
//! SEM admin e de forma idempotente. Lê o valor CRU (sem expandir `%VAR%`), acrescenta só se
//! faltar, regrava como `REG_EXPAND_SZ` (preserva `%USERPROFILE%` e afins) e avisa o sistema
//! (`WM_SETTINGCHANGE`) para os próximos terminais enxergarem. A decisão ("já está?") e o texto
//! novo são PUROS e testados no Linux; ler/gravar o registro é o script PowerShell.
//!
//! **Onde:** `Plataforma::windows().path`: fim de toda instalação (`ensure_path_setup`) e a
//! dica de PATH do market.

use crate::exec::rodar_ps;
use crate::ps::literal;
use schematize_plataforma::seams::pathsetup::{Orientacao, TipoShell};
use schematize_plataforma::seams::PathSetup;
use std::path::Path;

/// **O quê:** forma canônica de UMA entrada de PATH para comparar: sem aspas, barras `\`, sem
/// barra final, `%USERPROFILE%` expandido (é a única variável que o nosso diretório usa) e
/// minúscula (o Windows ignora caixa). PURA. **Onde:** [`contem`].
pub fn normalizar(entrada: &str, home: &str) -> String {
    let mut s = entrada.trim().trim_matches('"').replace('/', "\\");
    let alvo = "%userprofile%";
    while let Some(i) = s.to_lowercase().find(alvo) {
        s.replace_range(i..i + alvo.len(), home);
    }
    s.trim_end_matches('\\').to_lowercase()
}

/// **O quê:** `dir` já é uma das entradas de `path` (separador `;`)? PURA.
/// **Onde:** [`plano_path`] e `orientacao`.
pub fn contem(path: &str, dir: &str, home: &str) -> bool {
    let alvo = normalizar(dir, home);
    !alvo.is_empty()
        && path.split(';').filter(|p| !p.trim().is_empty()).any(|p| normalizar(p, home) == alvo)
}

/// **O quê:** o PATH novo = `atual` + `;` + `dir`, ou `None` se `dir` já estiver (idempotente).
/// Rejeita `dir` com `;`, aspas ou controle (corromperia o PATH). PURA.
///
/// **Onde:** [`PathSetupWindows::garantir_no_path`].
pub fn plano_path(atual: &str, dir: &str, home: &str) -> Result<Option<String>, String> {
    if dir.trim().is_empty() || dir.contains([';', '"', '\0']) || dir.chars().any(char::is_control)
    {
        return Err(format!("diretório inválido para o PATH: {dir:?}"));
    }
    if contem(atual, dir, home) {
        return Ok(None);
    }
    let base = atual.trim_end_matches(';');
    Ok(Some(if base.is_empty() { dir.to_string() } else { format!("{base};{dir}") }))
}

/// Script que imprime o `Path` de usuário SEM expandir variáveis (vazio se não existir).
pub const SCRIPT_LER_PATH: &str = "$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment'); \
if($k){[Console]::Out.Write($k.GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames))}";

/// **O quê:** o script que grava `novo` em `HKCU\Environment\Path` (`REG_EXPAND_SZ`) e difunde
/// `WM_SETTINGCHANGE`. Falha só no broadcast vira um aviso no stdout (o PATH já foi gravado).
/// PURA. **Onde:** [`PathSetupWindows::garantir_no_path`].
pub fn script_gravar_path(novo: &str) -> String {
    format!(
        "$k=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment'); \
         $k.SetValue('Path', {}, [Microsoft.Win32.RegistryValueKind]::ExpandString); $k.Close(); \
         try {{ Add-Type -Namespace Schematize -Name Env -MemberDefinition \
         '[DllImport(\"user32.dll\", SetLastError=true, CharSet=CharSet.Auto)] public static extern \
         IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, \
         uint uTimeout, out UIntPtr lpdwResult);'; $r=[UIntPtr]::Zero; \
         [void][Schematize.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, \
         'Environment', 2, 5000, [ref]$r) }} catch {{ [Console]::Out.Write('AVISO: PATH gravado, mas não \
         consegui avisar o sistema: ' + $_.Exception.Message) }}",
        literal(novo)
    )
}

/// **O quê:** a orientação para a SESSÃO ATUAL, ou `None` se `dir` já está no `PATH` do processo.
/// PURA. **Onde:** [`PathSetupWindows::orientacao`] e os testes.
pub fn orientacao_em(path_do_processo: &str, dir: &Path, home: &str) -> Option<Orientacao> {
    let d = dir.to_string_lossy();
    if contem(path_do_processo, &d, home) {
        return None;
    }
    Some(Orientacao {
        shell: TipoShell::Outro,
        arquivo_rc: None,
        linha: format!("$env:Path += {}", literal(&format!(";{d}"))),
    })
}

/// O ajuste de `PATH` do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct PathSetupWindows;

impl PathSetup for PathSetupWindows {
    /// **O quê:** acrescenta `dir` ao PATH de usuário se faltar. **Onde:** fim da instalação.
    fn garantir_no_path(&self, dir: &Path) -> Result<(), String> {
        let home = schematize_plataforma::home::home_do_ambiente()
            .ok_or_else(|| "USERPROFILE não definido (ou relativo)".to_string())?;
        let atual = rodar_ps(SCRIPT_LER_PATH)?;
        let dir_s = dir.to_str().ok_or_else(|| format!("caminho não-UTF-8: {}", dir.display()))?;
        match plano_path(&atual, dir_s, &home.to_string_lossy())? {
            None => Ok(()),
            Some(novo) => {
                let saida = rodar_ps(&script_gravar_path(&novo))?;
                if !saida.trim().is_empty() {
                    eprintln!("aviso: {}", saida.trim());
                }
                Ok(())
            }
        }
    }

    /// **O quê:** a linha PowerShell que põe `dir` no PATH desta sessão, se ainda não estiver.
    /// **Onde:** a dica de PATH depois da instalação.
    fn orientacao(&self, dir: &Path) -> Option<Orientacao> {
        let path = std::env::var("PATH").unwrap_or_default();
        let home = schematize_plataforma::home::home_do_ambiente()?;
        orientacao_em(&path, dir, &home.to_string_lossy())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const H: &str = "C:\\Users\\ana";

    /// O quê: comparação ignora caixa, barra final, `/`, aspas e `%USERPROFILE%`.
    /// Onde: `normalizar` / `contem`.
    #[test]
    fn contem_variantes() {
        let p = "C:\\Windows;%USERPROFILE%\\.cargo\\bin\\;\"D:\\x y\"";
        assert!(contem(p, "C:\\Users\\ANA\\.cargo\\bin", H));
        assert!(contem(p, "c:/users/ana/.cargo/bin/", H));
        assert!(contem(p, "D:\\x y", H));
        assert!(!contem(p, "C:\\Users\\ana\\.cargo", H));
        assert!(!contem("", "C:\\a", H));
        assert!(!contem(p, "", H));
    }

    /// O quê: idempotência — segunda aplicação não muda nada. Onde: `plano_path`.
    #[test]
    fn plano_idempotente() {
        let dir = "C:\\Users\\ana\\.cargo\\bin";
        let novo = plano_path("C:\\Windows;", dir, H).expect("ok").expect("muda");
        assert_eq!(novo, format!("C:\\Windows;{dir}"));
        assert_eq!(plano_path(&novo, dir, H), Ok(None));
        assert_eq!(plano_path("", dir, H), Ok(Some(dir.to_string())));
    }

    /// O quê: diretório hostil não corrompe o PATH. Onde: `plano_path`.
    #[test]
    fn plano_rejeita_hostil() {
        for ruim in ["", "  ", "C:\\a;C:\\b", "C:\\a\"b", "C:\\a\nb", "C:\\a\0"] {
            assert!(plano_path("C:\\x", ruim, H).is_err(), "{ruim:?}");
        }
    }

    /// O quê: o script de gravação escapa o valor e usa ExpandString. Onde: `script_gravar_path`.
    #[test]
    fn script_gravar() {
        let s = script_gravar_path("C:\\a'b");
        assert!(s.contains("'C:\\a''b'"));
        assert!(s.contains("ExpandString"));
        assert!(s.contains("WM_SETTINGCHANGE") || s.contains("0x1A"));
    }

    /// O quê: orientação some se já está no PATH; senão traz a linha PowerShell. Onde: `orientacao_em`.
    #[test]
    fn orientacao() {
        let dir = Path::new("C:\\Users\\ana\\.cargo\\bin");
        assert!(orientacao_em("C:\\Users\\ana\\.cargo\\bin;C:\\x", dir, H).is_none());
        let o = orientacao_em("C:\\x", dir, H).expect("deve orientar");
        assert_eq!(o.shell, TipoShell::Outro);
        assert!(o.arquivo_rc.is_none());
        assert_eq!(o.linha, "$env:Path += ';C:\\Users\\ana\\.cargo\\bin'");
    }
}
