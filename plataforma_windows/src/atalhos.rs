//! **atalhos** — a costura [`Atalhos`] do Windows: atalho `.lnk` no Menu Iniciar.
//!
//! **O quê:** instala `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Schematize\<Nome>.lnk`
//! (criado via `WScript.Shell` COM no PowerShell) apontando para a janela (`gui`) ou, sem
//! janela, para o CLI com os argumentos de fallback. O ícone é um `.ico` gravado a partir dos
//! PNGs que o host entrega em `EntradaAtalho.icones`, em `%LOCALAPPDATA%\Schematize\icons`.
//! O script do atalho e os caminhos são gerados por funções PURAS (testadas no Linux).
//!
//! **Onde:** `Plataforma::windows().atalhos`: fim da instalação e `desktop --install/--remover`.

use crate::exec::rodar_ps;
use crate::ico::ico_de_pngs;
use crate::ps::literal;
use schematize_plataforma::seams::atalhos::{validar, EntradaAtalho};
use schematize_plataforma::seams::Atalhos;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Pasta do Menu Iniciar onde ficam os atalhos da casa.
pub const PASTA_MENU: &str = "Schematize";

/// Onde os atalhos e ícones moram (derivado do ambiente; injetável nos testes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pastas {
    /// `%APPDATA%` (Roaming).
    pub appdata: PathBuf,
    /// `%LOCALAPPDATA%`.
    pub localappdata: PathBuf,
}

impl Pastas {
    /// **O quê:** pastas a partir de `APPDATA`/`LOCALAPPDATA` (`get`), com `home\AppData\…` de
    /// reserva. PURA. **Onde:** [`Pastas::do_ambiente`] e os testes.
    pub fn de(get: &dyn Fn(&str) -> Option<String>, home: &Path) -> Self {
        let pega = |k: &str, resto: &str| {
            get(k)
                .filter(|v| !v.trim().is_empty() && Path::new(v).is_absolute())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join(resto))
        };
        Pastas { appdata: pega("APPDATA", "Roaming"), localappdata: pega("LOCALAPPDATA", "Local") }
    }

    /// **O quê:** [`Pastas::de`] com o ambiente real. **Onde:** `AtalhosWindows`.
    pub fn do_ambiente(home: &Path) -> Self {
        Self::de(&|k| std::env::var(k).ok(), home)
    }

    /// **O quê:** `<APPDATA>\Microsoft\Windows\Start Menu\Programs\Schematize`. PURA.
    pub fn menu(&self) -> PathBuf {
        self.appdata
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(PASTA_MENU)
    }

    /// **O quê:** `<LOCALAPPDATA>\Schematize\icons`. PURA.
    pub fn icones(&self) -> PathBuf {
        self.localappdata.join("Schematize").join("icons")
    }
}

/// **O quê:** nome de arquivo seguro para o `.lnk` a partir do nome de exibição: troca
/// `\ / : * ? " < > |` e controles por espaço e apara; vazio vira `Schematize`. PURA.
///
/// **Onde:** [`caminho_lnk`].
pub fn nome_de_arquivo(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { ' ' } else { c })
        .collect();
    let limpo = limpo.trim().trim_end_matches('.').trim().to_string();
    if limpo.is_empty() {
        "Schematize".to_string()
    } else {
        limpo
    }
}

/// **O quê:** caminho do `.lnk` de `e`. PURA. **Onde:** instalar/remover.
pub fn caminho_lnk(p: &Pastas, e: &EntradaAtalho) -> PathBuf {
    p.menu().join(format!("{}.lnk", nome_de_arquivo(&e.nome)))
}

/// **O quê:** caminho do `.ico` de `e` (`<icone_nome>.ico`). PURA. **Onde:** instalar/remover.
pub fn caminho_ico(p: &Pastas, e: &EntradaAtalho) -> PathBuf {
    p.icones().join(format!("{}.ico", e.icone_nome))
}

/// **O quê:** `(alvo, argumentos)` do atalho: a janela, ou o CLI com `args_sem_gui`. `Err` se
/// não há nenhum dos dois. PURA. **Onde:** [`script_lnk`].
pub fn alvo_do_atalho(e: &EntradaAtalho) -> Result<(PathBuf, String), String> {
    match (&e.gui, &e.bin) {
        (Some(g), _) => Ok((g.clone(), String::new())),
        (None, Some(b)) => Ok((b.clone(), e.args_sem_gui.clone())),
        (None, None) => Err("atalho sem `gui` nem `bin`: não há o que executar".to_string()),
    }
}

