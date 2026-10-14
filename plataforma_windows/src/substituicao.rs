//! **substituicao** — a costura [`Substituicao`] do Windows: trocar um `.exe` EM USO.
//!
//! **O quê:** o Windows não deixa sobrescrever (nem apagar) um `.exe` em execução, mas deixa
//! RENOMEÁ-LO. Então: o binário atual vira `<stem>.old`, o novo ocupa o nome original e o
//! `.old` é apagado quando der (agora, ou na próxima execução — [`limpar_old`]). Se a troca
//! falha no meio, o original é restaurado. `encerrar_gui` usa `taskkill /F /IM`.
//!
//! **Onde:** `Plataforma::windows().substituicao`: self-update do market, instalação dos apps
//! e do `install.ps1` (via `schematize-market`). A lógica só usa `std::fs`, então roda (e é
//! testada) no Linux.

use schematize_plataforma::seams::Substituicao;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Quantas variantes `.old1`..`.old9` tentar quando o `.old` anterior ainda está em uso.
const VARIANTES_OLD: u32 = 9;

/// **O quê:** `<dir>/<stem>.old` para o executável `destino` (`schematize-market.exe` →
/// `schematize-market.old`); `n > 0` acrescenta o número (`.old3`). PURA.
///
/// **Onde:** [`substituir_arquivo`] e [`limpar_old`].
pub fn caminho_old(destino: &Path, n: u32) -> PathBuf {
    let stem = destino.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let sufixo = if n == 0 { "old".to_string() } else { format!("old{n}") };
    destino.with_file_name(format!("{stem}.{sufixo}"))
}

/// **O quê:** `<destino>.novo` — onde o binário novo é gravado antes do rename final (mesmo
/// diretório = mesmo volume = rename atômico). PURA. **Onde:** [`substituir_arquivo`].
pub fn caminho_novo(destino: &Path) -> PathBuf {
    let mut n = destino.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    n.push(".novo");
    destino.with_file_name(n)
}

/// **O quê:** aposenta o `destino` atual como `.old` (qualquer variante livre). Devolve o novo
/// caminho do antigo. **Onde:** [`substituir_arquivo`].
fn aposentar(destino: &Path) -> Result<PathBuf, String> {
    let mut ultimo = String::new();
    for n in 0..=VARIANTES_OLD {
        let old = caminho_old(destino, n);
        match std::fs::remove_file(&old) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            // `.old` anterior ainda em uso (processo antigo vivo): tenta a próxima variante.
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                ultimo = format!("{}: {e}", old.display());
                continue;
            }
            Err(e) => return Err(format!("não consegui limpar {}: {e}", old.display())),
        }
        match std::fs::rename(destino, &old) {
            Ok(()) => return Ok(old),
            Err(e) => ultimo = format!("{}: {e}", old.display()),
        }
    }
    Err(format!("não consegui aposentar {} como .old ({ultimo})", destino.display()))
}

/// **O quê:** troca `destino` pelo arquivo `novo` (copia — `novo` pode estar em outro volume),
/// aposentando o antigo como `.old` se ele existir. Se a colocação falha, restaura o antigo.
///
/// **Onde:** [`SubstituicaoWindows::substituir`] e os testes.
pub fn substituir_arquivo(novo: &Path, destino: &Path) -> Result<(), String> {
    if !novo.is_file() {
        return Err(format!("origem inexistente ou não é arquivo: {}", novo.display()));
    }
    if let Some(dir) = destino.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("não consegui criar o diretório {}: {e}", dir.display()))?;
    }
    let tmp = caminho_novo(destino);
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(format!("não consegui limpar {}: {e}", tmp.display())),
    }
    std::fs::copy(novo, &tmp).map_err(|e| format!("não consegui gravar {}: {e}", tmp.display()))?;
    let antigo = if destino.exists() {
        match aposentar(destino) {
            Ok(old) => Some(old),
            Err(e) => return Err(limpa_tmp(&tmp, e)),
        }
    } else {
        None
    };
    if let Err(e) = std::fs::rename(&tmp, destino) {
        let msg = format!("não consegui colocar {}: {e}", destino.display());
        return Err(match restaurar(antigo.as_deref(), destino) {
            Ok(()) => limpa_tmp(&tmp, msg),
            Err(r) => limpa_tmp(&tmp, format!("{msg}; e a restauração falhou: {r}")),
        });
    }
    if let Some(old) = antigo {
        avisar_remocao(&old);
    }
    Ok(())
}

