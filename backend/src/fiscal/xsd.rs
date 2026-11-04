//! Validação de XML contra os schemas oficiais (XSD), em Rust puro.
//!
//! Implementa o subconjunto de XML Schema 1.0 usado pelos pacotes da NF-e:
//! sequence/choice/any com minOccurs/maxOccurs, element (name/ref/type ou tipo
//! anônimo), simpleType por restriction (pattern, enumeration, length,
//! minLength, maxLength, whiteSpace), complexType com atributos, simpleContent
//! por extension e unique simples (selector "./*", field "@atributo").
//! Qualquer construção fora desse subconjunto faz o carregamento do schema falhar,
//! em vez de ser ignorada em silêncio.

use super::xml::{NS_DSIG, NS_NFE, ler};
use anyhow::{Context, Result, bail};
use regex::Regex;
use roxmltree::{Document, Node};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex};

const XS: &str = "http://www.w3.org/2001/XMLSchema";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Pacotes de schema usados por cada documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pacote {
    Nfe,
    Inutilizacao,
    Cancelamento,
    Cce,
}

impl Pacote {
    fn arquivos(self) -> (&'static str, &'static str) {
        match self {
            Pacote::Nfe => ("PL_009g_V4_00_NT_2020_005_v120", "nfe_v4.00.xsd"),
            Pacote::Inutilizacao => ("PL_009g_V4_00_NT_2020_005_v120", "inutNFe_v4.00.xsd"),
            Pacote::Cancelamento => ("Evento_Canc_PL_v1.01", "envEventoCancNFe_v1.00.xsd"),
            Pacote::Cce => ("Evento_CCe_PL_v1.01", "envCCe_v1.00.xsd"),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ResultadoXsd {
    pub valido: bool,
    pub erros: Vec<String>,
}

/// Pasta `schemas/` do projeto (NFE_SCHEMAS ou procurada a partir da pasta atual).
pub fn pasta_schemas() -> PathBuf {
    if let Ok(p) = std::env::var("NFE_SCHEMAS") {
        return PathBuf::from(p);
    }
    crate::raiz_do_projeto().join("schemas")
}

static CACHE: LazyLock<Mutex<HashMap<Pacote, Arc<Schema>>>> = LazyLock::new(Default::default);

pub fn schema(pacote: Pacote) -> Result<Arc<Schema>> {
    let mut cache = CACHE.lock().unwrap();
    if let Some(s) = cache.get(&pacote) {
        return Ok(s.clone());
    }
    let (pasta, principal) = pacote.arquivos();
    let s = Arc::new(Schema::carregar(&pasta_schemas().join(pasta), principal)?);
    cache.insert(pacote, s.clone());
    Ok(s)
}

pub fn validar_xsd(xml: &str, pacote: Pacote) -> Result<ResultadoXsd> {
    Ok(schema(pacote)?.validar(xml))
}

// ---------------------------------------------------------------------------
// Modelo do schema
// ---------------------------------------------------------------------------

type QName = (String, String); // (namespace, nome local)
type TipoId = usize;
type ElemId = usize;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Primitivo {
    Texto,    // string: espaços preservados
    Token,    // token, anyURI, dateTime...: espaços colapsados
    Id,       // xs:ID: como token, e único no documento
    Base64,
    Qualquer, // anyType / anySimpleType
}

#[derive(Debug, Default)]
struct Facetas {
    padroes: Vec<(String, Regex)>,
    enumeracao: Vec<String>,
    tamanho: Option<usize>,
    min: Option<usize>,
    max: Option<usize>,
}

#[derive(Debug)]
struct TipoSimples {
    primitivo: Primitivo,
    /// Uma entrada por passo de derivação (da base para o tipo final).
    restricoes: Vec<Arc<Facetas>>,
}

#[derive(Debug)]
struct Atributo {
    nome: String,
    tipo: TipoId,
    obrigatorio: bool,
    fixo: Option<String>,
}

#[derive(Debug)]
enum Conteudo {
    Vazio,
    Simples(TipoId),
    Elementos(Particula),
    Qualquer,
}

#[derive(Debug)]
struct TipoComplexo {
    atributos: Vec<Atributo>,
    qualquer_atributo: bool,
    conteudo: Conteudo,
}

#[derive(Debug)]
enum Tipo {
    Pendente,
    Simples(TipoSimples),
    Complexo(TipoComplexo),
}

#[derive(Debug, Clone, Copy)]
struct Ocorrencias {
    min: usize,
    max: Option<usize>, // None = unbounded
}

#[derive(Debug)]
enum Particula {
    Elemento(ElemId, Ocorrencias),
    Sequencia(Vec<Particula>, Ocorrencias),
    Escolha(Vec<Particula>, Ocorrencias),
    Qualquer(Ocorrencias),
}

#[derive(Debug)]
struct DeclElemento {
    nome: QName,
    tipo: TipoId,
    unicos: Vec<String>, // atributos que devem ser únicos entre os filhos
}

pub struct Schema {
    tipos: Vec<Tipo>,
    elementos: Vec<DeclElemento>,
    globais: HashMap<QName, ElemId>,
}

// ---------------------------------------------------------------------------
// Carregamento
// ---------------------------------------------------------------------------

struct Montador<'d> {
    docs: Vec<&'d Document<'d>>,
    tipos_nomeados: HashMap<QName, Node<'d, 'd>>,
    elementos_globais: HashMap<QName, Node<'d, 'd>>,
    tipos_prontos: HashMap<QName, TipoId>,
    globais_prontos: HashMap<QName, ElemId>,
    tipos: Vec<Tipo>,
    elementos: Vec<DeclElemento>,
}

fn e_xs(n: &Node, nome: &str) -> bool {
    n.is_element() && n.tag_name().namespace() == Some(XS) && n.tag_name().name() == nome
}

fn filhos_xs<'a, 'd>(n: Node<'a, 'd>) -> impl Iterator<Item = Node<'a, 'd>> {
    n.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(XS) && c.tag_name().name() != "annotation")
}

fn namespace_alvo(n: Node) -> String {
    n.ancestors().find(|a| e_xs(a, "schema")).and_then(|s| s.attribute("targetNamespace")).unwrap_or("").to_string()
}

fn resolver_qname(n: Node, valor: &str) -> Result<QName> {
    let (prefixo, local) = match valor.split_once(':') {
        Some((p, l)) => (Some(p), l),
        None => (None, valor),
    };
    let ns = n.lookup_namespace_uri(prefixo).unwrap_or("");
    if prefixo.is_some() && ns.is_empty() {
        bail!("Prefixo não declarado em {valor}");
    }
    Ok((ns.to_string(), local.to_string()))
}

fn ocorrencias(n: Node) -> Result<Ocorrencias> {
    let min = n.attribute("minOccurs").map(str::parse).transpose()?.unwrap_or(1);
    let max = match n.attribute("maxOccurs") {
        Some("unbounded") => None,
        Some(v) => Some(v.parse()?),
        None => Some(1),
    };
    Ok(Ocorrencias { min, max })
}

impl Schema {
    pub fn carregar(pasta: &Path, principal: &str) -> Result<Schema> {
        // Lê o principal e tudo o que ele inclui/importa (mesma pasta).
        let mut textos: Vec<(String, String)> = Vec::new();
        let mut fila = vec![principal.to_string()];
        let mut vistos = HashSet::new();
        let re_local = Regex::new(r#"schemaLocation="([^"]+)""#).unwrap();
        while let Some(arq) = fila.pop() {
            if !vistos.insert(arq.clone()) {
                continue;
            }
            let caminho = pasta.join(&arq);
            let bruto = std::fs::read(&caminho).with_context(|| format!("Schema não encontrado: {}", caminho.display()))?;
            let texto = String::from_utf8_lossy(&bruto).trim_start_matches('\u{feff}').to_string();
            for local in re_local.captures_iter(&texto) {
                fila.push(local[1].to_string());
            }
            textos.push((arq, texto));
        }
        let docs: Vec<Document> = textos
            .iter()
            .map(|(arq, t)| ler(t).with_context(|| format!("Schema inválido: {arq}")))
            .collect::<Result<_>>()?;
        let mut m = Montador {
            docs: docs.iter().collect(),
            tipos_nomeados: HashMap::new(),
            elementos_globais: HashMap::new(),
            tipos_prontos: HashMap::new(),
            globais_prontos: HashMap::new(),
            tipos: Vec::new(),
            elementos: Vec::new(),
        };
        m.indexar()?;
        let nomes: Vec<QName> = m.elementos_globais.keys().cloned().collect();
        for q in &nomes {
            m.elemento_global(q)?;
        }
        Ok(Schema { tipos: m.tipos, elementos: m.elementos, globais: m.globais_prontos })
    }
}

impl<'d> Montador<'d> {
    fn indexar(&mut self) -> Result<()> {
        for doc in self.docs.clone() {
            let raiz = doc.root_element();
            if !e_xs(&raiz, "schema") {
                bail!("Arquivo não é um XSD");
            }
            if raiz.attribute("elementFormDefault") != Some("qualified") {
                bail!("Somente elementFormDefault=\"qualified\" é suportado");
            }
            let ns = raiz.attribute("targetNamespace").unwrap_or("").to_string();
            for n in filhos_xs(raiz) {
                let nome = n.attribute("name").map(|s| (ns.clone(), s.to_string()));
                match n.tag_name().name() {
                    "simpleType" | "complexType" => {
                        self.tipos_nomeados.insert(nome.context("tipo global sem nome")?, n);
                    }
                    "element" => {
                        self.elementos_globais.insert(nome.context("elemento global sem nome")?, n);
                    }
                    "include" | "import" => {}
                    outro => bail!("Construção XSD não suportada no topo: {outro}"),
                }
            }
        }
        Ok(())
    }