/// **O quê:** o script PowerShell (COM `WScript.Shell`) que cria o `.lnk`. Todo valor passa por
/// [`literal`]. `ico` é `None` se a entrada não trouxe ícone. PURA. **Onde:** instalar.
pub fn script_lnk(e: &EntradaAtalho, lnk: &Path, ico: Option<&Path>) -> Result<String, String> {
    let (alvo, args) = alvo_do_atalho(e)?;
    let lnk_s = lnk.to_string_lossy();
    let alvo_s = alvo.to_string_lossy();
    let mut s = format!(
        "$ws = New-Object -ComObject WScript.Shell; $l = $ws.CreateShortcut({}); \
         $l.TargetPath = {}; $l.Arguments = {}; $l.WorkingDirectory = {};",
        literal(&lnk_s),
        literal(&alvo_s),
        literal(&args),
        literal(&e.home.to_string_lossy()),
    );
    if let Some(c) = &e.comentario {
        s.push_str(&format!(" $l.Description = {};", literal(c)));
    }
    if let Some(i) = ico {
        s.push_str(&format!(" $l.IconLocation = {};", literal(&format!("{},0", i.display()))));
    }
    s.push_str(" $l.Save()");
    Ok(s)
}

/// **O quê:** grava o `.ico` de `e` em `destino`; sem PNGs devolve `Ok(false)`. **Onde:** instalar.
fn gravar_ico(e: &EntradaAtalho, destino: &Path) -> Result<bool, String> {
    if e.icones.is_empty() {
        return Ok(false);
    }
    let bytes = ico_de_pngs(&e.icones)?;
    if let Some(pai) = destino.parent() {
        std::fs::create_dir_all(pai)
            .map_err(|er| format!("não consegui criar {}: {er}", pai.display()))?;
    }
    std::fs::write(destino, bytes)
        .map_err(|er| format!("não consegui gravar {}: {er}", destino.display()))?;
    Ok(true)
}

/// **O quê:** remove `p`; inexistente é `Ok`. **Onde:** remover.
fn remover_se_existe(p: &Path) -> Result<(), String> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("não consegui remover {}: {e}", p.display())),
    }
}

/// Os atalhos do Windows (Menu Iniciar).
#[derive(Debug, Default, Clone, Copy)]
pub struct AtalhosWindows;

impl Atalhos for AtalhosWindows {
    /// **O quê:** grava o `.ico` (se há PNGs) e cria/regrava o `.lnk`. Falha no ícone obrigatório
    /// é `Err`; no opcional vira aviso em stderr. **Onde:** fim da instalação dos apps.
    fn instalar(&self, e: &EntradaAtalho) -> Result<(), String> {
        validar(e)?;
        let p = Pastas::do_ambiente(&e.home);
        let ico = caminho_ico(&p, e);
        let tem_ico = match gravar_ico(e, &ico) {
            Ok(b) => b,
            Err(er) if e.icone_obrigatorio => return Err(er),
            Err(er) => {
                eprintln!("aviso: ícone do atalho: {er}");
                false
            }
        };
        let lnk = caminho_lnk(&p, e);
        std::fs::create_dir_all(p.menu())
            .map_err(|er| format!("não consegui criar {}: {er}", p.menu().display()))?;
        let script = script_lnk(e, &lnk, tem_ico.then_some(ico.as_path()))?;
        rodar_ps(&script).map(|_| ()).map_err(|er| format!("atalho {}: {er}", lnk.display()))
    }

