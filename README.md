# schematize_windows_rs

Os executáveis **Windows** do ecossistema schematize: `schematize-market.exe`,
`schematize-overdev.exe` e `schematize-overdev-gui.exe` (mais a janela do market, que vem do
repo `schematize-updater-gui`).

## O que este repo é (ADR-0026)

Os **cores** (`schematize_market_rs`, `schematize_overdev_rs`) são **Unix** e falam com o SO
só por traits (`Plataforma` de `schematize_plataforma_rs`, `Receitas` do market, `Hooks` do
overdev). Tudo o que é específico do Windows mora **aqui** — e só aqui:

| crate | o que é |
|---|---|
| `plataforma_windows` | as 8 costuras: PowerShell, `curl.exe`, `.old`/`taskkill`, atalho `.lnk`, PATH no registro, `wt.exe`/Git Bash, `explorer.exe`, `Get-CimInstance` |
| `market_windows` | `ReceitasWindows`: runtimes por `winget`/`rustup-init`, ferramentas (claude, codex, cursor, VS Code), toolchain Rust + MSVC |
| `overdev_windows` | `HooksWindows`: o comando de hook que funciona em Git Bash, cmd e PowerShell |
| `bins` | os `main` (`Plataforma::windows()` + core) |

A lógica (montar comando, parsear saída, decidir) é **pura** e testada no Linux; só o *spawn*
toca o SO. Cada função tem doc **O quê / Onde**.

## Instalar

```powershell
irm https://raw.githubusercontent.com/schematizeme/schematize_windows_rs/main/install.ps1 | iex
```

Com o kit completo (instala também linguagens e ferramentas: `schematize-market install --all -y`):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/schematizeme/schematize_windows_rs/main/install.ps1))) -Completo
```

Sem administrador. Instala em `%USERPROFILE%\.cargo\bin`, confere o SHA-256 (`SHA256SUMS` do
release), testa `--version` e põe a pasta no PATH **do usuário** (idempotente). Windows
PowerShell 5.1 ou 7. Abra um terminal novo depois.

## Estratégia por costura

| costura | Windows |
|---|---|
| `Shell` | `powershell.exe -NoProfile -NonInteractive -Command`; `executar` por argv (resolve `PATH`+`PATHEXT`); `existe` por `PATH`+`PATHEXT` |
| `Rede` | `curl.exe` (Win10 1803+), senão `Invoke-WebRequest` (TLS 1.2); `.part` + rename; progresso quando stderr é terminal |
| `Substituicao` | `.exe` em uso é renomeado para `<nome>.old`, o novo entra, `.old` é apagado na hora ou na próxima execução; `encerrar_gui` = `taskkill /F /IM` |
| `Atalhos` | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Schematize\<App>.lnk` (COM `WScript.Shell`) + `.ico` gravado dos PNGs |
| `PathSetup` | `HKCU\Environment\Path` (`REG_EXPAND_SZ`) + `WM_SETTINGCHANGE`; idempotente |
| `Terminal` | `wt.exe` ou `cmd /c start`; o script bash roda no **Git Bash** (exigido também pelo Claude Code); sem Git Bash só o `exec <cmd>` do market é traduzido para PowerShell |
| `Abridor` | `explorer.exe <alvo>` direto (nenhum shell reinterpreta o alvo) |
| `Processos` | `tasklist` + `Get-CimInstance Win32_Process`; RAM por `Win32_OperatingSystem` |

## Limitações (conhecidas, documentadas)

- **cwd de outro processo:** o Windows não o expõe sem API privilegiada. `Processos::em_dir` e
  `pids_por_argv_em` aproximam por "a linha de comando cita o diretório" (falso negativo possível,
  nunca falso positivo). `carga1` é sempre `None` (não há load average).
- **Hook do overdev:** o comando é `C:/Users/<u>/.cargo/bin/schematize-overdev check` — sem
  aspas e sem `.exe` de propósito (ver o doc de `overdev_windows`). Se o caminho tem espaço, cai
  em `schematize-overdev check` pelo PATH.
- **Terminal sem Git Bash:** scripts além do `exec <cmd>` simples pedem o Git for Windows
  (`winget install --id Git.Git -e`).
- **Windows ARM64:** roda os `.exe` x64 por emulação (não há build nativo ainda).
- Parte do que fala com o SO (UAC do winget, COM do atalho, registro, `wt.exe`) só pode ser
  provada no CI Windows (`ci.yml`, `release.yml` → `e2e-windows`).

## Desenvolver

```sh
# no Linux/macOS: lógica pura
cargo test --workspace
# cross-check do alvo Windows (precisa de gcc/mingw para o rusqlite)
CC_x86_64_pc_windows_gnu=gcc AR_x86_64_pc_windows_gnu=ar \
  cargo check --target x86_64-pc-windows-gnu --workspace --all-targets --features bins/gui
# no Windows
cargo build --release --features bins/gui
```

As dependências `schematize_plataforma`, `schematize-market` e `schematize-overdev` estão como
`path = "../…"` com a marca `# TEMP(ADR-0026): trocar por git + tag ao publicar`
(plataforma `v0.2.1`, market `v0.6.0`, overdev `v0.4.0`). O CI clona os irmãos nessas tags
(`.github/scripts/clonar-irmaos.sh`, também TEMP).

## Release

Tag `v*` → `release.yml` compila, publica `schematize-{market,market-gui,overdev,overdev-gui}-windows-x86_64.exe`
+ `SHA256SUMS` e roda o `e2e-windows` (instala com o `install.ps1` da tag, liga o hook do overdev
num `USERPROFILE` temporário e executa o comando gerado por cmd, PowerShell e Git Bash).

Licença: MIT.
