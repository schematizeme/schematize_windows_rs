//! **processos_parse** — parsers e filtros PUROS da consulta de processos do Windows.
//!
//! **O quê:** o CSV do `ConvertTo-Csv` (RFC 4180: aspas, `""`, quebra de linha dentro de campo),
//! a lista `(pid, nome, linha de comando)`, a regra de "este processo é o `nome`?" e a de "a
//! linha de comando cita o diretório?" (o Windows não expõe o cwd de outro processo sem API
//! privilegiada — a aproximação é a linha de comando). Nada aqui executa processo.
//!
//! **Onde:** `processos` (a costura) e os testes no Linux.

/// Um processo da listagem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    /// PID.
    pub pid: u32,
    /// Nome da imagem (`claude.exe`).
    pub nome: String,
    /// Linha de comando completa; `None` se o Windows não a revelou (processo protegido).
    pub cmdline: Option<String>,
}

/// Script que lista os processos como CSV (`ProcessId,Name,CommandLine`).
pub const SCRIPT_LISTAR: &str =
    "Get-CimInstance Win32_Process | Select-Object ProcessId,Name,CommandLine | \
     ConvertTo-Csv -NoTypeInformation";

/// Script que imprime a RAM livre em KB.
pub const SCRIPT_MEMORIA: &str =
    "[Console]::Out.Write((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory)";

/// **O quê:** parseia CSV (vírgula, aspas duplas, `""` literal, CR/LF) em linhas de campos. PURA.
/// Linha totalmente vazia é descartada. **Onde:** [`parse_processos`].
pub fn parse_csv(texto: &str) -> Vec<Vec<String>> {
    let mut linhas: Vec<Vec<String>> = Vec::new();
    let mut campo = String::new();
    let mut linha: Vec<String> = Vec::new();
    let mut aspas = false;
    let mut chars = texto.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        match (aspas, c) {
            (true, '"') => {
                if chars.peek() == Some(&'"') {
                    campo.push('"');
                    chars.next();
                } else {
                    aspas = false;
                }
            }
            (true, _) => campo.push(c),
            (false, '"') => aspas = true,
            (false, ',') => linha.push(std::mem::take(&mut campo)),
            (false, '\r') => {}
            (false, '\n') => {
                linha.push(std::mem::take(&mut campo));
                if linha.iter().any(|f| !f.is_empty()) || linha.len() > 1 {
                    linhas.push(std::mem::take(&mut linha));
                } else {
                    linha.clear();
                }
            }
            (false, _) => campo.push(c),
        }
    }
    if !campo.is_empty() || !linha.is_empty() {
        linha.push(campo);
        linhas.push(linha);
    }
    linhas
}

/// **O quê:** a listagem do [`SCRIPT_LISTAR`] em [`Proc`]s. Cabeçalho e linha com PID não
/// numérico são pulados; `CommandLine` vazio vira `None`. PURA. **Onde:** `ProcessosWindows`.
pub fn parse_processos(csv: &str) -> Vec<Proc> {
    parse_csv(csv)
        .into_iter()
        .filter_map(|f| {
            let pid = f.first()?.trim().parse::<u32>().ok()?;
            let nome = f.get(1)?.clone();
            let cmdline = f.get(2).filter(|c| !c.is_empty()).cloned();
            Some(Proc { pid, nome, cmdline })
        })
        .collect()
}

