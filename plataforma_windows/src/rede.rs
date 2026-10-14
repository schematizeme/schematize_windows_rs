//! **rede** — a costura [`Rede`] do Windows: `curl.exe` (Windows 10 1803+) ou PowerShell.
//!
//! **O quê:** GET de texto e download em arquivo. Prefere o `curl.exe` do sistema; sem ele cai
//! em `Invoke-WebRequest`. Download grava em `<destino>.part` e renomeia (nunca deixa arquivo
//! pela metade no destino). Com `mostrar` e stderr num terminal, deixa o progresso do cliente
//! na tela. Os construtores de comando são PUROS e testados no Linux.
//!
//! **Onde:** `Plataforma::windows().rede`; as sondas HTTP de `market_windows` reusam os
//! construtores públicos.

use crate::exec::{powershell_exe, resolver_no_ambiente, rodar_ps};
use crate::ps::{argv_encoded, literal};
use schematize_plataforma::seams::rede::caminho_parcial;
use schematize_plataforma::seams::Rede;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// User-Agent: o GitHub responde 403 a requisição sem UA identificável.
pub const UA: &str = "User-Agent: schematize-market";

/// Qual cliente HTTP usar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cliente {
    /// `curl.exe` no caminho dado.
    Curl(PathBuf),
    /// `Invoke-WebRequest` do PowerShell.
    PowerShell,
}

/// **O quê:** escolhe o cliente: o `curl.exe` real (System32 ou PATH) se existir. PURA a partir
/// de `achado`. **Onde:** [`cliente_atual`] e os testes.
pub fn escolher(achado: Option<PathBuf>) -> Cliente {
    match achado {
        Some(p) => Cliente::Curl(p),
        None => Cliente::PowerShell,
    }
}

/// **O quê:** o cliente desta máquina (`curl.exe` do PATH). **Onde:** toda chamada de rede.
pub fn cliente_atual() -> Cliente {
    escolher(resolver_no_ambiente("curl.exe"))
}

/// **O quê:** argv do `curl` para ler texto (falha em HTTP>=400, timeout 20s, segue redirect). PURA.
/// **Onde:** [`texto_com`].
pub fn curl_texto_args(url: &str) -> Vec<String> {
    ["-fsSL", "-m", "20", "-H", UA, url].iter().map(|s| s.to_string()).collect()
}

/// **O quê:** argv do `curl` para baixar em `dest`; `silencioso` desliga o medidor de progresso. PURA.
/// **Onde:** [`baixar_com`].
pub fn curl_baixar_args(url: &str, dest: &str, silencioso: bool) -> Vec<String> {
    let mut v: Vec<String> =
        ["-fL", "--connect-timeout", "20"].iter().map(|s| s.to_string()).collect();
    v.push(if silencioso { "-sS" } else { "-S" }.to_string());
    v.extend(["-H", UA, "-o", dest, url].iter().map(|s| s.to_string()));
    v
}

/// **O quê:** script PowerShell que escreve em stdout o corpo de `url`. PURA.
/// **Onde:** [`texto_com`] sem curl.
pub fn ps_texto(url: &str) -> String {
    format!(
        "$ProgressPreference='SilentlyContinue'; \
         [Console]::Out.Write((Invoke-WebRequest -UseBasicParsing -TimeoutSec 20 -Uri {} \
         -Headers @{{'User-Agent'='schematize-market'}}).Content)",
        literal(url)
    )
}

/// **O quê:** script PowerShell que baixa `url` em `dest`; `progresso` mantém a barra do cmdlet. PURA.
/// **Onde:** [`baixar_com`] sem curl.
pub fn ps_baixar(url: &str, dest: &str, progresso: bool) -> String {
    let pref = if progresso { "" } else { "$ProgressPreference='SilentlyContinue'; " };
    format!(
        "{pref}Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {} \
         -Headers @{{'User-Agent'='schematize-market'}}",
        literal(url),
        literal(dest)
    )
}

