//! **ps** — construção PURA de comandos PowerShell (quoting, `-EncodedCommand`, flags).
//!
//! **O quê:** tudo o que é preciso para falar com o `powershell.exe` sem injeção: escapar um
//! literal (`'…'`), codificar um script em Base64 UTF-16LE (`-EncodedCommand`, que dispensa
//! qualquer quoting de linha de comando) e montar o argv. Nenhuma função aqui executa algo — a
//! execução mora nos módulos das costuras e é `cfg(windows)`.
//!
//! **Onde:** `shell`, `rede`, `atalhos`, `pathsetup`, `processos` e o crate `market_windows`.

/// Flags comuns a toda invocação: sem perfil, sem prompt, sem política de execução.
pub const FLAGS_BASE: [&str; 5] =
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"];

/// Prefixo dos scripts internos: saída em UTF-8 (senão acento vira lixo na página de código OEM),
/// erro fatal (`Stop`, para o exit code refletir a falha) e TLS 1.2 (PowerShell 5.1 usa TLS 1.0).
pub const PREAMBULO: &str = "$ErrorActionPreference='Stop'; \
[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; ";

/// **O quê:** escapa `s` como literal de string PowerShell entre aspas simples (`'` vira `''`)
/// e devolve JÁ com as aspas. Aspas simples não interpolam `$`, backtick nem nada.
///
/// **Onde:** todo valor externo (URL, caminho, nome) que entra num script gerado.
pub fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// **O quê:** o argv (sem o programa) de `powershell.exe -Command <script>`.
///
/// **Onde:** `Shell::run`/`run_herdado`, que recebem o texto do plano de receita.
pub fn argv_command(script: &str) -> Vec<String> {
    let mut v: Vec<String> = FLAGS_BASE.iter().map(|s| s.to_string()).collect();
    v.push(script.to_string());
    v
}

/// **O quê:** o argv (sem o programa) de `powershell.exe … -EncodedCommand <b64>` para um script
/// INTERNO (com [`PREAMBULO`]). Base64 de UTF-16LE: nenhum caractere do script passa pelo
/// parser de linha de comando.
///
/// **Onde:** atalhos, PATH, processos e as sondas de rede.
pub fn argv_encoded(script: &str) -> Vec<String> {
    let completo = format!("{PREAMBULO}{script}");
    vec![
        "-NoProfile".into(),
        "-NonInteractive".into(),
        "-ExecutionPolicy".into(),
        "Bypass".into(),
        "-EncodedCommand".into(),
        base64_utf16le(&completo),
    ]
}

/// **O quê:** Base64 (RFC 4648, com padding) dos bytes UTF-16LE de `s`. PURA, std-only.
///
/// **Onde:** [`argv_encoded`].
pub fn base64_utf16le(s: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// **O quê:** quoting de UM argumento para a linha de comando do Windows (regras do
/// `CommandLineToArgvW`: aspas só se preciso; `\` antes de `"` dobra). Para montar linhas
/// brutas (`cmd /c start`), onde o `Command::args` do Rust não serve.
///
/// **Onde:** `terminal` (fallback `cmd /c start`) e `abridor`.
pub fn arg_win(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"', '\n']) {
        return a.to_string();
    }
    let mut out = String::from("\"");
    let mut barras = 0usize;
    for c in a.chars() {
        match c {
            '\\' => barras += 1,
            '"' => {
                out.push_str(&"\\".repeat(barras * 2 + 1));
                out.push('"');
                barras = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(barras));
                out.push(c);
                barras = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(barras * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: aspas simples dobram; nada mais é tocado. Onde: `literal`.
    #[test]
    fn literal_dobra_aspa_simples() {
        assert_eq!(literal("a'b"), "'a''b'");
        assert_eq!(literal("$(rm -rf /)"), "'$(rm -rf /)'");
        assert_eq!(literal(""), "''");
    }

    /// O quê: vetores conhecidos de Base64 (RFC 4648) sobre UTF-16LE. Onde: `base64_utf16le`.
    #[test]
    fn base64_utf16le_vetores() {
        // "a" = 61 00 -> YQA=
        assert_eq!(base64_utf16le("a"), "YQA=");
        // "ls" = 6C 00 73 00 -> bAB zAA==
        assert_eq!(base64_utf16le("ls"), "bABzAA==");
        assert_eq!(base64_utf16le(""), "");
        // fora do BMP: U+1F600 = D83D DE00 -> 3D D8 00 DE -> PdgA3g==
        assert_eq!(base64_utf16le("\u{1F600}"), "PdgA3g==");
    }

    /// O quê: o argv codificado leva as flags e o payload como ÚLTIMO item. Onde: `argv_encoded`.
    #[test]
    fn argv_encoded_forma() {
        let a = argv_encoded("Get-Date");
        assert_eq!(a[4], "-EncodedCommand");
        assert_eq!(a.len(), 6);
        assert!(!a[5].contains(' '));
    }

    /// O quê: o argv de -Command leva o script intacto no fim. Onde: `argv_command`.
    #[test]
    fn argv_command_forma() {
        let a = argv_command("winget --version");
        assert_eq!(a.last().map(String::as_str), Some("winget --version"));
        assert_eq!(a[0], "-NoProfile");
    }

    /// O quê: quoting do CommandLineToArgvW (espaço, aspas, barra final). Onde: `arg_win`.
    #[test]
    fn arg_win_regras() {
        assert_eq!(arg_win("abc"), "abc");
        assert_eq!(arg_win("a b"), "\"a b\"");
        assert_eq!(arg_win(""), "\"\"");
        assert_eq!(arg_win("a\"b"), "\"a\\\"b\"");
        assert_eq!(arg_win("C:\\Program Files\\"), "\"C:\\Program Files\\\\\"");
    }
}
