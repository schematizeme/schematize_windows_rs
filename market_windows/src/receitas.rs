//! **receitas** — os planos de instalação/remoção do Windows (PURO: devolve `Recipe`).
//!
//! **O quê:** para cada linguagem e ferramenta do market, o comando PowerShell exato (um
//! `Step`) que o consentimento mostra e o `exec_step` roda. Runtimes e VS Code vêm do `winget`
//! (ids verificados no repositório `microsoft/winget-pkgs`); Rust vem do `rustup-init.exe`;
//! `claude`/`cursor` usam os instaladores PowerShell oficiais; `codex` é `npm -g` (sem admin).
//! Sem `winget`, o plano vira `Na` com a instrução de instalar o "App Installer".
//!
//! **Onde:** `ReceitasWindows::{install_recipe, remove_recipe, tool_install_recipe,
//! tool_remove_recipe}`.

use market::environments::defs::{self, Env, Recipe, Step, Tool};
use market::environments::detect::Family;
use market::environments::Method;

/// Mensagem quando falta o `winget`.
pub const SEM_WINGET: &str = "o `winget` não foi encontrado. Instale o \"App Installer\" da \
Microsoft Store (https://aka.ms/getwinget) ou atualize o Windows 10/11, e rode de novo.";

/// Código de saída do winget para "já instalado / nada a atualizar" (`0x8A15002B`).
const WINGET_JA_INSTALADO: i64 = -1_978_335_189;

/// Prefixo dos passos PowerShell: erro fatal e TLS 1.2 (PowerShell 5.1).
const PRE: &str = "$ErrorActionPreference='Stop'; \
[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; ";

/// **O quê:** monta um `Step` PowerShell. **Onde:** todos os planos.
fn passo(cmd: String, fonte: &str, pipe_sh: bool) -> Step {
    Step { cmd, source: fonte.to_string(), sudo: false, pipe_sh }
}

/// **O quê:** `winget install` não interativo do pacote `id` (aceita o "já instalado"). PURO.
/// **Onde:** runtimes e VS Code.
pub fn winget_instalar(id: &str) -> String {
    format!(
        "winget install --id {id} -e --silent --accept-source-agreements \
         --accept-package-agreements; \
         if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne {WINGET_JA_INSTALADO}) \
         {{ exit $LASTEXITCODE }}"
    )
}

/// **O quê:** `winget uninstall` do pacote `id`. PURO. **Onde:** remoção de runtimes e VS Code.
pub fn winget_remover(id: &str) -> String {
    format!(
        "winget uninstall --id {id} -e --silent --accept-source-agreements; \
         if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}"
    )
}

/// **O quê:** o script que baixa o `rustup-init.exe` oficial e o roda (`-y --profile minimal`).
/// PURO. **Onde:** instalar Rust.
pub fn rustup_init() -> String {
    format!(
        "{PRE}$t = Join-Path $env:TEMP 'schematize-rustup-init.exe'; \
         Invoke-WebRequest -UseBasicParsing -Uri 'https://win.rustup.rs/x86_64' -OutFile $t; \
         & $t -y --profile minimal; $c = $LASTEXITCODE; Remove-Item -Force $t; \
         if ($c -ne 0) {{ exit $c }}"
    )
}

/// **O quê:** os ids do winget que instalam o runtime de `lang` (na ordem); vazio = sem id
/// confiável. PURA. **Onde:** [`instalar`] e [`remover`].
pub fn ids_winget(lang: &str) -> &'static [&'static str] {
    match lang {
        "go" => &["GoLang.Go"],
        "node" => &["OpenJS.NodeJS.LTS"],
        "csharp" => &["Microsoft.DotNet.SDK.10"],
        "zig" => &["zig.zig"],
        "ruby" => &["RubyInstallerTeam.RubyWithDevKit.3.4"],
        // O Elixir precisa do Erlang/OTP antes.
        "elixir" => &["Erlang.ErlangOTP", "Elixir.Elixir"],
        _ => &[],
    }
}