/// **O quê:** sonda do `releases/latest` pelo `curl` SEM seguir o redirect: imprime
/// `"<http_code> <redirect_url>"` (mesmo formato do Unix). PURA. **Onde:** `market_windows`.
pub fn curl_sonda_release_args(url: &str) -> Vec<String> {
    ["-sS", "-m", "20", "-H", UA, "-o", "NUL", "-w", "%{http_code} %{redirect_url}", url]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// **O quê:** script PowerShell equivalente (sem curl): um HEAD que SEGUE o redirect e imprime
/// `"302 <url_final>"`; falha HTTP vira `"<codigo> "`. PURA. **Onde:** `market_windows`.
pub fn ps_sonda_release(url: &str) -> String {
    format!(
        "$ProgressPreference='SilentlyContinue'; try {{ \
         $r=Invoke-WebRequest -UseBasicParsing -Method Head -TimeoutSec 20 -Uri {u}; \
         if($r.BaseResponse.ResponseUri){{$x=$r.BaseResponse.ResponseUri.AbsoluteUri}} \
         else{{$x=$r.BaseResponse.RequestMessage.RequestUri.AbsoluteUri}}; \
         [Console]::Out.Write(\"302 $x\") }} catch {{ \
         if($_.Exception.Response){{[Console]::Out.Write(([int]$_.Exception.Response.StatusCode).ToString()+' ')}} \
         else{{exit 3}} }}",
        u = literal(url)
    )
}

/// **O quê:** argv do `curl` que sonda a existência de um asset pedindo só o 1º byte; imprime o
/// código HTTP. PURA. **Onde:** `market_windows`.
pub fn curl_sonda_asset_args(url: &str) -> Vec<String> {
    ["-sSL", "-m", "20", "-H", UA, "-r", "0-0", "-o", "NUL", "-w", "%{http_code}", url]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// **O quê:** script PowerShell equivalente: imprime o código HTTP (0 se nem respondeu). PURA.
/// **Onde:** `market_windows`.
pub fn ps_sonda_asset(url: &str) -> String {
    format!(
        "$ProgressPreference='SilentlyContinue'; try {{ \
         [Console]::Out.Write([int](Invoke-WebRequest -UseBasicParsing -Method Head -TimeoutSec 20 -Uri {u}).StatusCode) }} \
         catch {{ if($_.Exception.Response){{[Console]::Out.Write([int]$_.Exception.Response.StatusCode)}} \
         else{{[Console]::Out.Write(0)}} }}",
        u = literal(url)
    )
}

/// **O quê:** divide `"Nome: valor"` em `(nome, valor)` aparados; `None` se não for um cabeçalho
/// bem formado (sem `:`, nome vazio ou com caractere de controle/espaço). PURA.
/// **Onde:** [`ps_texto_com_cabecalho`].
pub fn dividir_cabecalho(cab: &str) -> Option<(String, String)> {
    let (n, v) = cab.split_once(':')?;
    let (n, v) = (n.trim(), v.trim());
    let nome_ok = !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (nome_ok && !v.chars().any(char::is_control)).then(|| (n.to_string(), v.to_string()))
}

/// **O quê:** script PowerShell de GET com UM cabeçalho extra; `None` se o cabeçalho for inválido.
/// PURA. **Onde:** `market_windows` (consulta de commit pela API do GitHub).
pub fn ps_texto_com_cabecalho(url: &str, cab: &str) -> Option<String> {
    let (n, v) = dividir_cabecalho(cab)?;
    Some(format!(
        "$ProgressPreference='SilentlyContinue'; \
         [Console]::Out.Write((Invoke-WebRequest -UseBasicParsing -TimeoutSec 20 -Uri {} \
         -Headers @{{'User-Agent'='schematize-market'; {}={}}}).Content)",
        literal(url),
        literal(&n),
        literal(&v)
    ))
}

/// **O quê:** argv do `curl` de texto com um cabeçalho extra. PURA. **Onde:** `market_windows`.
pub fn curl_texto_com_cabecalho_args(url: &str, cab: &str) -> Vec<String> {
    ["-fsSL", "-m", "20", "-H", UA, "-H", cab, url].iter().map(|s| s.to_string()).collect()
}

/// **O quê:** roda `curl`/PowerShell e devolve o stdout; `None` se falhou. **Onde:**
/// [`RedeWindows::texto`] e as sondas de `market_windows` (via [`capturar`]).
pub fn capturar(cli: &Cliente, curl_args: &[String], ps_script: &str) -> Result<String, String> {
    match cli {
        Cliente::Curl(p) => {
            let out = Command::new(p)
                .args(curl_args)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("não consegui executar {}: {e}", p.display()))?;
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).into_owned())
            } else {
                Err(format!(
                    "curl falhou ({}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ))
            }
        }
        Cliente::PowerShell => rodar_ps(ps_script),
    }
}