    /// **O quê:** remove o `.lnk` e o `.ico`; o que não existe é `Ok`. **Onde:** `desktop --remover`.
    fn remover(&self, e: &EntradaAtalho) -> Result<(), String> {
        validar(e)?;
        let p = Pastas::do_ambiente(&e.home);
        remover_se_existe(&caminho_lnk(&p, e))?;
        remover_se_existe(&caminho_ico(&p, e))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Home de teste ABSOLUTO em qualquer SO (no Windows `/x` não é absoluto).
    fn home() -> PathBuf {
        std::env::temp_dir().join("pw_ana")
    }

    fn entrada() -> EntradaAtalho {
        let bin = home().join(".cargo").join("bin");
        EntradaAtalho {
            home: home(),
            id: "schematize-market".into(),
            nome: "Schematize Market".into(),
            generico: None,
            comentario: Some("Instala o d'ia".into()),
            bin: Some(bin.join("schematize-market.exe")),
            args_sem_gui: "list --wait".into(),
            gui: Some(bin.join("schematize-market-gui.exe")),
            categorias: vec![],
            palavras_chave: vec![],
            startup_notify: None,
            startup_wm_class: None,
            icone_nome: "schematize-market".into(),
            icones: vec![],
            icone_extras: false,
            icone_obrigatorio: true,
            cache_icones: false,
            legados: vec![],
        }
    }

    fn pastas() -> Pastas {
        let ap = home().join("Roaming").to_string_lossy().into_owned();
        Pastas::de(&|k| (k == "APPDATA").then(|| ap.clone()), &home())
    }

    /// O quê: caminhos do Menu Iniciar e dos ícones, com reserva sob o HOME. Onde: `Pastas`.
    #[test]
    fn pastas_e_caminhos() {
        let p = pastas();
        assert!(p.menu().ends_with(Path::new("Programs").join("Schematize")));
        assert!(p.menu().starts_with(home().join("Roaming")));
        assert_eq!(p.localappdata, home().join("AppData").join("Local"));
        let e = entrada();
        assert!(caminho_lnk(&p, &e).ends_with("Schematize Market.lnk"));
        assert!(caminho_ico(&p, &e).ends_with("schematize-market.ico"));
        assert!(p.icones().ends_with(Path::new("Schematize").join("icons")));
    }

    /// O quê: APPDATA relativo/vazio é ignorado (fail-closed para o HOME). Onde: `Pastas::de`.
    #[test]
    fn appdata_hostil() {
        let p = Pastas::de(&|_| Some("rel".into()), &home());
        assert_eq!(p.appdata, home().join("AppData").join("Roaming"));
        let q = Pastas::de(&|_| Some("  ".into()), &home());
        assert_eq!(q.localappdata, home().join("AppData").join("Local"));
    }

    /// O quê: nome de arquivo sem caracteres proibidos. Onde: `nome_de_arquivo`.
    #[test]
    fn nome_seguro() {
        assert_eq!(nome_de_arquivo("A/B:C*"), "A B C");
        assert_eq!(nome_de_arquivo("..."), "Schematize");
        assert_eq!(nome_de_arquivo("  "), "Schematize");
        assert_eq!(nome_de_arquivo("Market"), "Market");
    }

    /// O quê: com GUI o alvo é a janela; sem GUI é o CLI com args; sem nada é erro.
    /// Onde: `alvo_do_atalho`.
    #[test]
    fn alvo() {
        let mut e = entrada();
        assert!(alvo_do_atalho(&e).expect("gui").0.ends_with("schematize-market-gui.exe"));
        e.gui = None;
        let (b, a) = alvo_do_atalho(&e).expect("cli");
        assert!(b.ends_with("schematize-market.exe") && a == "list --wait");
        e.bin = None;
        assert!(alvo_do_atalho(&e).is_err());
    }

    /// O quê: o script leva alvo, ícone e descrição; aspas simples são dobradas. Onde: `script_lnk`.
    #[test]
    fn script_do_lnk() {
        let e = entrada();
        let ico = home().join("m.ico");
        let s = script_lnk(&e, &home().join("M.lnk"), Some(&ico)).expect("s");
        assert!(s.contains("WScript.Shell") && s.ends_with("$l.Save()"));
        assert!(s.contains("$l.TargetPath = '"));
        assert!(s.contains("schematize-market-gui.exe'"));
        assert!(s.contains(&format!("$l.IconLocation = '{},0'", ico.display())));
        assert!(s.contains("Instala o d''ia"));
        let sem = script_lnk(&e, &home().join("M.lnk"), None).expect("s");
        assert!(!sem.contains("IconLocation"));
    }

    /// O quê: sem PNG não grava .ico; com PNG inválido é erro; com PNG válido grava. Onde: `gravar_ico`.
    #[test]
    fn gravar_ico_casos() {
        let d = std::env::temp_dir().join(format!("pw_ico_{}", std::process::id()));
        let alvo = d.join("sub").join("x.ico");
        let mut e = entrada();
        assert_eq!(gravar_ico(&e, &alvo), Ok(false));
        e.icones = vec![(16, b"lixo".to_vec())];
        assert!(gravar_ico(&e, &alvo).is_err());
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&16u32.to_be_bytes());
        png.extend_from_slice(&16u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        e.icones = vec![(16, png)];
        assert_eq!(gravar_ico(&e, &alvo), Ok(true));
        assert!(alvo.is_file());
        std::fs::remove_dir_all(&d).expect("limpar");
    }
}