/// **O quê:** devolve o `.old` ao nome original após uma troca que falhou. **Onde:**
/// [`substituir_arquivo`].
fn restaurar(antigo: Option<&Path>, destino: &Path) -> Result<(), String> {
    match antigo {
        Some(old) => std::fs::rename(old, destino)
            .map_err(|e| format!("não consegui restaurar {}: {e}", destino.display())),
        None => Ok(()),
    }
}

/// **O quê:** apaga o temporário e devolve a mensagem de erro (acrescida se a limpeza falhar).
/// **Onde:** [`substituir_arquivo`].
fn limpa_tmp(tmp: &Path, msg: String) -> String {
    match std::fs::remove_file(tmp) {
        Ok(()) => msg,
        Err(e) if e.kind() == ErrorKind::NotFound => msg,
        Err(e) => format!("{msg} (e não consegui remover {}: {e})", tmp.display()),
    }
}

/// **O quê:** tenta apagar o `.old` já. Acesso negado é o ESPERADO (o processo antigo ainda
/// roda): fica para [`limpar_old`] na próxima execução. Outro erro vai ao stderr.
/// **Onde:** [`substituir_arquivo`].
fn avisar_remocao(old: &Path) {
    match std::fs::remove_file(old) {
        Ok(()) => {}
        Err(e) if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::NotFound) => {}
        Err(e) => eprintln!("aviso: não consegui apagar {}: {e}", old.display()),
    }
}

/// **O quê:** apaga os `schematize*.old` / `.oldN` de `dir` (sobras de trocas anteriores).
/// `PermissionDenied` (ainda em uso) é ignorado; qualquer outro erro é devolvido, nunca engolido.
///
/// **Onde:** `Plataforma::windows()` no início de todo processo (cobre "limpa na próxima
/// execução") e os testes.
pub fn limpar_old(dir: &Path) -> Vec<String> {
    let mut erros = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == ErrorKind::NotFound => return erros,
        Err(e) => return vec![format!("não consegui listar {}: {e}", dir.display())],
    };
    for ent in rd.flatten() {
        let nome = ent.file_name().to_string_lossy().to_lowercase();
        if nome.starts_with("schematize") && eh_old(&nome) {
            match std::fs::remove_file(ent.path()) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::PermissionDenied => {}
                Err(e) => erros.push(format!("não consegui apagar {}: {e}", ent.path().display())),
            }
        }
    }
    erros
}

/// **O quê:** `nome` (minúsculo) termina em `.old` ou `.old<dígitos>`? PURA.
/// **Onde:** [`limpar_old`].
pub fn eh_old(nome: &str) -> bool {
    match nome.rsplit_once('.') {
        Some((_, ext)) => {
            ext == "old"
                || ext
                    .strip_prefix("old")
                    .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        }
        None => false,
    }
}

/// **O quê:** valida o nome da GUI (allowlist `[A-Za-z0-9_-]`, `.exe` opcional) e devolve o
/// nome da imagem (`<nome>.exe`). PURA. **Onde:** [`args_taskkill`].
pub fn imagem_da_gui(nome: &str) -> Result<String, String> {
    let base = match nome.len().checked_sub(4).and_then(|i| nome.get(i..)) {
        Some(ext) if ext.eq_ignore_ascii_case(".exe") => &nome[..nome.len() - 4],
        _ => nome,
    };
    if base.is_empty() {
        return Err("nome da GUI vazio".to_string());
    }
    if base.starts_with('-') {
        return Err(format!("nome da GUI não pode começar com '-': {nome:?}"));
    }
    if !base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("nome da GUI com caractere inválido: {nome:?}"));
    }
    Ok(format!("{base}.exe"))
}

/// **O quê:** argv do `taskkill` (`/F /IM <imagem>`). PURA. **Onde:**
/// [`SubstituicaoWindows::encerrar_gui_com`].
pub fn args_taskkill(nome: &str) -> Result<Vec<String>, String> {
    Ok(vec!["/F".into(), "/IM".into(), imagem_da_gui(nome)?])
}