/// **O quê:** GET de texto (aparado) com o cliente dado. **Onde:** [`RedeWindows::texto`].
pub fn texto_com(cli: &Cliente, url: &str) -> Option<String> {
    capturar(cli, &curl_texto_args(url), &ps_texto(url)).ok().map(|s| s.trim().to_string())
}

/// **O quê:** download em `.part` + rename. `herdado` deixa o progresso do cliente no terminal.
/// Em qualquer falha remove o `.part` (e diz se não conseguiu). **Onde:**
/// [`RedeWindows::baixar_com_progresso`].
pub fn baixar_com(cli: &Cliente, url: &str, destino: &Path, herdado: bool) -> Result<(), String> {
    let parcial = caminho_parcial(destino);
    let parcial_s =
        parcial.to_str().ok_or_else(|| format!("caminho não-UTF-8: {}", parcial.display()))?;
    if let Some(pai) = destino.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(pai)
            .map_err(|e| format!("não consegui criar {}: {e}", pai.display()))?;
    }
    let res = executar_download(cli, url, parcial_s, herdado).and_then(|()| {
        if parcial.is_file() {
            std::fs::rename(&parcial, destino)
                .map_err(|e| format!("não consegui gravar {}: {e}", destino.display()))
        } else {
            Err(format!("o download de {url} não gerou arquivo"))
        }
    });
    match res {
        Ok(()) => Ok(()),
        Err(e) if parcial.exists() => match std::fs::remove_file(&parcial) {
            Ok(()) => Err(e),
            Err(r) => Err(format!("{e} (e não consegui remover {}: {r})", parcial.display())),
        },
        Err(e) => Err(e),
    }
}