    fn novo_tipo(&mut self) -> TipoId {
        self.tipos.push(Tipo::Pendente);
        self.tipos.len() - 1
    }

    fn primitivo(&mut self, p: Primitivo) -> TipoId {
        let id = self.novo_tipo();
        self.tipos[id] = Tipo::Simples(TipoSimples { primitivo: p, restricoes: vec![] });
        id
    }

    fn tipo_por_nome(&mut self, q: &QName) -> Result<TipoId> {
        if let Some(&id) = self.tipos_prontos.get(q) {
            return Ok(id);
        }
        if q.0 == XS {
            let p = match q.1.as_str() {
                "string" => Primitivo::Texto,
                "ID" => Primitivo::Id,
                "token" | "anyURI" | "dateTime" | "date" | "NCName" => Primitivo::Token,
                "base64Binary" => Primitivo::Base64,
                "anyType" | "anySimpleType" => Primitivo::Qualquer,
                outro => bail!("Tipo nativo XSD não suportado: {outro}"),
            };
            let id = self.primitivo(p);
            self.tipos_prontos.insert(q.clone(), id);
            return Ok(id);
        }
        let no = *self.tipos_nomeados.get(q).with_context(|| format!("Tipo não encontrado: {}:{}", q.0, q.1))?;
        let id = self.novo_tipo();
        self.tipos_prontos.insert(q.clone(), id); // antes de montar: permite referência circular
        let tipo = self.montar_tipo(no)?;
        self.tipos[id] = tipo;
        Ok(id)
    }