/// **O quê:** plano de instalação de `env` pelo `method`. `winget` diz se o winget existe. PURA.
/// **Onde:** `ReceitasWindows::install_recipe`.
pub fn instalar(env: &Env, method: Method, winget: bool) -> Recipe {
    match method {
        Method::Docker => defs::install_recipe(env, Method::Docker, Family::Unknown, false),
        Method::Mise => Recipe::Na(format!(
            "o método mise não é suportado no Windows; use `official` para {}.",
            env.display
        )),
        Method::Distro => Recipe::Na(format!(
            "no Windows não há pacotes de distro; use `official` (winget) para {}.",
            env.display
        )),
        Method::Official if env.lang == "rust" => Recipe::Steps(vec![passo(
            rustup_init(),
            "https://win.rustup.rs (rustup-init oficial)",
            false,
        )]),
        Method::Official => {
            let ids = ids_winget(env.lang);
            if ids.is_empty() {
                return Recipe::Na(format!("sem pacote winget mapeado para {}.", env.display));
            }
            if !winget {
                return Recipe::Na(SEM_WINGET.to_string());
            }
            Recipe::Steps(
                ids.iter()
                    .map(|id| passo(winget_instalar(id), &format!("winget: {id}"), false))
                    .collect(),
            )
        }
    }
}

/// **O quê:** plano de remoção de `env`. PURA. **Onde:** `ReceitasWindows::remove_recipe`.
pub fn remover(env: &Env, method: Method, winget: bool) -> Recipe {
    match method {
        Method::Docker => defs::remove_recipe(env, Method::Docker, Family::Unknown),
        Method::Mise | Method::Distro => {
            Recipe::Na(format!("método não suportado no Windows para {}.", env.display))
        }
        Method::Official if env.lang == "rust" => Recipe::Steps(vec![passo(
            "rustup self uninstall -y".to_string(),
            "rustup (desinstalador oficial)",
            false,
        )]),
        Method::Official => {
            let ids = ids_winget(env.lang);
            if ids.is_empty() {
                return Recipe::Na(format!("sem pacote winget mapeado para {}.", env.display));
            }
            if !winget {
                return Recipe::Na(SEM_WINGET.to_string());
            }
            Recipe::Steps(
                ids.iter()
                    .rev()
                    .map(|id| passo(winget_remover(id), &format!("winget: {id}"), false))
                    .collect(),
            )
        }
    }
}

/// **O quê:** plano de instalação de uma ferramenta de dev. PURA.
/// **Onde:** `ReceitasWindows::tool_install_recipe`.
pub fn ferramenta_instalar(tool: &Tool, winget: bool) -> Recipe {
    match tool.slug {
        "claude" => Recipe::Steps(vec![passo(
            format!("{PRE}irm https://claude.ai/install.ps1 | iex"),
            "https://claude.ai/install.ps1 (instalador oficial Anthropic)",
            true,
        )]),
        "cursor" => Recipe::Steps(vec![passo(
            format!("{PRE}irm 'https://cursor.com/install?win32=true' | iex"),
            "https://cursor.com/install?win32=true (instalador oficial Cursor)",
            true,
        )]),
        "codex" => Recipe::Steps(vec![passo(
            "npm install -g @openai/codex; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }"
                .to_string(),
            "npmjs.com (@openai/codex)",
            false,
        )]),
        "code" if winget => Recipe::Steps(vec![passo(
            winget_instalar("Microsoft.VisualStudioCode"),
            "winget: Microsoft.VisualStudioCode",
            false,
        )]),
        "code" => Recipe::Na(SEM_WINGET.to_string()),
        outro => Recipe::Na(format!("sem receita Windows para a ferramenta `{outro}`.")),
    }
}

