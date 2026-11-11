//! Imprimir no Windows: sem CUPS, as páginas desenhadas vão para a impressora pelo GDI
//! (`StartDoc`/`StretchDIBits`), em faixas para não montar a folha inteira na memória.
//! A ordem das folhas, as cópias e o encaixe ficam fora do `cfg(windows)` para os testes.

/// Folhas na ordem em que saem: cada uma com até `por_folha` páginas. Agrupar repete o jogo
/// inteiro (1, 2, 3… 1, 2, 3); sem agrupar, cada folha sai `copias` vezes seguidas.
pub(crate) fn folhas(paginas: &[u32], por_folha: u32, copias: u32, agrupar: bool, inversa: bool) -> Vec<Vec<u32>> {
    let mut jogo: Vec<Vec<u32>> = paginas.chunks(por_folha.max(1) as usize).map(<[u32]>::to_vec).collect();
    if inversa { jogo.reverse(); }
    let copias = copias.max(1) as usize;
    if agrupar { std::iter::repeat_n(jogo, copias).flatten().collect() }
    else { jogo.into_iter().flat_map(|f| std::iter::repeat_n(f, copias)).collect() }
}

/// Colunas × linhas de páginas numa folha (em pé; deitada troca).
pub(crate) fn grade(por_folha: u32, deitada: bool) -> (u32, u32) {
    let (c, l) = match por_folha { 2 => (1, 2), 4 => (2, 2), 6 => (2, 3), 9 => (3, 3), _ => (1, 1) };
    if deitada { (l, c) } else { (c, l) }
}

/// Pixels do aparelho por ponto do PDF e o canto de cima da página centrada na célula
/// `(largura, altura)` em pixels. Tamanho real: 1 pt = 1/72 pol na resolução `dpi`.
pub(crate) fn encaixe(celula: (f32, f32), pagina: (f32, f32), ajustar: bool, dpi: f32) -> (f32, f32, f32) {
    let s = if ajustar { (celula.0 / pagina.0).min(celula.1 / pagina.1) } else { dpi / 72.0 };
    (s, (celula.0 - pagina.0 * s) / 2.0, (celula.1 - pagina.1 * s) / 2.0)
}

#[cfg(windows)]
pub(crate) use sistema::{impressoras, imprimir};