    fn tipo_anonimo(&mut self, no: Node<'d, 'd>) -> Result<TipoId> {
        let id = self.novo_tipo();
        let tipo = self.montar_tipo(no)?;
        self.tipos[id] = tipo;
        Ok(id)
    }

    fn montar_tipo(&mut self, no: Node<'d, 'd>) -> Result<Tipo> {
        if e_xs(&no, "simpleType") {
            Ok(Tipo::Simples(self.montar_simples(no)?))
        } else {
            Ok(Tipo::Complexo(self.montar_complexo(no)?))
        }
    }

    /// Tipo simples já montado, copiado (as facetas são compartilhadas).
    fn simples_de(&self, id: TipoId) -> Result<TipoSimples> {
        match &self.tipos[id] {
            Tipo::Simples(s) => Ok(TipoSimples { primitivo: s.primitivo, restricoes: s.restricoes.clone() }),
            _ => bail!("Esperado tipo simples"),
        }
    }

    fn montar_simples(&mut self, no: Node<'d, 'd>) -> Result<TipoSimples> {
        let restr = filhos_xs(no).next().context("simpleType vazio")?;
        if !e_xs(&restr, "restriction") {
            bail!("simpleType só por restriction é suportado ({})", restr.tag_name().name());
        }
        let base_id = match restr.attribute("base") {
            Some(b) => self.tipo_por_nome(&resolver_qname(restr, b)?)?,
            None => {
                let inline = filhos_xs(restr).find(|c| e_xs(c, "simpleType")).context("restriction sem base")?;
                self.tipo_anonimo(inline)?
            }
        };
        let mut tipo = self.simples_de(base_id)?;
        tipo.restricoes.push(Arc::new(self.facetas(restr)?));
        Ok(tipo)
    }