/// **O quê:** KB livres (saída de [`SCRIPT_MEMORIA`]) → MB. `None` se não for número. PURA.
/// **Onde:** `memoria_disponivel_mb`.
pub fn parse_memoria_mb(saida: &str) -> Option<u64> {
    saida.trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

/// **O quê:** nome de imagem sem `.exe`, minúsculo. PURA. **Onde:** [`e_o_processo`].
fn base(nome: &str) -> String {
    let n = nome.trim().to_lowercase();
    n.strip_suffix(".exe").unwrap_or(&n).to_string()
}

/// **O quê:** `p` é o programa `nome`? Imagem `nome(.exe)`, OU runtime (`node`/`bun`/`deno`)
/// cuja linha de comando cita `nome` como segmento de caminho/`nome-code`/`nome/cli`.
/// `nome` vazio nunca casa. PURA. **Onde:** `contar` e `em_dir`.
pub fn e_o_processo(p: &Proc, nome: &str) -> bool {
    let alvo = base(nome);
    if alvo.is_empty() {
        return false;
    }
    let img = base(&p.nome);
    if img == alvo {
        return true;
    }
    if !matches!(img.as_str(), "node" | "nodejs" | "bun" | "deno") {
        return false;
    }
    let Some(c) = p.cmdline.as_deref() else {
        return false;
    };
    let c = c.to_lowercase().replace('\\', "/");
    c.contains(&format!("/{alvo}/cli"))
        || c.contains(&format!("{alvo}-code"))
        || c.contains(&format!("/{alvo}.js"))
        || c.contains(&format!("/{alvo}.cmd"))
}

/// **O quê:** forma comparável de um caminho: minúsculo, `/`→`\`, sem barra final. PURA.
/// **Onde:** [`cita_dir`].
pub fn norm_caminho(s: &str) -> String {
    s.to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string()
}

/// **O quê:** a linha de comando de `p` cita `dir`? (aproximação do "cwd == dir": o Windows não
/// expõe o cwd de outro processo). `dir` vazio nunca casa. PURA. **Onde:** `em_dir` e
/// `pids_por_argv_em`.
pub fn cita_dir(p: &Proc, dir: &str) -> bool {
    let d = norm_caminho(dir);
    if d.is_empty() {
        return false;
    }
    p.cmdline.as_deref().is_some_and(|c| norm_caminho(c).contains(&d))
}

/// **O quê:** PIDs (sem `eu`) cuja linha de comando contém CADA `trecho`. `trechos` vazio ou com
/// trecho vazio → vazio (fail-closed). PURA. **Onde:** `pids_por_argv`.
pub fn pids_por_trechos(procs: &[Proc], trechos: &[&str], eu: u32) -> Vec<u32> {
    if trechos.is_empty() || trechos.iter().any(|t| t.is_empty()) {
        return Vec::new();
    }
    procs
        .iter()
        .filter(|p| p.pid != eu)
        .filter(|p| p.cmdline.as_deref().is_some_and(|c| trechos.iter().all(|t| c.contains(t))))
        .map(|p| p.pid)
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    const CSV: &str = "\"ProcessId\",\"Name\",\"CommandLine\"\r\n\
\"4\",\"System\",\"\"\r\n\
\"100\",\"claude.exe\",\"\"\"C:\\Users\\a\\.local\\bin\\claude.exe\"\" --dangerously-skip-permissions\"\r\n\
\"200\",\"node.exe\",\"node C:\\x\\@anthropic-ai\\claude-code\\cli.js\"\r\n\
\"300\",\"schematize-overdev.exe\",\"schematize-overdev.exe supervise --project C:\\Proj\\a\"\r\n\
\"abc\",\"lixo\",\"x\"\r\n";

    /// O quê: CSV com aspas escapadas, vazio e multilinha. Onde: `parse_csv`.
    #[test]
    fn csv_casos() {
        let r = parse_csv("a,\"b,c\",\"d\"\"e\"\r\n\"x\ny\",,z\r\n\r\n");
        assert_eq!(r[0], vec!["a", "b,c", "d\"e"]);
        assert_eq!(r[1], vec!["x\ny", "", "z"]);
        assert_eq!(r.len(), 2);
        assert!(parse_csv("").is_empty());
        assert_eq!(parse_csv("\u{feff}k,v")[0], vec!["k", "v"]);
    }

    /// O quê: lista de processos: cabeçalho/lixo pulados, cmdline vazio vira None. Onde: `parse_processos`.
    #[test]
    fn lista() {
        let l = parse_processos(CSV);
        assert_eq!(l.len(), 4);
        assert_eq!(l[0], Proc { pid: 4, nome: "System".into(), cmdline: None });
        assert_eq!(
            l[1].cmdline.as_deref(),
            Some("\"C:\\Users\\a\\.local\\bin\\claude.exe\" --dangerously-skip-permissions")
        );
    }

    /// O quê: memória KB→MB. Onde: `parse_memoria_mb`.
    #[test]
    fn memoria() {
        assert_eq!(parse_memoria_mb("8388608\r\n"), Some(8192));
        assert_eq!(parse_memoria_mb(""), None);
        assert_eq!(parse_memoria_mb("x"), None);
    }

    /// O quê: reconhece o programa pela imagem e pelo runtime node. Onde: `e_o_processo`.
    #[test]
    fn reconhece_programa() {
        let l = parse_processos(CSV);
        assert!(e_o_processo(&l[1], "claude"));
        assert!(e_o_processo(&l[1], "CLAUDE.exe"));
        assert!(e_o_processo(&l[2], "claude"));
        assert!(!e_o_processo(&l[3], "claude"));
        assert!(!e_o_processo(&l[1], ""));
        assert!(!e_o_processo(&l[0], "claude"));
    }

    /// O quê: cita_dir ignora caixa/barras e exige dir não vazio. Onde: `cita_dir`.
    #[test]
    fn dir_citado() {
        let l = parse_processos(CSV);
        assert!(cita_dir(&l[3], "c:/proj/A/"));
        assert!(!cita_dir(&l[3], "C:\\Outro"));
        assert!(!cita_dir(&l[3], ""));
        assert!(!cita_dir(&l[0], "C:\\Proj"));
    }

    /// O quê: pids por trechos: todos os trechos, sem o próprio, fail-closed. Onde: `pids_por_trechos`.
    #[test]
    fn trechos() {
        let l = parse_processos(CSV);
        assert_eq!(pids_por_trechos(&l, &["overdev", "supervise"], 1), vec![300]);
        assert!(pids_por_trechos(&l, &["overdev", "supervise"], 300).is_empty());
        assert!(pids_por_trechos(&l, &[], 1).is_empty());
        assert!(pids_por_trechos(&l, &["overdev", ""], 1).is_empty());
        assert!(pids_por_trechos(&l, &["nada"], 1).is_empty());
    }
}
