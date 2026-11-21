# BUDGET: orçamento de memória dos executáveis Windows

Piso de eficiência de runtime (`skill_engineering_src/references/eficiencia-runtime.md`, §1).

**Estado: orçamento DECLARADO, ainda NÃO MEDIDO.** Este repo é cola Windows (winget, PowerShell,
`rustup-init`): ~3.850 linhas que só rodam em Windows, e o toolchain `x86_64-pc-windows-gnu` não
está instalado na máquina de desenvolvimento (Linux). Não há número aqui porque não há como
medir sem Windows; nenhum valor abaixo foi inventado.

| Operação | RSS pico alvo (teto) | RSS medido antes/depois | Como medir (no Windows) |
|---|---|---|---|
| `schematize-market --version` / `--help` | 16 MiB | não medido: precisa Windows | `Get-Process -Id <pid>` `PeakWorkingSet64` em laço curto, ou `Measure-Command` + `(Get-Process).PeakWorkingSet64` |
| `schematize-market status` / `list --json` | 48 MiB | não medido: precisa Windows | idem |
| `schematize-overdev` (CLI) | 32 MiB | não medido: precisa Windows | idem |
| `schematize-overdev-gui` (feature `gui`, iced) | 400 MiB (máximo da casa para GUI) | não medido: precisa Windows e display | idem |

Referência: o mesmo código de negócio do market, no Linux, mede 3,8 MiB (`--version`) e 16 MiB
(`status`) de pico; ver `schematize_market_rs/BUDGET.md`, que tem o teste (`tests/orcamento_rss.rs`)
que faz o teto valer. O equivalente Windows precisa de `GetProcessMemoryInfo` (crate
`windows-sys` já presente) e fica como pendência: não foi escrito aqui porque não pode ser
compilado nem provado nesta máquina, e teste não verificado é pior que teste ausente.

## Dependências (análise estática, sem build)

- Sem dependência própria além de `schematize_plataforma` (features `base`+`seams`, as mínimas
  que os adaptadores usam), `market` e `overdev_casa` (este com `default-features = false`).
- O `Cargo.lock` tem 333 pacotes porque resolve também a GUI (iced) e todos os alvos; o build
  lógico sem a feature `gui` é o do market (~41 entradas, 1 duplicata dentro do `png`).
- `[profile.release]` idêntico ao dos repos irmãos (cache compartilhado do `install.sh`); não
  alterado.
