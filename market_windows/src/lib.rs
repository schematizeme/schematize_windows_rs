//! **market_windows** — `ReceitasWindows`, o impl Windows de [`market::receitas::Receitas`].
//!
//! **O quê:** o que o market faz diferente no Windows (ADR-0026): planos de instalação por
//! `winget`/`rustup-init`/instaladores PowerShell oficiais ([`receitas`]), toolchain Rust e MSVC
//! ([`toolchain`]), execução de passo no PowerShell e as sondas HTTP do GitHub (`curl.exe` ou
//! `Invoke-WebRequest`, reusando os construtores de `plataforma_windows::rede`).
//!
//! **Onde:** o `main` de `schematize-market.exe` (`crate bins`).

pub mod receitas;
pub mod toolchain;

use market::environments::defs::{Env, Recipe, Step, Tool};
use market::environments::detect::Family;
use market::environments::{Machine, Method};
use market::receitas::Receitas;
use plataforma_windows::exec::resolver_alias_no_ambiente;
use plataforma_windows::rede::{
    capturar, cliente_atual, curl_sonda_asset_args, curl_sonda_release_args,
    curl_texto_com_cabecalho_args, ps_sonda_asset, ps_sonda_release, ps_texto_com_cabecalho,
    Cliente,
};
use schematize_plataforma::seams::Plataforma;
use std::path::PathBuf;

/// As receitas do Windows.
#[derive(Debug, Clone, Copy)]
pub struct ReceitasWindows {
    /// O `winget` existe nesta máquina? (decide os planos de runtime e do VS Code.)
    pub winget: bool,
}

impl ReceitasWindows {
    /// **O quê:** detecta o winget no PATH (alias de `WindowsApps`). **Onde:** o `main`.
    pub fn detectar() -> Self {
        ReceitasWindows { winget: resolver_alias_no_ambiente("winget.exe").is_some() }
    }
}

impl Default for ReceitasWindows {
    /// **O quê:** [`ReceitasWindows::detectar`]. **Onde:** conveniência.
    fn default() -> Self {
        Self::detectar()
    }
}

impl Receitas for ReceitasWindows {
    /// **O quê:** [`receitas::instalar`]. **Onde:** `install`/`switch`.
    fn install_recipe(&self, env: &Env, m: Method, _fam: Family, _mise: bool) -> Recipe {
        receitas::instalar(env, m, self.winget)
    }

    /// **O quê:** [`receitas::remover`]. **Onde:** `remove`/`switch`.
    fn remove_recipe(&self, env: &Env, m: Method, _fam: Family) -> Recipe {
        receitas::remover(env, m, self.winget)
    }

    /// **O quê:** [`receitas::ferramenta_instalar`]. **Onde:** `install <ferramenta>`.
    fn tool_install_recipe(&self, tool: &Tool, _fam: Family) -> Recipe {
        receitas::ferramenta_instalar(tool, self.winget)
    }

    /// **O quê:** [`receitas::ferramenta_remover`]. **Onde:** `remove <ferramenta>`.
    fn tool_remove_recipe(&self, tool: &Tool, _fam: Family) -> Recipe {
        receitas::ferramenta_remover(tool, self.winget)
    }

    /// **O quê:** `Unknown` — `Family` não tem variante Windows e `Unknown` desliga o método
    /// `distro` (que não existe aqui). **Onde:** `Machine::probe`.
    fn family(&self) -> Family {
        Family::Unknown
    }

    /// **O quê:** só `official` (winget/rustup) e `docker` (se houver) estão disponíveis.
    /// **Onde:** `Machine::available` e `install --method`.
    fn method_reason(&self, m: &Machine, method: Method) -> Result<(), String> {
        match method {
            Method::Official => Ok(()),
            Method::Docker if m.docker => Ok(()),
            Method::Docker => Err("docker não encontrado no PATH.".to_string()),
            Method::Mise => Err("o mise não é suportado no Windows; use `official`.".to_string()),
            Method::Distro => {
                Err("não há gerenciador de distro no Windows; use `official` (winget).".to_string())
            }
        }
    }

    /// **O quê:** roda o passo no PowerShell herdando o terminal (saída ao vivo, winget/UAC).
    /// Nota é no-op. **Onde:** `run_steps` das ações de instalar/remover.
    fn exec_step(&self, plat: &Plataforma, s: &Step) -> Result<(), String> {
        if s.source == "nota" {
            return Ok(());
        }
        let status =
            plat.shell.run_herdado(&s.cmd).map_err(|e| format!("falha ao executar shell: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("comando falhou ({status}): {}", s.cmd))
        }
    }

    /// **O quê:** [`toolchain::garantir_toolchain`]. **Onde:** build do fonte.
    fn garantir_toolchain(&self, plat: &Plataforma) -> Result<PathBuf, String> {
        toolchain::garantir_toolchain(plat)
    }

    /// **O quê:** [`toolchain::garantir_build_deps`]. **Onde:** build do fonte.
    fn garantir_build_deps(&self, plat: &Plataforma) -> Result<(), String> {
        toolchain::garantir_build_deps(plat)
    }

    /// **O quê:** `"<http_code> <redirect_url>"` de `releases/latest` (curl sem seguir redirect;
    /// no PowerShell, um HEAD que segue e devolve `302 <url final>`). **Onde:**
    /// `url_do_ultimo_release_tipado`.
    fn sondar_ultimo_release(&self, repo: &str) -> Option<String> {
        let url = format!("https://github.com/{repo}/releases/latest");
        capturar_ou_none(&cliente_atual(), &curl_sonda_release_args(&url), &ps_sonda_release(&url))
    }

    /// **O quê:** código HTTP de `url` pedindo só o 1º byte. **Onde:** `asset_existe`.
    fn sondar_asset(&self, url: &str) -> Option<String> {
        capturar_ou_none(&cliente_atual(), &curl_sonda_asset_args(url), &ps_sonda_asset(url))
    }

    /// **O quê:** GET com um cabeçalho extra; cabeçalho inválido → `None`. **Onde:** consulta de
    /// commit pela API (só sem `git`).
    fn texto_com_cabecalho(&self, url: &str, cabecalho: &str) -> Option<String> {
        let ps = ps_texto_com_cabecalho(url, cabecalho)?;
        capturar_ou_none(&cliente_atual(), &curl_texto_com_cabecalho_args(url, cabecalho), &ps)
    }
}

/// **O quê:** `capturar` com a falha vista como "rede fora" (`None`), que é o contrato das sondas.
/// **Onde:** as três sondas de `ReceitasWindows`.
fn capturar_ou_none(cli: &Cliente, curl: &[String], ps: &str) -> Option<String> {
    capturar(cli, curl, ps).ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O quê: métodos disponíveis no Windows. Onde: `method_reason`.
    #[test]
    fn metodos() {
        let r = ReceitasWindows { winget: true };
        let m = |docker| Machine { family: Family::Unknown, mise: false, docker };
        assert!(r.method_reason(&m(false), Method::Official).is_ok());
        assert!(r.method_reason(&m(true), Method::Docker).is_ok());
        assert!(r.method_reason(&m(false), Method::Docker).is_err());
        assert!(r.method_reason(&m(true), Method::Mise).is_err());
        assert!(r.method_reason(&m(true), Method::Distro).is_err());
        assert_eq!(r.family(), Family::Unknown);
    }
}
