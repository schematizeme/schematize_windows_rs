//! **ico** — escreve um `.ico` mínimo a partir de PNGs (std-only).
//!
//! **O quê:** o formato ICO aceita imagens PNG embutidas (Windows Vista+): um cabeçalho de 6
//! bytes, uma entrada de 16 bytes por imagem e depois os PNGs crus. Nada de BMP/máscara. Largura
//! e altura vêm do `IHDR` do próprio PNG (não de quem chama); 256 é gravado como 0.
//!
//! **Onde:** `atalhos` (ícone do atalho do Menu Iniciar). PURA — roda e é testada no Linux.

/// Assinatura de 8 bytes de todo PNG.
const ASSINATURA_PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Lado máximo de uma imagem num ICO.
pub const LADO_MAX: u32 = 256;

/// **O quê:** `(largura, altura)` do `IHDR` de um PNG, ou `Err` se não for PNG válido. PURA.
///
/// **Onde:** [`ico_de_pngs`].
pub fn dimensoes_png(png: &[u8]) -> Result<(u32, u32), String> {
    if png.len() < 24 || png[..8] != ASSINATURA_PNG {
        return Err("não é um PNG (assinatura ausente ou arquivo curto)".to_string());
    }
    if &png[12..16] != b"IHDR" {
        return Err("PNG sem IHDR logo após a assinatura".to_string());
    }
    let u32_be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    Ok((u32_be(16), u32_be(20)))
}

/// **O quê:** monta o `.ico` com todas as imagens `pngs` (`(px, bytes)`; o `px` é só rótulo — vale
/// o `IHDR`). Imagens maiores que 256 são puladas (limite do formato); se sobrar nenhuma, `Err`.
/// PNG inválido é `Err` (nunca vira ícone corrompido). PURA.
///
/// **Onde:** `AtalhosWindows::instalar`.
pub fn ico_de_pngs(pngs: &[(u32, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut itens: Vec<(u32, u32, &[u8])> = Vec::new();
    for (px, bytes) in pngs {
        let (w, h) = dimensoes_png(bytes).map_err(|e| format!("ícone de {px}px: {e}"))?;
        if w == 0 || h == 0 {
            return Err(format!("ícone de {px}px com dimensão zero"));
        }
        if w > LADO_MAX || h > LADO_MAX {
            continue;
        }
        itens.push((w, h, bytes));
    }
    if itens.is_empty() {
        return Err("nenhum PNG de até 256px para o ícone".to_string());
    }
    let n = u16::try_from(itens.len()).map_err(|_| "imagens demais para um ICO".to_string())?;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&n.to_le_bytes());
    let mut offset = 6u32 + 16 * u32::from(n);
    for (w, h, bytes) in &itens {
        let tam = u32::try_from(bytes.len()).map_err(|_| "PNG grande demais".to_string())?;
        out.push(if *w == LADO_MAX { 0 } else { *w as u8 });
        out.push(if *h == LADO_MAX { 0 } else { *h as u8 });
        out.extend_from_slice(&[0, 0]); // paleta, reservado
        out.extend_from_slice(&1u16.to_le_bytes()); // planos
        out.extend_from_slice(&32u16.to_le_bytes()); // bits por pixel
        out.extend_from_slice(&tam.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset = offset.checked_add(tam).ok_or_else(|| "ICO grande demais".to_string())?;
    }
    for (_, _, bytes) in &itens {
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// PNG falso mínimo: assinatura + chunk IHDR com `w`x`h` (o resto não importa aqui).
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = ASSINATURA_PNG.to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v.extend_from_slice(&[0, 0, 0, 0]); // crc (não validado)
        v
    }

    /// O quê: leitura das dimensões do IHDR. Onde: `dimensoes_png`.
    #[test]
    fn le_dimensoes() {
        assert_eq!(dimensoes_png(&png(48, 32)), Ok((48, 32)));
        assert!(dimensoes_png(b"nao png").is_err());
        let mut ruim = png(1, 1);
        ruim[12] = b'X';
        assert!(dimensoes_png(&ruim).is_err());
    }

    /// O quê: cabeçalho, entradas e offsets corretos; 256 vira 0. Onde: `ico_de_pngs`.
    #[test]
    fn estrutura_do_ico() {
        let a = png(16, 16);
        let b = png(256, 256);
        let ico = ico_de_pngs(&[(16, a.clone()), (256, b.clone())]).expect("ico");
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]);
        // entrada 0
        assert_eq!((ico[6], ico[7]), (16, 16));
        assert_eq!(u32::from_le_bytes([ico[14], ico[15], ico[16], ico[17]]), a.len() as u32);
        assert_eq!(u32::from_le_bytes([ico[18], ico[19], ico[20], ico[21]]), 6 + 32);
        // entrada 1: 256 -> 0
        assert_eq!((ico[22], ico[23]), (0, 0));
        assert_eq!(
            u32::from_le_bytes([ico[34], ico[35], ico[36], ico[37]]),
            6 + 32 + a.len() as u32
        );
        assert_eq!(ico.len(), 6 + 32 + a.len() + b.len());
        assert_eq!(&ico[38..38 + a.len()], a.as_slice());
    }

    /// O quê: >256 é pulado; só-grandes e inválidos são erro. Onde: `ico_de_pngs`.
    #[test]
    fn grandes_e_invalidos() {
        let ico = ico_de_pngs(&[(512, png(512, 512)), (32, png(32, 32))]).expect("ico");
        assert_eq!(ico[4], 1);
        assert!(ico_de_pngs(&[(512, png(512, 512))]).is_err());
        assert!(ico_de_pngs(&[(16, b"lixo".to_vec())]).is_err());
        assert!(ico_de_pngs(&[]).is_err());
        assert!(ico_de_pngs(&[(0, png(0, 0))]).is_err());
    }
}