    fn facetas(&self, restr: Node) -> Result<Facetas> {
        let mut f = Facetas::default();
        for c in filhos_xs(restr) {
            let valor = c.attribute("value").unwrap_or("");
            match c.tag_name().name() {
                "pattern" => {
                    let re = Regex::new(&format!("^(?:{valor})$")).with_context(|| format!("Padrão XSD não suportado: {valor}"))?;
                    f.padroes.push((valor.to_string(), re));
                }
                "enumeration" => f.enumeracao.push(valor.to_string()),
                "length" => f.tamanho = Some(valor.parse()?),
                "minLength" => f.min = Some(valor.parse()?),
                "maxLength" => f.max = Some(valor.parse()?),
                "whiteSpace" if valor == "preserve" => {}
                "simpleType" => {}
                outro => bail!("Faceta XSD não suportada: {outro} {valor}"),
            }
        }
        Ok(f)
    }

    fn montar_complexo(&mut self, no: Node<'d, 'd>) -> Result<TipoComplexo> {
        let mut tipo = TipoComplexo { atributos: vec![], qualquer_atributo: false, conteudo: Conteudo::Vazio };
        for c in filhos_xs(no) {
            match c.tag_name().name() {
                "sequence" | "choice" | "all" => tipo.conteudo = Conteudo::Elementos(self.particula(c)?),
                "attribute" => tipo.atributos.push(self.atributo(c)?),
                "anyAttribute" => tipo.qualquer_atributo = true,
                "simpleContent" => {
                    let ext = filhos_xs(c).next().context("simpleContent vazio")?;
                    if !e_xs(&ext, "extension") {
                        bail!("simpleContent só por extension é suportado");
                    }
                    let base = self.tipo_por_nome(&resolver_qname(ext, ext.attribute("base").context("extension sem base")?)?)?;
                    tipo.conteudo = Conteudo::Simples(base);
                    for a in filhos_xs(ext) {
                        match a.tag_name().name() {
                            "attribute" => tipo.atributos.push(self.atributo(a)?),
                            "anyAttribute" => tipo.qualquer_atributo = true,
                            outro => bail!("Construção não suportada em extension: {outro}"),
                        }
                    }
                }
                outro => bail!("Construção não suportada em complexType: {outro}"),
            }
        }
        if no.attribute("mixed") == Some("true") {
            bail!("complexType mixed não é suportado");
        }
        Ok(tipo)
    }

    fn atributo(&mut self, no: Node<'d, 'd>) -> Result<Atributo> {
        let nome = no.attribute("name").context("atributo sem nome (ref não suportado)")?.to_string();
        let tipo = match no.attribute("type") {
            Some(t) => self.tipo_por_nome(&resolver_qname(no, t)?)?,
            None => match filhos_xs(no).find(|c| e_xs(c, "simpleType")) {
                Some(s) => self.tipo_anonimo(s)?,
                None => self.primitivo(Primitivo::Qualquer),
            },
        };
        Ok(Atributo {
            nome,
            tipo,
            obrigatorio: no.attribute("use") == Some("required"),
            fixo: no.attribute("fixed").map(String::from),
        })
    }

    fn particula(&mut self, no: Node<'d, 'd>) -> Result<Particula> {
        let oc = ocorrencias(no)?;
        match no.tag_name().name() {
            "element" => Ok(Particula::Elemento(self.elemento_local(no)?, oc)),
            "any" => Ok(Particula::Qualquer(oc)),
            "sequence" | "choice" => {
                let itens = filhos_xs(no).map(|c| self.particula(c)).collect::<Result<Vec<_>>>()?;
                Ok(if no.tag_name().name() == "sequence" { Particula::Sequencia(itens, oc) } else { Particula::Escolha(itens, oc) })
            }
            outro => bail!("Partícula XSD não suportada: {outro}"),
        }
    }

