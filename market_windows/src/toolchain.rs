//! **toolchain** — Rust e MSVC Build Tools no Windows.
//!
//! **O quê:** [`garantir_toolchain`] devolve o caminho do `cargo` (instalando o rustup pelo
//! `rustup-init.exe` oficial se faltar); [`garantir_build_deps`] confere o toolchain MSVC
//! (`vswhere`/`cl`) e, se faltar, instala o Visual Studio 2022 Build Tools (workload C++) pelo
//! `winget`, mandando a pessoa REABRIR o terminal. As decisões são puras e testadas no Linux.
//!
//! **Onde:** `ReceitasWindows::{garantir_toolchain, garantir_build_deps}` — todo build do fonte.

use schematize_plataforma::seams::Plataforma;
use std::path::{Path, PathBuf};

/// URL do instalador oficial do rustup para Windows x64.
pub const URL_RUSTUP_INIT: &str = "https://win.rustup.rs/x86_64";

/// **O quê:** `<home>\.cargo\bin\cargo.exe`. PURA. **Onde:** [`garantir_toolchain`].
pub fn cargo_abs(home: &Path) -> PathBuf {
    home.join(".cargo").join("bin").join("cargo.exe")
}

/// **O quê:** onde fica o `vswhere.exe` (instalado com o Visual Studio/Build Tools), a partir de
/// `ProgramFiles(x86)`. PURA. **Onde:** [`msvc_instalado`].
pub fn caminho_vswhere(program_files_x86: &str) -> PathBuf {
    PathBuf::from(program_files_x86)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe")
}

