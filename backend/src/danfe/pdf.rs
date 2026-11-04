//! Motor de desenho mínimo sobre o pdf-writer: retângulos, linhas e texto com
//! quebra de linha e alinhamento, nas fontes padrão Helvetica (codificação WinAnsi).
//!
//! As coordenadas seguem a convenção de layout (origem no canto superior esquerdo,
//! y crescendo para baixo), convertidas para o PDF na hora de desenhar.

use super::metricas::{HELVETICA, HELVETICA_NEGRITO, Metricas};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

pub const LARGURA_A4: f32 = 595.28;
pub const ALTURA_A4: f32 = 841.89;

#[derive(Clone, Copy, PartialEq)]
pub enum Fonte {
    Normal,
    Negrito,
}

impl Fonte {
    fn metricas(self) -> &'static Metricas {
        match self {
            Fonte::Normal => &HELVETICA,
            Fonte::Negrito => &HELVETICA_NEGRITO,
        }
    }
    fn nome(self) -> Name<'static> {
        match self {
            Fonte::Normal => Name(b"F1"),
            Fonte::Negrito => Name(b"F2"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Alinhar {
    Esquerda,
    Centro,
    Direita,
}

pub type Cor = (f32, f32, f32);
pub const PRETO: Cor = (0.0, 0.0, 0.0);
pub const VERMELHO: Cor = (0.8, 0.0, 0.0);
pub const CINZA: Cor = (0.6, 0.6, 0.6);

/// Opções de texto.
#[derive(Clone, Copy)]
pub struct Txt {
    pub fonte: Fonte,
    pub tamanho: f32,
    pub largura: Option<f32>,
    pub alinhar: Alinhar,
    /// Quebra em várias linhas; sem quebra, o texto é cortado com reticências.
    pub quebrar: bool,
    pub altura: Option<f32>,
    pub cor: Cor,
}

impl Txt {
    pub fn new(fonte: Fonte, tamanho: f32) -> Self {
        Txt { fonte, tamanho, largura: None, alinhar: Alinhar::Esquerda, quebrar: true, altura: None, cor: PRETO }
    }
    pub fn largura(mut self, l: f32) -> Self {
        self.largura = Some(l);
        self
    }
    pub fn centro(mut self) -> Self {
        self.alinhar = Alinhar::Centro;
        self
    }
    pub fn alinhar(mut self, a: Alinhar) -> Self {
        self.alinhar = a;
        self
    }
    pub fn sem_quebra(mut self) -> Self {
        self.quebrar = false;
        self
    }
    pub fn altura(mut self, h: f32) -> Self {
        self.altura = Some(h);
        self
    }
    pub fn cor(mut self, c: Cor) -> Self {
        self.cor = c;
        self
    }
    fn altura_linha(&self) -> f32 {
        let m = self.fonte.metricas();
        (m.ascendente - m.descendente + m.espaco_entre_linhas) / 1000.0 * self.tamanho
    }
}

/// Converte para WinAnsi (Windows-1252); caracteres fora da tabela viram "?".
pub fn win_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c as u32 {
            0x20..=0x7E | 0xA0..=0xFF => c as u32 as u8,
            _ => match c {
                '€' => 0x80,
                '‚' => 0x82,
                'ƒ' => 0x83,
                '„' => 0x84,
                '…' => 0x85,
                '†' => 0x86,
                '‡' => 0x87,
                'ˆ' => 0x88,
                '‰' => 0x89,
                'Š' => 0x8A,
                '‹' => 0x8B,
                'Œ' => 0x8C,
                'Ž' => 0x8E,
                '‘' => 0x91,
                '’' => 0x92,
                '“' => 0x93,
                '”' => 0x94,
                '•' => 0x95,
                '–' => 0x96,
                '—' => 0x97,
                '˜' => 0x98,
                '™' => 0x99,
                'š' => 0x9A,
                '›' => 0x9B,
                'œ' => 0x9C,
                'ž' => 0x9E,
                'Ÿ' => 0x9F,
                '\t' => b' ',
                _ => b'?',
            },
        })
        .collect()
}

pub fn largura_texto(s: &str, fonte: Fonte, tamanho: f32) -> f32 {
    let m = fonte.metricas();
    win_ansi(s).iter().map(|&b| m.larguras[b as usize]).sum::<f32>() * tamanho / 1000.0
}