    fn unicos(no: Node) -> Result<Vec<String>> {
        let mut campos = vec![];
        for u in filhos_xs(no).filter(|c| e_xs(c, "unique")) {
            let sel = filhos_xs(u).find(|c| e_xs(c, "selector")).and_then(|c| c.attribute("xpath"));
            let campo = filhos_xs(u).find(|c| e_xs(c, "field")).and_then(|c| c.attribute("xpath"));
            match (sel, campo) {
                (Some("./*"), Some(f)) if f.starts_with('@') => campos.push(f[1..].to_string()),
                _ => bail!("unique não suportado: {sel:?} {campo:?}"),
            }
        }
        Ok(campos)
    }

    fn declarar_elemento(&mut self, no: Node<'d, 'd>, nome: QName) -> Result<ElemId> {
        let tipo = match no.attribute("type") {
            Some(t) => self.tipo_por_nome(&resolver_qname(no, t)?)?,
            None => match filhos_xs(no).find(|c| e_xs(c, "simpleType") || e_xs(c, "complexType")) {
                Some(t) => self.tipo_anonimo(t)?,
                None => {
                    let id = self.novo_tipo();
                    self.tipos[id] = Tipo::Complexo(TipoComplexo {
                        atributos: vec![],
                        qualquer_atributo: true,
                        conteudo: Conteudo::Qualquer,
                    });
                    id
                }
            },
        };
        let unicos = Self::unicos(no)?;
        self.elementos.push(DeclElemento { nome, tipo, unicos });
        Ok(self.elementos.len() - 1)
    }

    fn elemento_global(&mut self, q: &QName) -> Result<ElemId> {
        if let Some(&id) = self.globais_prontos.get(q) {
            return Ok(id);
        }
        let no = *self.elementos_globais.get(q).with_context(|| format!("Elemento global não encontrado: {}", q.1))?;
        let id = self.declarar_elemento(no, q.clone())?;
        self.globais_prontos.insert(q.clone(), id);
        Ok(id)
    }