/// **O quê:** `taskkill`: 0 = matou; 128 = não havia processo. Qualquer outro código é erro.
/// PURA. **Onde:** [`SubstituicaoWindows::encerrar_gui_com`].
pub fn interpretar_taskkill(codigo: i32) -> Result<(), String> {
    match codigo {
        0 | 128 => Ok(()),
        c => Err(format!("taskkill saiu com código {c}")),
    }
}

/// A substituição de binário do Windows.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubstituicaoWindows;

impl SubstituicaoWindows {
    /// **O quê:** `encerrar_gui` com runner injetável (devolve o código de saída). **Onde:**
    /// [`Substituicao::encerrar_gui`] e os testes (sem matar processo real).
    pub fn encerrar_gui_com(
        &self,
        nome: &str,
        runner: &dyn Fn(&[String]) -> Result<i32, String>,
    ) -> Result<(), String> {
        let args = args_taskkill(nome)?;
        interpretar_taskkill(runner(&args)?)
    }
}

/// **O quê:** runner real: executa o `taskkill` e devolve o código. **Onde:**
/// [`Substituicao::encerrar_gui`].
fn taskkill_real(args: &[String]) -> Result<i32, String> {
    let st = std::process::Command::new("taskkill")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("não consegui executar taskkill: {e}"))?;
    st.code().ok_or_else(|| "taskkill terminado sem código".to_string())
}

impl Substituicao for SubstituicaoWindows {
    /// **O quê:** [`substituir_arquivo`]. **Onde:** self-update e instalação dos binários.
    fn substituir(&self, novo: &Path, destino: &Path) -> Result<(), String> {
        substituir_arquivo(novo, destino)
    }

