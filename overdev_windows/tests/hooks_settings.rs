//! Prova com as funções REAIS de `overdev_casa::settings` que o `HooksWindows`:
//! - `enable` duas vezes não duplica o grupo (as `MARCAS` por substring reconhecem o comando);
//! - `disable` remove todos os grupos;
//! - `hooks_atualizados` fecha o ciclo e `refresh_hooks` é no-op quando já está atualizado.
//!
//! UM só `#[test]` porque `settings` lê o HOME do ambiente (variável global do processo).

use overdev_casa::hooks::Hooks;
use overdev_casa::settings;
use overdev_windows::HooksWindows;
use std::path::Path;

/// Quantos grupos de `hooks.<evento>` têm um comando contendo `marca`.
fn grupos(settings_json: &serde_json::Value, evento: &str, marca: &str) -> usize {
    settings_json["hooks"][evento].as_array().map_or(0, |a| {
        a.iter()
            .filter(|g| {
                g["hooks"].as_array().is_some_and(|hs| {
                    hs.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains(marca)))
                })
            })
            .count()
    })
}

#[test]
fn enable_idempotente_disable_remove() {
    let home = std::env::temp_dir().join(format!("ow_hooks_{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("home de teste");
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);
    let arq = home.join(".claude").join("settings.json");
    let ler = || -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(&arq).expect("settings.json")).expect("json")
    };
    let h = HooksWindows::com_dir(Some(Path::new("C:\\Users\\ana\\.cargo\\bin")));

    settings::enable(&h).expect("enable 1");
    settings::enable(&h).expect("enable 2");
    let j = ler();
    assert_eq!(grupos(&j, "Stop", "overdev check"), 1, "duplicou o Stop: {j}");
    assert_eq!(grupos(&j, "PreToolUse", "overdev guard"), 1, "duplicou o guard: {j}");
    assert!(settings::overdev_enabled());
    assert!(settings::hooks_atualizados(&h));
    assert!(!settings::refresh_hooks(&h).expect("refresh"), "já atualizado: não regrava");

    // Migração: o hook de um caminho antigo (outro HOME) é SUBSTITUÍDO, não duplicado.
    let velho = HooksWindows::com_dir(Some(Path::new("D:\\Old\\bin")));
    settings::enable(&velho).expect("enable velho");
    assert!(!settings::hooks_atualizados(&h));
    assert!(settings::refresh_hooks(&h).expect("refresh"), "deve regravar");
    let j = ler();
    assert_eq!(grupos(&j, "Stop", "overdev check"), 1, "{j}");
    assert_eq!(grupos(&j, "Stop", &h.comando("check")), 1, "{j}");
    assert_eq!(grupos(&j, "Stop", "D:/Old"), 0, "{j}");

    settings::disable().expect("disable");
    let j = ler();
    assert_eq!(grupos(&j, "Stop", "overdev check"), 0, "{j}");
    assert_eq!(grupos(&j, "PreToolUse", "overdev guard"), 0, "{j}");
    assert!(!settings::overdev_enabled());

    // Hook alheio no mesmo arquivo sobrevive a enable + disable.
    std::fs::write(
        &arq,
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo meu-hook"}]}]}}"#,
    )
    .expect("settings alheio");
    settings::enable(&h).expect("enable");
    settings::disable().expect("disable");
    assert_eq!(grupos(&ler(), "Stop", "meu-hook"), 1);
    std::fs::remove_dir_all(&home).expect("limpar");
}