    fn elemento_local(&mut self, no: Node<'d, 'd>) -> Result<ElemId> {
        if let Some(r) = no.attribute("ref") {
            return self.elemento_global(&resolver_qname(no, r)?);
        }
        let nome = no.attribute("name").context("element sem name/ref")?;
        self.declarar_elemento(no, (namespace_alvo(no), nome.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Validação
// ---------------------------------------------------------------------------

fn nome_legivel(ns: &str, local: &str) -> String {
    match ns {
        NS_NFE | "" => local.to_string(),
        NS_DSIG => format!("ds:{local}"),
        outro => format!("{{{outro}}}{local}"),
    }
}

fn nome_no(n: Node) -> String {
    nome_legivel(n.tag_name().namespace().unwrap_or(""), n.tag_name().name())
}

/// Caminho percorrido no casamento do conteúdo: qual declaração casou cada filho.
struct Passo {
    decl: Option<ElemId>, // None = xs:any (não valida)
    anterior: Option<Rc<Passo>>,
}

type Estado = (usize, Option<Rc<Passo>>);

/// Erros encontrados e IDs já vistos no documento.
#[derive(Default)]
struct Saida {
    erros: Vec<String>,
    ids: HashSet<String>,
}

impl std::ops::Deref for Saida {
    type Target = Vec<String>;
    fn deref(&self) -> &Vec<String> {
        &self.erros
    }
}

impl std::ops::DerefMut for Saida {
    fn deref_mut(&mut self) -> &mut Vec<String> {
        &mut self.erros
    }
}

#[derive(Default)]
struct Diagnostico {
    alcance: usize,
    esperados: HashMap<usize, BTreeSet<String>>,
}

impl Schema {
    pub fn validar(&self, xml: &str) -> ResultadoXsd {
        let mut erros = Saida::default();
        match ler(xml) {
            Err(e) => erros.push(format!("XML mal formado: {e}")),
            Ok(doc) => {
                let raiz = doc.root_element();
                let q = (raiz.tag_name().namespace().unwrap_or("").to_string(), raiz.tag_name().name().to_string());
                match self.globais.get(&q) {
                    Some(&id) => self.validar_elemento(raiz, id, &mut erros),
                    None => erros.push(format!("Elemento '{}': não há declaração para o elemento raiz.", nome_no(raiz))),
                }
            }
        }
        ResultadoXsd { valido: erros.is_empty(), erros: erros.erros }
    }

    fn validar_elemento(&self, no: Node, id: ElemId, erros: &mut Saida) {
        let decl = &self.elementos[id];
        let nome = nome_no(no);
        match &self.tipos[decl.tipo] {
            Tipo::Pendente => erros.push(format!("Elemento '{nome}': tipo não resolvido no schema.")),
            Tipo::Simples(t) => {
                if no.children().any(|c| c.is_element()) {
                    erros.push(format!("Elemento '{nome}': não pode conter elementos filhos."));
                    return;
                }
                if let Some(a) = no.attributes().find(|a| a.namespace() != Some(XSI)) {
                    erros.push(format!("Elemento '{nome}', atributo '{}': atributo não permitido.", a.name()));
                }
                self.validar_valor(t, &texto_direto(no), &format!("Elemento '{nome}'"), erros);
            }
            Tipo::Complexo(t) => {
                self.validar_atributos(no, t, &nome, erros);
                match &t.conteudo {
                    Conteudo::Qualquer => {}
                    Conteudo::Vazio => {
                        if no.children().any(|c| c.is_element()) || !texto_direto(no).trim().is_empty() {
                            erros.push(format!("Elemento '{nome}': deve ser vazio."));
                        }
                    }
                    Conteudo::Simples(tid) => {
                        if no.children().any(|c| c.is_element()) {
                            erros.push(format!("Elemento '{nome}': não pode conter elementos filhos."));
                        } else if let Tipo::Simples(ts) = &self.tipos[*tid] {
                            self.validar_valor(ts, &texto_direto(no), &format!("Elemento '{nome}'"), erros);
                        }
                    }
                    Conteudo::Elementos(p) => {
                        if !texto_direto(no).trim().is_empty() {
                            erros.push(format!("Elemento '{nome}': texto não permitido (só elementos filhos)."));
                        }
                        self.validar_conteudo(no, p, &nome, erros);
                    }
                }
            }
        }
        for campo in &decl.unicos {
            let mut vistos = HashSet::new();
            for f in no.children().filter(|c| c.is_element()) {
                if let Some(v) = f.attribute(campo.as_str()) {
                    if !vistos.insert(v) {
                        erros.push(format!("Elemento '{}': valor duplicado '{v}' no atributo '{campo}'.", nome_no(f)));
                    }
                }
            }
        }
    }

    fn validar_atributos(&self, no: Node, t: &TipoComplexo, nome: &str, erros: &mut Saida) {
        for a in no.attributes() {
            if a.namespace() == Some(XSI) {
                continue;
            }
            let decl = t.atributos.iter().find(|d| a.namespace().is_none() && d.nome == a.name());
            match decl {
                None if t.qualquer_atributo => {}
                None => erros.push(format!("Elemento '{nome}', atributo '{}': atributo não permitido.", a.name())),
                Some(d) => {
                    let onde = format!("Elemento '{nome}', atributo '{}'", a.name());
                    if let Some(f) = &d.fixo {
                        if a.value() != f {
                            erros.push(format!("{onde}: o valor deve ser '{f}'."));
                        }
                    }
                    if let Tipo::Simples(ts) = &self.tipos[d.tipo] {
                        self.validar_valor(ts, a.value(), &onde, erros);
                        if ts.primitivo == Primitivo::Id && !erros.ids.insert(a.value().trim().to_string()) {
                            erros.push(format!("{onde}: o ID '{}' se repete no documento.", a.value()));
                        }
                    }
                }
            }
        }
        for d in t.atributos.iter().filter(|d| d.obrigatorio) {
            if no.attribute(d.nome.as_str()).is_none() {
                erros.push(format!("Elemento '{nome}': falta o atributo obrigatório '{}'.", d.nome));
            }
        }
    }

    fn validar_valor(&self, t: &TipoSimples, bruto: &str, onde: &str, erros: &mut Vec<String>) {
        let colapsado;
        let valor = if t.primitivo == Primitivo::Texto || t.primitivo == Primitivo::Qualquer {
            bruto
        } else {
            colapsado = bruto.split_whitespace().collect::<Vec<_>>().join(" ");
            &colapsado
        };
        if t.primitivo == Primitivo::Base64 {
            let limpo: String = valor.chars().filter(|c| !c.is_whitespace()).collect();
            let ok = limpo.len().is_multiple_of(4) && limpo.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=');
            if !ok {
                erros.push(format!("{onde}: '{}' não é base64 válido.", resumir(valor)));
                return;
            }
        }
        let tamanho = valor.chars().count();
        for f in &t.restricoes {
            if let Some(n) = f.tamanho {
                if tamanho != n {
                    erros.push(format!("{onde}: o valor '{}' tem {tamanho} caracteres; o exigido é {n}.", resumir(valor)));
                    return;
                }
            }
            if let Some(n) = f.min {
                if tamanho < n {
                    erros.push(format!("{onde}: o valor '{}' tem {tamanho} caracteres; o mínimo é {n}.", resumir(valor)));
                    return;
                }
            }
            if let Some(n) = f.max {
                if tamanho > n {
                    erros.push(format!("{onde}: o valor '{}' tem {tamanho} caracteres; o máximo é {n}.", resumir(valor)));
                    return;
                }
            }
            if !f.enumeracao.is_empty() && !f.enumeracao.iter().any(|e| e == valor) {
                let lista = if f.enumeracao.len() <= 12 { f.enumeracao.join(", ") } else { format!("{} valores", f.enumeracao.len()) };
                erros.push(format!("{onde}: o valor '{}' não está entre os permitidos ({lista}).", resumir(valor)));
                return;
            }
            if !f.padroes.is_empty() && !f.padroes.iter().any(|(_, re)| re.is_match(valor)) {
                erros.push(format!("{onde}: o valor '{}' não atende ao padrão '{}'.", resumir(valor), resumir(&f.padroes[0].0)));
                return;
            }
        }
    }

    fn validar_conteudo(&self, no: Node, p: &Particula, nome: &str, erros: &mut Saida) {
        let filhos: Vec<Node> = no.children().filter(|c| c.is_element()).collect();
        let mut diag = Diagnostico::default();
        let finais = self.casar(p, vec![(0, None)], &filhos, &mut diag);
        let Some((_, caminho)) = finais.into_iter().find(|(pos, _)| *pos == filhos.len()) else {
            let esperados = diag.esperados.get(&diag.alcance).map(|s| s.iter().cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default();
            if diag.alcance < filhos.len() {
                let f = filhos[diag.alcance];
                if esperados.is_empty() {
                    erros.push(format!("Elemento '{}': não esperado aqui (dentro de '{nome}').", nome_no(f)));
                } else {
                    erros.push(format!("Elemento '{}': não esperado aqui. Esperado: ( {esperados} ).", nome_no(f)));
                }
            } else {
                erros.push(format!("Elemento '{nome}': conteúdo incompleto. Esperado: ( {esperados} )."));
            }
            return;
        };
        // Reconstrói a ordem dos filhos a partir do caminho (do fim para o início).
        let mut decls = Vec::with_capacity(filhos.len());
        let mut atual = caminho;
        while let Some(p) = atual {
            decls.push(p.decl);
            atual = p.anterior.clone();
        }
        decls.reverse();
        for (f, d) in filhos.iter().zip(decls) {
            if let Some(id) = d {
                self.validar_elemento(*f, id, erros);
            }
        }
    }

    /// Avança os estados (posição + caminho) consumindo uma ocorrência da partícula.
    fn casar_uma(&self, p: &Particula, entrada: Vec<Estado>, filhos: &[Node], diag: &mut Diagnostico) -> Vec<Estado> {
        match p {
            Particula::Elemento(id, _) => {
                let (ns, local) = &self.elementos[*id].nome;
                let mut saida = vec![];
                for (pos, caminho) in entrada {
                    let casa = filhos.get(pos).is_some_and(|f| {
                        f.tag_name().name() == local && f.tag_name().namespace().unwrap_or("") == ns
                    });
                    if casa {
                        saida.push((pos + 1, Some(Rc::new(Passo { decl: Some(*id), anterior: caminho }))));
                    } else {
                        diag.esperados.entry(pos).or_default().insert(nome_legivel(ns, local));
                    }
                }
                saida
            }
            Particula::Qualquer(_) => entrada
                .into_iter()
                .filter(|(pos, _)| *pos < filhos.len())
                .map(|(pos, caminho)| (pos + 1, Some(Rc::new(Passo { decl: None, anterior: caminho }))))
                .collect(),
            Particula::Sequencia(itens, _) => {
                let mut estados = entrada;
                for item in itens {
                    estados = self.casar(item, estados, filhos, diag);
                    if estados.is_empty() {
                        break;
                    }
                }
                estados
            }
            Particula::Escolha(itens, _) => {
                let mut saida = vec![];
                for item in itens {
                    saida.extend(self.casar(item, entrada.clone(), filhos, diag));
                }
                deduplicar(saida)
            }
        }
    }

    /// Casa a partícula respeitando minOccurs/maxOccurs.
    fn casar(&self, p: &Particula, entrada: Vec<Estado>, filhos: &[Node], diag: &mut Diagnostico) -> Vec<Estado> {
        let oc = match p {
            Particula::Elemento(_, o) | Particula::Sequencia(_, o) | Particula::Escolha(_, o) | Particula::Qualquer(o) => *o,
        };
        // resultado = união dos estados após k repetições, para min ≤ k ≤ max.
        let mut resultado: Vec<Estado> = vec![];
        let mut posicoes: HashSet<usize> = HashSet::new();
        let mut atual = entrada;
        let mut k = 0;
        loop {
            if k >= oc.min {
                // A partir do mínimo, só posições novas precisam ser exploradas
                // (evita laço infinito com partículas que casam vazio).
                atual.retain(|(p, _)| !posicoes.contains(p));
                posicoes.extend(atual.iter().map(|(p, _)| *p));
                resultado.extend(atual.iter().cloned());
            }
            if atual.is_empty() || oc.max.is_some_and(|m| k >= m) || k > filhos.len() + oc.min {
                break;
            }
            atual = deduplicar(self.casar_uma(p, atual, filhos, diag));
            for (pos, _) in &atual {
                diag.alcance = diag.alcance.max(*pos);
            }
            k += 1;
        }
        resultado
    }
}

fn deduplicar(estados: Vec<Estado>) -> Vec<Estado> {
    let mut vistos = HashSet::new();
    estados.into_iter().filter(|(p, _)| vistos.insert(*p)).collect()
}

fn texto_direto(no: Node) -> String {
    no.children().filter(|c| c.is_text()).filter_map(|c| c.text()).collect()
}

fn resumir(s: &str) -> String {
    if s.chars().count() > 80 { format!("{}…", s.chars().take(80).collect::<String>()) } else { s.to_string() }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todos_os_pacotes_carregam() {
        for p in [Pacote::Nfe, Pacote::Inutilizacao, Pacote::Cancelamento, Pacote::Cce] {
            let s = schema(p).unwrap_or_else(|e| panic!("{p:?}: {e:#}"));
            assert!(!s.globais.is_empty());
        }
    }

    #[test]
    fn consulta_de_status_valida_e_invalida() {
        let pasta = pasta_schemas().join("PL_009g_V4_00_NT_2020_005_v120");
        let s = Schema::carregar(&pasta, "consStatServ_v4.00.xsd").unwrap();
        let ok = r#"<consStatServ versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><cUF>52</cUF><xServ>STATUS</xServ></consStatServ>"#;
        assert!(s.validar(ok).valido, "{:?}", s.validar(ok).erros);

        let r = s.validar(&ok.replace("<tpAmb>2</tpAmb>", "<tpAmb>3</tpAmb>"));
        assert_eq!(r.erros.len(), 1, "{:?}", r.erros);
        assert!(r.erros[0].contains("tpAmb"));

        let r = s.validar(&ok.replace("<cUF>52</cUF>", ""));
        assert!(r.erros[0].contains("Elemento 'xServ': não esperado aqui. Esperado: ( cUF )"), "{:?}", r.erros);

        let r = s.validar(&ok.replace(" versao=\"4.00\"", ""));
        assert!(r.erros[0].contains("falta o atributo obrigatório 'versao'"), "{:?}", r.erros);

        let r = s.validar(&ok.replace("<xServ>STATUS</xServ>", ""));
        assert!(r.erros[0].contains("conteúdo incompleto. Esperado: ( xServ )"), "{:?}", r.erros);
    }
}