/// **O quê:** plano de remoção de uma ferramenta de dev. PURA.
/// **Onde:** `ReceitasWindows::tool_remove_recipe`.
pub fn ferramenta_remover(tool: &Tool, winget: bool) -> Recipe {
    match tool.slug {
        "claude" => Recipe::Steps(vec![passo(
            "Remove-Item -Force -ErrorAction SilentlyContinue \
             \"$env:USERPROFILE\\.local\\bin\\claude.exe\"; \
             Remove-Item -Recurse -Force -ErrorAction SilentlyContinue \
             \"$env:USERPROFILE\\.local\\share\\claude\""
                .to_string(),
            "instalador oficial (remove o binário e os dados de versão)",
            false,
        )]),
        "cursor" => Recipe::Steps(vec![passo(
            "Remove-Item -Recurse -Force -ErrorAction SilentlyContinue \
             \"$env:LOCALAPPDATA\\cursor-agent\""
                .to_string(),
            "instalador oficial (remove %LOCALAPPDATA%\\cursor-agent)",
            false,
        )]),
        "codex" => Recipe::Steps(vec![passo(
            "npm uninstall -g @openai/codex; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }"
                .to_string(),
            "npmjs.com (@openai/codex)",
            false,
        )]),
        "code" if winget => Recipe::Steps(vec![passo(
            winget_remover("Microsoft.VisualStudioCode"),
            "winget: Microsoft.VisualStudioCode",
            false,
        )]),
        "code" => Recipe::Na(SEM_WINGET.to_string()),
        outro => Recipe::Na(format!("sem receita Windows para a ferramenta `{outro}`.")),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn comandos(r: Recipe) -> Vec<String> {
        match r {
            Recipe::Steps(v) => v.into_iter().map(|s| s.cmd).collect(),
            Recipe::Todo(m) | Recipe::Na(m) => vec![format!("NA:{m}")],
        }
    }

    /// O quê: todo runtime do market tem plano oficial (winget/rustup) ou `Na` explícito.
    /// Onde: `instalar`.
    #[test]
    fn todo_runtime_tem_plano() {
        for e in defs::ENVS {
            let c = comandos(instalar(e, Method::Official, true));
            assert!(!c[0].starts_with("NA:"), "{}: {c:?}", e.lang);
            if e.lang != "rust" {
                assert!(c[0].contains("winget install --id"), "{}", e.lang);
            }
        }
    }

    /// O quê: ids exatos (verificados em winget-pkgs). Onde: `ids_winget`.
    #[test]
    fn ids_verificados() {
        assert_eq!(ids_winget("go"), ["GoLang.Go"]);
        assert_eq!(ids_winget("node"), ["OpenJS.NodeJS.LTS"]);
        assert_eq!(ids_winget("elixir"), ["Erlang.ErlangOTP", "Elixir.Elixir"]);
        assert!(ids_winget("rust").is_empty() && ids_winget("php").is_empty());
    }

    /// O quê: sem winget a instrução é acionável; Rust não depende do winget. Onde: `instalar`.
    #[test]
    fn sem_winget() {
        let go = defs::find("go").expect("go");
        assert_eq!(comandos(instalar(go, Method::Official, false)), [format!("NA:{SEM_WINGET}")]);
        let rust = defs::find("rust").expect("rust");
        assert!(comandos(instalar(rust, Method::Official, false))[0].contains("win.rustup.rs"));
    }

    /// O quê: métodos de Unix são `Na`; docker delega a `defs`. Onde: `instalar`.
    #[test]
    fn metodos_nao_windows() {
        let go = defs::find("go").expect("go");
        assert!(comandos(instalar(go, Method::Mise, true))[0].starts_with("NA:"));
        assert!(comandos(instalar(go, Method::Distro, true))[0].starts_with("NA:"));
        assert!(comandos(instalar(go, Method::Docker, true))[0].starts_with("docker pull"));
    }

    /// O quê: o Elixir remove na ordem inversa (Elixir antes do Erlang). Onde: `remover`.
    #[test]
    fn remocao_em_ordem_inversa() {
        let ex = defs::find("elixir").expect("elixir");
        let c = comandos(remover(ex, Method::Official, true));
        assert!(c[0].contains("Elixir.Elixir") && c[1].contains("Erlang.ErlangOTP"));
        let rust = defs::find("rust").expect("rust");
        assert_eq!(comandos(remover(rust, Method::Official, true)), ["rustup self uninstall -y"]);
    }

    /// O quê: ferramentas: claude/cursor são `irm | iex` com pipe_sh; codex npm; code winget.
    /// Onde: `ferramenta_instalar`.
    #[test]
    fn ferramentas() {
        for t in defs::TOOLS {
            let r = ferramenta_instalar(t, true);
            assert!(matches!(r, Recipe::Steps(_)), "{}", t.slug);
            assert!(matches!(ferramenta_remover(t, true), Recipe::Steps(_)), "{}", t.slug);
        }
        let claude = defs::find_tool("claude").expect("claude");
        match ferramenta_instalar(claude, true) {
            Recipe::Steps(v) => {
                assert!(
                    v[0].pipe_sh && v[0].cmd.ends_with("irm https://claude.ai/install.ps1 | iex")
                );
            }
            _ => panic!("esperava Steps"),
        }
        let code = defs::find_tool("code").expect("code");
        assert!(comandos(ferramenta_instalar(code, false))[0].starts_with("NA:"));
        let cursor = defs::find_tool("cursor").expect("cursor");
        assert!(comandos(ferramenta_instalar(cursor, false))[0].contains("install?win32=true"));
    }

    /// O quê: o comando do winget aceita "já instalado" e propaga falha real. Onde: `winget_instalar`.
    #[test]
    fn winget_exit_codes() {
        let c = winget_instalar("X.Y");
        assert!(c.contains("-1978335189") && c.contains("exit $LASTEXITCODE"));
        assert!(c.contains("--accept-package-agreements"));
    }
}