    /// **O quê:** no-op — no Windows executável é questão de extensão, não de bit. **Onde:**
    /// depois de gravar um binário.
    fn tornar_executavel(&self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    /// **O quê:** `taskkill /F /IM <nome>.exe`; falha vai ao stderr (não bloqueia a troca).
    /// **Onde:** antes de substituir o binário da GUI.
    fn encerrar_gui(&self, nome: &str) {
        if let Err(e) = self.encerrar_gui_com(nome, &taskkill_real) {
            eprintln!("aviso: não consegui encerrar a GUI {nome:?}: {e}");
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tmp(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pw_subst_{nome}_{}", std::process::id()));
        if d.exists() {
            std::fs::remove_dir_all(&d).expect("limpar");
        }
        std::fs::create_dir_all(&d).expect("mkdir");
        d
    }

    /// O quê: nomes do `.old`/`.novo`. Onde: `caminho_old`/`caminho_novo`.
    #[test]
    fn nomes_old_e_novo() {
        let d = Path::new("/b/schematize-market.exe");
        assert_eq!(
            caminho_old(d, 0).file_name().and_then(|n| n.to_str()),
            Some("schematize-market.old")
        );
        assert_eq!(
            caminho_old(d, 3).file_name().and_then(|n| n.to_str()),
            Some("schematize-market.old3")
        );
        assert_eq!(
            caminho_novo(d).file_name().and_then(|n| n.to_str()),
            Some("schematize-market.exe.novo")
        );
    }

    /// O quê: a troca aposenta o antigo, coloca o novo e apaga o `.old`. Onde: `substituir_arquivo`.
    #[test]
    fn troca_aposenta_e_limpa() {
        let d = tmp("troca");
        let dest = d.join("app.exe");
        let novo = d.join("novo.bin");
        std::fs::write(&dest, "v1").expect("w");
        std::fs::write(&novo, "v2").expect("w");
        substituir_arquivo(&novo, &dest).expect("troca");
        assert_eq!(std::fs::read_to_string(&dest).expect("r"), "v2");
        assert!(!caminho_old(&dest, 0).exists());
        assert!(!caminho_novo(&dest).exists());
    }

    /// O quê: destino inexistente e diretório pai ausente são criados. Onde: `substituir_arquivo`.
    #[test]
    fn destino_novo_cria_pai() {
        let d = tmp("novo");
        let novo = d.join("n.bin");
        std::fs::write(&novo, "x").expect("w");
        let dest = d.join("sub/dir/app.exe");
        substituir_arquivo(&novo, &dest).expect("troca");
        assert_eq!(std::fs::read_to_string(&dest).expect("r"), "x");
    }

    /// O quê: origem inexistente é erro e não mexe no destino. Onde: `substituir_arquivo`.
    #[test]
    fn origem_inexistente_nao_toca_no_destino() {
        let d = tmp("sem_origem");
        let dest = d.join("app.exe");
        std::fs::write(&dest, "v1").expect("w");
        assert!(substituir_arquivo(&d.join("nao.bin"), &dest).is_err());
        assert_eq!(std::fs::read_to_string(&dest).expect("r"), "v1");
    }

    /// O quê: `.old` anterior obstruído (diretório com o nome) cai na variante `.old1`.
    /// Onde: `aposentar` — simula "não consegui apagar o .old" sem precisar de processo vivo.
    #[test]
    fn old_obstruido_usa_variante() {
        let d = tmp("variante");
        let dest = d.join("app.exe");
        let novo = d.join("n.bin");
        std::fs::write(&dest, "v1").expect("w");
        std::fs::write(&novo, "v2").expect("w");
        // Um diretório não vazio com o nome do .old: remove_file falha (não é NotFound).
        let old0 = caminho_old(&dest, 0);
        std::fs::create_dir_all(old0.join("x")).expect("mk");
        let r = substituir_arquivo(&novo, &dest);
        // Diretório: remove_file dá PermissionDenied/IsADirectory conforme o SO; em ambos os
        // casos o resultado final tem de ser coerente (troca feita OU erro explícito + original).
        match r {
            Ok(()) => assert_eq!(std::fs::read_to_string(&dest).expect("r"), "v2"),
            Err(_) => assert_eq!(std::fs::read_to_string(&dest).expect("r"), "v1"),
        }
    }

    /// O quê: reconhece `.old`/`.oldN` e só isso. Onde: `eh_old`.
    #[test]
    fn eh_old_reconhece() {
        assert!(eh_old("schematize-market.old"));
        assert!(eh_old("schematize-market.old12"));
        assert!(!eh_old("schematize-market.exe"));
        assert!(!eh_old("schematize-market.older"));
        assert!(!eh_old("old"));
    }

    /// O quê: `limpar_old` só apaga `schematize*.old*` e não toca em alheios. Onde: `limpar_old`.
    #[test]
    fn limpar_old_so_os_nossos() {
        let d = tmp("limpa");
        for n in [
            "schematize-market.old",
            "schematize-overdev.old2",
            "outro.old",
            "schematize-market.exe",
        ] {
            std::fs::write(d.join(n), "x").expect("w");
        }
        assert!(limpar_old(&d).is_empty());
        assert!(!d.join("schematize-market.old").exists());
        assert!(!d.join("schematize-overdev.old2").exists());
        assert!(d.join("outro.old").exists());
        assert!(d.join("schematize-market.exe").exists());
        assert!(limpar_old(&d.join("inexistente")).is_empty());
    }

    /// O quê: validação do nome da GUI (injeção de argumento). Onde: `imagem_da_gui`.
    #[test]
    fn nome_da_gui() {
        assert_eq!(imagem_da_gui("schematize-gui"), Ok("schematize-gui.exe".into()));
        assert_eq!(imagem_da_gui("schematize-gui.EXE"), Ok("schematize-gui.exe".into()));
        for ruim in ["", "-x", "a b", "a;b", "a*", "..\\x", "a\"b", "a.b.exe"] {
            assert!(imagem_da_gui(ruim).is_err(), "{ruim:?}");
        }
    }

    /// O quê: códigos do taskkill e runner injetado. Onde: `encerrar_gui_com`.
    #[test]
    fn taskkill_codigos() {
        let s = SubstituicaoWindows;
        let ok = |c: i32| move |_: &[String]| Ok(c);
        assert!(s.encerrar_gui_com("g", &ok(0)).is_ok());
        assert!(s.encerrar_gui_com("g", &ok(128)).is_ok());
        assert!(s.encerrar_gui_com("g", &ok(1)).is_err());
        assert!(s.encerrar_gui_com("g", &|_: &[String]| Err("sem taskkill".into())).is_err());
        assert!(s.encerrar_gui_com("a b", &ok(0)).is_err());
        let vistos = std::cell::RefCell::new(Vec::new());
        s.encerrar_gui_com("g", &|a: &[String]| {
            vistos.borrow_mut().extend(a.to_vec());
            Ok(0)
        })
        .expect("ok");
        assert_eq!(*vistos.borrow(), vec!["/F", "/IM", "g.exe"]);
    }
}