/// **O quê:** argv do `vswhere` que imprime o diretório de uma instalação com as ferramentas C++
/// x64 (o que o `link.exe` do Rust MSVC precisa). PURA. **Onde:** [`msvc_instalado`].
pub fn args_vswhere() -> Vec<String> {
    [
        "-latest",
        "-products",
        "*",
        "-requires",
        "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "-property",
        "installationPath",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// **O quê:** o comando winget que instala o Build Tools 2022 com a workload C++ (UAC aparece).
/// PURA. **Onde:** [`garantir_build_deps`].
pub fn comando_build_tools() -> Vec<String> {
    [
        "winget",
        "install",
        "--id",
        "Microsoft.VisualStudio.2022.BuildTools",
        "-e",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--override",
        "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Mensagem quando o MSVC falta e não há winget.
pub const SEM_MSVC_SEM_WINGET: &str = "no Windows a compilação do fonte precisa do MSVC Build \
Tools. Instale o \"Visual Studio Build Tools\" com a carga de trabalho \"Desenvolvimento para \
desktop com C++\" (https://aka.ms/vs/17/release/vs_BuildTools.exe) e rode de novo — ou use o \
binário pré-compilado.";

/// Mensagem depois de instalar o MSVC.
pub const REABRIR_TERMINAL: &str =
    "instalei o MSVC Build Tools — REABRA o terminal (para o PATH pegar) e rode de novo.";

/// **O quê:** o MSVC está instalado? `cl` no PATH (prompt de desenvolvedor) OU o `vswhere`
/// responde um caminho. **Onde:** [`garantir_build_deps`].
fn msvc_instalado(plat: &Plataforma) -> bool {
    if plat.shell.existe("cl") {
        return true;
    }
    let Some(pf) = std::env::var_os("ProgramFiles(x86)") else { return false };
    let vsw = caminho_vswhere(&pf.to_string_lossy());
    if !vsw.is_file() {
        return false;
    }
    let out = std::process::Command::new(&vsw).args(args_vswhere()).output();
    matches!(out, Ok(o) if o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
}

/// **O quê:** garante o `cargo` e devolve o caminho. Já no PATH → `cargo`; em `~/.cargo/bin` →
/// absoluto; senão baixa e roda o `rustup-init.exe` (`-y --profile minimal`).
/// **Onde:** `ReceitasWindows::garantir_toolchain`.
///
/// **Por que o absoluto:** logo depois do rustup o PATH deste processo ainda não tem
/// `~/.cargo/bin`.
pub fn garantir_toolchain(plat: &Plataforma) -> Result<PathBuf, String> {
    let home = schematize_plataforma::home::home_do_ambiente()
        .ok_or_else(|| "USERPROFILE não definido; não sei onde fica o ~/.cargo.".to_string())?;
    let abs = cargo_abs(&home);
    if plat.shell.existe("cargo") {
        return Ok(PathBuf::from("cargo"));
    }
    if abs.is_file() {
        return Ok(abs);
    }
    println!("→ instalando Rust (rustup-init, perfil mínimo)…");
    let tmp = std::env::temp_dir().join("schematize-rustup-init.exe");
    plat.rede.baixar_com_progresso(URL_RUSTUP_INIT, &tmp, true).map_err(|e| {
        format!("não consegui baixar o rustup-init.exe ({e}). Instale o Rust em https://rustup.rs")
    })?;
    let tmp_s = tmp.to_str().ok_or("caminho temporário não-UTF-8")?;
    let st = plat
        .shell
        .executar(&[tmp_s, "-y", "--profile", "minimal"], true)
        .map_err(|e| format!("não consegui executar o rustup-init.exe: {e}"))?;
    if let Err(e) = std::fs::remove_file(&tmp) {
        eprintln!("aviso: não consegui apagar {}: {e}", tmp.display());
    }
    if !st.success() {
        return Err(format!("o rustup-init.exe falhou ({st})."));
    }
    if abs.is_file() {
        Ok(abs)
    } else if plat.shell.existe("cargo") {
        Ok(PathBuf::from("cargo"))
    } else {
        Err("o rustup instalou mas o `cargo` não apareceu. Reabra o terminal e rode de novo."
            .to_string())
    }
}

/// **O quê:** garante o MSVC (necessário para o alvo `x86_64-pc-windows-msvc`). Faltando, instala
/// o Build Tools pelo winget e devolve `Err` mandando reabrir o terminal (o PATH só pega em
/// terminal novo). **Onde:** `ReceitasWindows::garantir_build_deps`, antes do 1º `cargo build`.
pub fn garantir_build_deps(plat: &Plataforma) -> Result<(), String> {
    if msvc_instalado(plat) {
        return Ok(());
    }
    if !plat.shell.existe("winget") {
        return Err(SEM_MSVC_SEM_WINGET.to_string());
    }
    println!("→ instalando o Visual Studio Build Tools (MSVC) via winget (pede permissão de administrador)…");
    let cmd = comando_build_tools();
    let argv: Vec<&str> = cmd.iter().map(String::as_str).collect();
    let st = plat
        .shell
        .executar(&argv, true)
        .map_err(|e| format!("não consegui executar o winget: {e}"))?;
    if !st.success() {
        return Err(format!(
            "o winget não conseguiu instalar o Build Tools ({st}). {SEM_MSVC_SEM_WINGET}"
        ));
    }
    Err(REABRIR_TERMINAL.to_string())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: caminhos derivados. Onde: `cargo_abs` / `caminho_vswhere`.
    #[test]
    fn caminhos() {
        assert!(
            cargo_abs(Path::new("/h")).ends_with(Path::new(".cargo").join("bin").join("cargo.exe"))
        );
        assert!(caminho_vswhere("/pf86").ends_with("vswhere.exe"));
    }

    /// O quê: o vswhere pede o componente C++ x64; o winget leva a workload VCTools.
    /// Onde: `args_vswhere` / `comando_build_tools`.
    #[test]
    fn comandos_msvc() {
        assert!(
            args_vswhere().contains(&"Microsoft.VisualStudio.Component.VC.Tools.x86.x64".into())
        );
        let c = comando_build_tools();
        assert_eq!(c[0], "winget");
        assert!(c.last().is_some_and(|l| l.contains("Microsoft.VisualStudio.Workload.VCTools")));
        assert!(c.contains(&"Microsoft.VisualStudio.2022.BuildTools".to_string()));
    }
}