#[cfg(windows)]
mod sistema {
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Graphics::Printing::*;
    use windows_sys::Win32::Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};

    use super::{encaixe, folhas, grade};
    use crate::imprimir::{Lados, Orientacao, Papel};

    fn largo(s: &str) -> Vec<u16> { s.encode_utf16().chain([0]).collect() }

    fn de_largo(p: *const u16) -> String {
        if p.is_null() { return String::new(); }
        // SAFETY: texto terminado em zero que o spooler devolveu.
        unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(p, (0..).take_while(|&i| *p.add(i) != 0).count())) }
    }

    /// Impressoras instaladas (locais e de rede) e a padrão do Windows.
    pub(crate) fn impressoras() -> (Vec<String>, Option<String>) {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut preciso, mut quantas) = (0u32, 0u32);
        // SAFETY: primeiro só o tamanho; depois o buffer desse tamanho, lido como PRINTER_INFO_4W.
        let lista = unsafe {
            EnumPrintersW(flags, std::ptr::null(), 4, std::ptr::null_mut(), 0, &mut preciso, &mut quantas);
            let mut buf = vec![0u64; (preciso as usize).div_ceil(8).max(1)];
            if EnumPrintersW(flags, std::ptr::null(), 4, buf.as_mut_ptr().cast(), preciso, &mut preciso, &mut quantas) == 0 { Vec::new() }
            else { std::slice::from_raw_parts(buf.as_ptr().cast::<PRINTER_INFO_4W>(), quantas as usize).iter().map(|i| de_largo(i.pPrinterName)).collect() }
        };
        let mut tamanho = 0u32;
        // SAFETY: mesmo esquema: tamanho, depois o nome.
        let padrao = unsafe {
            GetDefaultPrinterW(std::ptr::null_mut(), &mut tamanho);
            let mut nome = vec![0u16; tamanho.max(1) as usize];
            (GetDefaultPrinterW(nome.as_mut_ptr(), &mut tamanho) != 0).then(|| de_largo(nome.as_ptr()))
        };
        let padrao = padrao.filter(|p| lista.contains(p));
        (lista, padrao)
    }

    /// As preferências da impressora com frente e verso, cor e orientação escolhidos.
    fn preferencias(nome: &[u16], lados: Lados, cinza: bool, papel: Papel) -> Result<Vec<u64>, String> {
        let mut impressora = PRINTER_HANDLE::default();
        // SAFETY: nome terminado em zero; o buffer tem o tamanho que o driver pediu (com o extra dele).
        let buf = unsafe {
            if OpenPrinterW(nome.as_ptr(), &mut impressora, std::ptr::null()) == 0 { return Err("Não consegui abrir a impressora.".into()); }
            let tamanho = DocumentPropertiesW(std::ptr::null_mut(), impressora, nome.as_ptr(), std::ptr::null_mut(), std::ptr::null(), 0);
            if tamanho <= 0 { ClosePrinter(impressora); return Err("O driver da impressora não respondeu.".into()); }
            let mut buf = vec![0u64; (tamanho as usize).div_ceil(8)];
            let dm = buf.as_mut_ptr().cast::<DEVMODEW>();
            DocumentPropertiesW(std::ptr::null_mut(), impressora, nome.as_ptr(), dm, std::ptr::null(), DM_OUT_BUFFER);
            (*dm).dmFields |= DM_DUPLEX | DM_COLOR | DM_COPIES | DM_ORIENTATION;
            (*dm).dmDuplex = match lados { Lados::Um => DMDUP_SIMPLEX, Lados::BordaLonga => DMDUP_VERTICAL, Lados::BordaCurta => DMDUP_HORIZONTAL };
            (*dm).dmColor = if cinza { DMCOLOR_MONOCHROME } else { DMCOLOR_COLOR };
            if let Some(n) = papel.windows() {
                (*dm).dmFields |= DM_PAPERSIZE;
                (*dm).Anonymous1.Anonymous1.dmPaperSize = n;
            }
            // As cópias saem uma a uma daqui (com a ordem certa de agrupar), não pelo driver.
            (*dm).Anonymous1.Anonymous1.dmCopies = 1;
            DocumentPropertiesW(std::ptr::null_mut(), impressora, nome.as_ptr(), dm, dm, DM_IN_BUFFER | DM_OUT_BUFFER);
            ClosePrinter(impressora);
            buf
        };
        Ok(buf)
    }

    struct Dc(HDC);
    impl Drop for Dc { fn drop(&mut self) { unsafe { DeleteDC(self.0); } } }

    pub(crate) struct Pedido {
        pub impressora: String,
        pub paginas: Vec<u32>,
        pub copias: u32,
        pub agrupar: bool,
        pub lados: Lados,
        pub orientacao: Orientacao,
        pub papel: Papel,
        pub cinza: bool,
        pub ajustar: bool,
        pub por_folha: u32,
        pub inversa: bool,
    }

    /// Bloqueante: desenha cada folha e manda para a impressora.
    pub(crate) fn imprimir(vis: &featherpdf_io::Visualizador, tamanhos: &[(f32, f32)], p: Pedido) -> Result<String, String> {
        let nome = largo(&p.impressora);
        let mut dm = preferencias(&nome, p.lados, p.cinza, p.papel)?;
        let devmode = dm.as_mut_ptr().cast::<DEVMODEW>();
        let tamanho = |n: u32| tamanhos.get(n as usize - 1).copied().unwrap_or((595.0, 842.0));
        let deitar = |folha: &[u32]| match p.orientacao {
            Orientacao::Retrato => false,
            Orientacao::Paisagem => true,
            // Uma por folha: segue a página. Várias: em pé (a grade de 2 e 6 já fica boa assim).
            Orientacao::Automatica => p.por_folha <= 1 && folha.first().is_some_and(|&n| { let (l, a) = tamanho(n); l > a }),
        };
        let lista = folhas(&p.paginas, p.por_folha, p.copias, p.agrupar, p.inversa);
        let primeira = lista.first().ok_or("Nenhuma página para imprimir.")?;
        // SAFETY: devmode aponta para o buffer `dm`, vivo até o fim da função.
        unsafe { (*devmode).Anonymous1.Anonymous1.dmOrientation = if deitar(primeira) { DMORIENT_LANDSCAPE } else { DMORIENT_PORTRAIT } as i16; }
        let winspool = largo("WINSPOOL");
        // SAFETY: textos terminados em zero e o DEVMODE validado pelo driver.
        let dc = Dc(unsafe { CreateDCW(winspool.as_ptr(), nome.as_ptr(), std::ptr::null(), devmode) });
        if dc.0.is_null() { return Err("Não consegui falar com a impressora.".into()); }
        let titulo = largo("FeatherPDF");
        let doc = DOCINFOW { cbSize: size_of::<DOCINFOW>() as i32, lpszDocName: titulo.as_ptr(), ..Default::default() };
        if unsafe { StartDocW(dc.0, &doc) } <= 0 { return Err("A impressora recusou o trabalho.".into()); }
        let r = (|| -> Result<(), String> {
            for folha in &lista {
                let deitada = deitar(folha);
                // SAFETY: troca a orientação entre páginas (fora de StartPage/EndPage), como o GDI pede.
                unsafe {
                    (*devmode).Anonymous1.Anonymous1.dmOrientation = if deitada { DMORIENT_LANDSCAPE } else { DMORIENT_PORTRAIT } as i16;
                    ResetDCW(dc.0, devmode);
                    if StartPage(dc.0) <= 0 { return Err("A impressora parou no meio.".into()); }
                }
                let (lw, lh) = unsafe { (GetDeviceCaps(dc.0, HORZRES as i32) as f32, GetDeviceCaps(dc.0, VERTRES as i32) as f32) };
                let dpi = unsafe { GetDeviceCaps(dc.0, LOGPIXELSX as i32) }.max(72) as f32;
                let (cols, linhas) = grade(p.por_folha, lw > lh);
                let (cw, ch) = (lw / cols as f32, lh / linhas as f32);
                for (i, &n) in folha.iter().enumerate() {
                    let (cx, cy) = ((i as u32 % cols) as f32 * cw, (i as u32 / cols) as f32 * ch);
                    let (s, dx, dy) = encaixe((cw, ch), tamanho(n), p.ajustar, dpi);
                    desenhar(vis, &dc, n, tamanho(n), s, (cx + dx, cy + dy), dpi, p.cinza)?;
                }
                if unsafe { EndPage(dc.0) } <= 0 { return Err("A impressora parou no meio.".into()); }
            }
            Ok(())
        })();
        match r {
            Ok(()) => { unsafe { EndDoc(dc.0); } Ok(format!("Enviado para {} ({} folha(s)).", p.impressora, lista.len())) }
            Err(e) => { unsafe { AbortDoc(dc.0); } Err(e) }
        }
    }

    /// Uma página em faixas de até 1024 linhas: a 600 dpi uma A4 inteira passaria de 100 MB.
    #[allow(clippy::too_many_arguments)]
    fn desenhar(vis: &featherpdf_io::Visualizador, dc: &Dc, n: u32, (pl, pa): (f32, f32), s: f32, (x, y): (f32, f32), dpi: f32, cinza: bool) -> Result<(), String> {
        // Até 300 dpi no desenho; a impressora amplia o resto (texto a 600 dpi não muda à vista, o tempo sim).
        let r = s * (300.0 / dpi).min(1.0);
        let (rw, rh) = ((pl * r).ceil() as u32, (pa * r).ceil() as u32);
        let k = s / r;
        const FAIXA: u32 = 1024;
        for y0 in (0..rh).step_by(FAIXA as usize) {
            let mut f = vis.desenhar_trecho(n, r, [0, y0, rw, FAIXA.min(rh - y0)])?;
            for px in f.rgba.chunks_exact_mut(4) {
                if cinza { let g = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) as u8; px[..3].fill(g); }
                px.swap(0, 2);
            }
            let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: f.largura as i32, biHeight: -(f.altura as i32),
                biPlanes: 1, biBitCount: 32, biCompression: BI_RGB, ..Default::default() }, ..Default::default() };
            let (dy, dh) = ((y + y0 as f32 * k).round() as i32, (f.altura as f32 * k).round() as i32);
            // SAFETY: f.rgba tem largura × altura × 4 bytes, como o cabeçalho diz.
            let ok = unsafe { StretchDIBits(dc.0, x.round() as i32, dy, (f.largura as f32 * k).round() as i32, dh,
                0, 0, f.largura as i32, f.altura as i32, f.rgba.as_ptr().cast(), &bmi, DIB_RGB_COLORS, SRCCOPY) };
            if ok == 0 { return Err(format!("Não consegui mandar a página {n} para a impressora.")); }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) use sistema::Pedido;

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ordem_das_folhas_com_copias_e_inversa() {
        assert_eq!(folhas(&[1, 2, 3], 1, 2, true, false), [vec![1], vec![2], vec![3], vec![1], vec![2], vec![3]]);
        assert_eq!(folhas(&[1, 2, 3], 1, 2, false, false), [vec![1], vec![1], vec![2], vec![2], vec![3], vec![3]]);
        assert_eq!(folhas(&[1, 2, 3], 2, 1, true, true), [vec![3], vec![1, 2]]);
    }

    #[test]
    fn grade_e_encaixe() {
        assert_eq!(grade(2, false), (1, 2));
        assert_eq!(grade(2, true), (2, 1));
        assert_eq!(grade(6, false), (2, 3));
        // A4 em pé numa célula de 1000 × 1000: a altura manda e a página fica centrada.
        let (s, dx, dy) = encaixe((1000.0, 1000.0), (595.0, 842.0), true, 600.0);
        assert!((842.0 * s - 1000.0).abs() < 0.01 && dy.abs() < 0.01 && dx > 0.0);
        assert_eq!(encaixe((1000.0, 1000.0), (72.0, 72.0), false, 600.0).0, 600.0 / 72.0);
    }
}