/// Quebra o texto em linhas que caibam na largura (palavras longas são partidas).
pub fn quebrar_linhas(s: &str, fonte: Fonte, tamanho: f32, largura: Option<f32>) -> Vec<String> {
    let mut linhas = Vec::new();
    for paragrafo in s.split('\n') {
        let Some(max) = largura else {
            linhas.push(paragrafo.to_string());
            continue;
        };
        let mut atual = String::new();
        for palavra in paragrafo.split(' ') {
            let tentativa = if atual.is_empty() { palavra.to_string() } else { format!("{atual} {palavra}") };
            if largura_texto(&tentativa, fonte, tamanho) <= max + 0.01 {
                atual = tentativa;
                continue;
            }
            if !atual.is_empty() {
                linhas.push(std::mem::take(&mut atual));
            }
            // Palavra maior que a linha: parte caractere a caractere.
            for c in palavra.chars() {
                let t = format!("{atual}{c}");
                if !atual.is_empty() && largura_texto(&t, fonte, tamanho) > max + 0.01 {
                    linhas.push(std::mem::take(&mut atual));
                    atual.push(c);
                } else {
                    atual = t;
                }
            }
        }
        linhas.push(atual);
    }
    linhas
}

fn cortar_com_reticencias(s: &str, fonte: Fonte, tamanho: f32, max: f32) -> String {
    if largura_texto(s, fonte, tamanho) <= max + 0.01 {
        return s.to_string();
    }
    let mut chars: Vec<char> = s.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let t: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if largura_texto(&t, fonte, tamanho) <= max + 0.01 {
            return t;
        }
    }
    String::new()
}

pub struct Documento {
    paginas: Vec<Content>,
    atual: usize,
    titulo: String,
}

fn y_pdf(y: f32) -> f32 {
    ALTURA_A4 - y
}

impl Documento {
    pub fn new(titulo: &str) -> Self {
        Documento { paginas: vec![Content::new()], atual: 0, titulo: titulo.to_string() }
    }

    pub fn nova_pagina(&mut self) {
        self.paginas.push(Content::new());
        self.atual = self.paginas.len() - 1;
    }

    pub fn total_paginas(&self) -> usize {
        self.paginas.len()
    }

    pub fn ir_para_pagina(&mut self, i: usize) {
        self.atual = i;
    }

    fn c(&mut self) -> &mut Content {
        &mut self.paginas[self.atual]
    }

    pub fn retangulo(&mut self, x: f32, y: f32, w: f32, h: f32, espessura: f32) {
        self.c().set_line_width(espessura).set_stroke_rgb(0.0, 0.0, 0.0).rect(x, y_pdf(y + h), w, h).stroke();
    }

    pub fn preencher(&mut self, x: f32, y: f32, w: f32, h: f32, cor: Cor) {
        self.c().set_fill_rgb(cor.0, cor.1, cor.2).rect(x, y_pdf(y + h), w, h).fill_nonzero();
    }

    pub fn linha(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, espessura: f32, cor: Cor, tracejado: Option<(f32, f32)>) {
        let c = self.c();
        c.save_state().set_line_width(espessura).set_stroke_rgb(cor.0, cor.1, cor.2);
        if let Some((traco, espaco)) = tracejado {
            c.set_dash_pattern([traco, espaco], 0.0);
        }
        c.move_to(x1, y_pdf(y1)).line_to(x2, y_pdf(y2)).stroke().restore_state();
    }

    /// Altura que o texto ocupará (linhas × altura da linha).
    pub fn altura_texto(&self, s: &str, o: &Txt) -> f32 {
        quebrar_linhas(s, o.fonte, o.tamanho, o.largura).len() as f32 * o.altura_linha()
    }

    /// Desenha o texto com o topo da primeira linha em `y`. Devolve o y após a última linha.
    pub fn texto(&mut self, s: &str, x: f32, y: f32, o: Txt) -> f32 {
        let lh = o.altura_linha();
        let mut linhas = if o.quebrar {
            quebrar_linhas(s, o.fonte, o.tamanho, o.largura)
        } else {
            let unica = s.replace('\n', " ");
            vec![match o.largura {
                Some(l) => cortar_com_reticencias(&unica, o.fonte, o.tamanho, l),
                None => unica,
            }]
        };
        if let Some(h) = o.altura {
            linhas.truncate(((h + 0.01) / lh).floor().max(0.0) as usize);
        }
        let ascendente = o.fonte.metricas().ascendente / 1000.0 * o.tamanho;
        let c = &mut self.paginas[self.atual];
        c.set_fill_rgb(o.cor.0, o.cor.1, o.cor.2);
        for (i, linha) in linhas.iter().enumerate() {
            if linha.is_empty() {
                continue;
            }
            let w = largura_texto(linha, o.fonte, o.tamanho);
            let dx = match (o.alinhar, o.largura) {
                (Alinhar::Centro, Some(l)) => (l - w) / 2.0,
                (Alinhar::Direita, Some(l)) => l - w,
                _ => 0.0,
            };
            let base = y + ascendente + i as f32 * lh;
            c.begin_text()
                .set_font(o.fonte.nome(), o.tamanho)
                .set_text_matrix([1.0, 0.0, 0.0, 1.0, x + dx, y_pdf(base)])
                .show(Str(&win_ansi(linha)))
                .end_text();
        }
        c.set_fill_rgb(0.0, 0.0, 0.0);
        y + linhas.len() as f32 * lh
    }