/// **O quê:** dispara o cliente para `parcial`. **Onde:** [`baixar_com`].
fn executar_download(cli: &Cliente, url: &str, parcial: &str, herdado: bool) -> Result<(), String> {
    let (prog, args): (PathBuf, Vec<String>) = match cli {
        Cliente::Curl(p) => (p.clone(), curl_baixar_args(url, parcial, !herdado)),
        Cliente::PowerShell => (powershell_exe(), argv_encoded(&ps_baixar(url, parcial, herdado))),
    };
    let mut c = Command::new(&prog);
    c.args(&args).stdin(Stdio::null()).stdout(Stdio::null());
    if !herdado {
        c.stderr(Stdio::piped());
    }
    let out = c
        .spawn()
        .and_then(|ch| ch.wait_with_output())
        .map_err(|e| format!("não consegui executar {}: {e}", prog.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        let causa = String::from_utf8_lossy(&out.stderr);
        Err(format!("{} falhou ({}) {}", prog.display(), out.status, causa.trim()))
    }
}

/// A rede do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct RedeWindows;

impl Rede for RedeWindows {
    /// **O quê:** corpo de `url` como texto; `None` se falhar. **Onde:** `Plataforma.rede.texto`.
    fn texto(&self, url: &str) -> Option<String> {
        texto_com(&cliente_atual(), url)
    }

    /// **O quê:** baixa `url` em `destino` (silencioso). **Onde:** `Plataforma.rede.baixar`.
    fn baixar(&self, url: &str, destino: &Path) -> Result<(), String> {
        baixar_com(&cliente_atual(), url, destino, false)
    }

    /// **O quê:** baixa com progresso na tela se `mostrar` e o stderr for um terminal.
    /// **Onde:** instalação/atualização interativas.
    fn baixar_com_progresso(&self, url: &str, destino: &Path, mostrar: bool) -> Result<(), String> {
        use std::io::IsTerminal;
        let tty = mostrar && std::io::stderr().is_terminal();
        baixar_com(&cliente_atual(), url, destino, tty)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: escolha do cliente. Onde: `escolher`.
    #[test]
    fn cliente_prefere_curl_exe() {
        assert_eq!(escolher(None), Cliente::PowerShell);
        let c = PathBuf::from("C:\\Windows\\System32\\curl.exe");
        assert_eq!(escolher(Some(c.clone())), Cliente::Curl(c));
    }

    /// O quê: argv do curl de texto e de download (silencioso vs progresso). Onde: builders.
    #[test]
    fn curl_args() {
        let t = curl_texto_args("https://x/y");
        assert_eq!(t.last().map(String::as_str), Some("https://x/y"));
        assert!(t.contains(&"-fsSL".to_string()));
        let sil = curl_baixar_args("https://x", "a.part", true);
        let pro = curl_baixar_args("https://x", "a.part", false);
        assert!(sil.contains(&"-sS".to_string()) && !pro.contains(&"-sS".to_string()));
        assert!(pro.contains(&"-S".to_string()));
        assert!(sil.windows(2).any(|w| w == ["-o", "a.part"]));
    }

    /// O quê: URL hostil fica contida no literal PowerShell. Onde: `ps_texto`/`ps_baixar`.
    #[test]
    fn ps_nao_deixa_url_escapar() {
        let s = ps_texto("https://x/'; Remove-Item C:\\ -Recurse; '");
        assert!(s.contains("''; Remove-Item"), "{s}");
        let b = ps_baixar("https://x", "C:\\a'b.part", false);
        assert!(b.contains("'C:\\a''b.part'"));
        assert!(b.starts_with("$ProgressPreference"));
        assert!(!ps_baixar("https://x", "d", true).starts_with("$ProgressPreference"));
    }

    /// O quê: as sondas usam NUL (não /dev/null) e o formato `code url`. Onde: sondas.
    #[test]
    fn sondas_formato() {
        let r = curl_sonda_release_args("https://github.com/o/r/releases/latest");
        assert!(r.windows(2).any(|w| w == ["-o", "NUL"]));
        assert!(r.contains(&"%{http_code} %{redirect_url}".to_string()));
        let a = curl_sonda_asset_args("https://x");
        assert!(a.windows(2).any(|w| w == ["-r", "0-0"]));
        assert!(ps_sonda_release("https://x").contains("302 $x"));
        assert!(ps_sonda_asset("https://x").contains("StatusCode"));
    }

    /// O quê: cabeçalho extra: divisão e script PowerShell. Onde: `dividir_cabecalho`.
    #[test]
    fn cabecalho_extra() {
        assert_eq!(
            dividir_cabecalho("Accept: application/vnd.github.sha"),
            Some(("Accept".into(), "application/vnd.github.sha".into()))
        );
        assert_eq!(dividir_cabecalho("sem dois pontos"), None);
        assert_eq!(dividir_cabecalho(": v"), None);
        assert_eq!(dividir_cabecalho("A b: v"), None);
        let s = ps_texto_com_cabecalho("https://x", "Accept: a/b").expect("script");
        assert!(s.contains("'Accept'='a/b'"), "{s}");
        assert!(ps_texto_com_cabecalho("https://x", "ruim").is_none());
        assert!(curl_texto_com_cabecalho_args("u", "Accept: a").contains(&"Accept: a".to_string()));
    }

    /// O quê: o `.part` é irmão do destino. Onde: `caminho_parcial` (lib).
    #[test]
    fn parcial_ao_lado() {
        let p = caminho_parcial(Path::new("a/b.exe"));
        assert_eq!(p, PathBuf::from("a/b.exe.part"));
    }
}