    /// Texto girado (graus, sentido anti-horário) em torno de (ox, oy), com transparência.
    pub fn texto_girado(&mut self, s: &str, x: f32, y: f32, o: Txt, graus: f32, origem: (f32, f32), opacidade: bool) {
        let (sen, cos) = graus.to_radians().sin_cos();
        let (ox, oy) = (origem.0, y_pdf(origem.1));
        {
            let c = self.c();
            c.save_state();
            if opacidade {
                c.set_parameters(Name(b"GS1"));
            }
            c.transform([cos, sen, -sen, cos, ox - ox * cos + oy * sen, oy - ox * sen - oy * cos]);
        }
        self.texto(s, x, y, o);
        self.c().restore_state();
    }

    pub fn finalizar(self) -> Vec<u8> {
        let mut pdf = Pdf::new();
        let catalogo = Ref::new(1);
        let arvore = Ref::new(2);
        let f1 = Ref::new(3);
        let f2 = Ref::new(4);
        let gs = Ref::new(5);
        let info = Ref::new(6);
        let mut prox = 7;
        let mut ids = Vec::new();
        for _ in &self.paginas {
            ids.push((Ref::new(prox), Ref::new(prox + 1)));
            prox += 2;
        }

        pdf.catalog(catalogo).pages(arvore);
        pdf.pages(arvore).kids(ids.iter().map(|(p, _)| *p)).count(ids.len() as i32);
        pdf.type1_font(f1).base_font(Name(b"Helvetica")).encoding_predefined(Name(b"WinAnsiEncoding"));
        pdf.type1_font(f2).base_font(Name(b"Helvetica-Bold")).encoding_predefined(Name(b"WinAnsiEncoding"));
        pdf.ext_graphics(gs).non_stroking_alpha(0.13).stroking_alpha(0.13);
        pdf.document_info(info).title(TextStr(&self.titulo)).producer(TextStr("EmissorNfe"));

        for ((pagina, conteudo), c) in ids.iter().zip(self.paginas) {
            let mut p = pdf.page(*pagina);
            p.media_box(Rect::new(0.0, 0.0, LARGURA_A4, ALTURA_A4)).parent(arvore).contents(*conteudo);
            let mut r = p.resources();
            r.fonts().pair(Name(b"F1"), f1).pair(Name(b"F2"), f2);
            r.ext_g_states().pair(Name(b"GS1"), gs);
            r.finish();
            p.finish();
            pdf.stream(*conteudo, &c.finish());
        }
        pdf.finish()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn codifica_win_ansi_e_mede() {
        assert_eq!(win_ansi("Ação…€"), vec![b'A', 0xE7, 0xE3, b'o', 0x85, 0x80]);
        assert_eq!(win_ansi("✓"), vec![b'?']);
        // "Hello" em Helvetica 10pt: H722 e556 l222 l222 o556 = 2278 → 22,78
        assert!((largura_texto("Hello", Fonte::Normal, 10.0) - 22.78).abs() < 0.01);
    }

    #[test]
    fn quebra_linhas() {
        let l = quebrar_linhas("aaa bbb ccc", Fonte::Normal, 10.0, Some(40.0));
        assert_eq!(l, vec!["aaa bbb", "ccc"]);
        assert_eq!(quebrar_linhas("a\nb", Fonte::Normal, 10.0, None), vec!["a", "b"]);
        let partida = quebrar_linhas("XXXXXXXXXXXXXXXX", Fonte::Normal, 10.0, Some(30.0));
        assert!(partida.len() > 1 && partida.iter().all(|p| largura_texto(p, Fonte::Normal, 10.0) <= 30.01));
        assert!(cortar_com_reticencias("texto muito comprido", Fonte::Normal, 10.0, 40.0).ends_with('…'));
    }
}
