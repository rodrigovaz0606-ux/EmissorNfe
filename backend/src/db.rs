//! Persistência em SQLite: usuários, sessões, produtores (emitentes com certificado),
//! notas, eventos (cancelamento/CC-e), inutilizações e produtos.
//!
//! O esquema-base é o do backend anterior; as mudanças seguintes são migrações numeradas
//! (`PRAGMA user_version`), então bancos existentes continuam funcionando.
//! Notas, inutilizações e produtos pertencem a um produtor (`produtor_id`).

use anyhow::Result;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Params, Row, named_params, params};
use serde_json::{Map, Value};
use std::path::Path;

const ESQUEMA: &str = "
CREATE TABLE IF NOT EXISTS notas (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  chave         TEXT NOT NULL UNIQUE,
  serie         INTEGER NOT NULL,
  nNF           INTEGER NOT NULL,
  status        TEXT NOT NULL,          -- autorizada | rejeitada | cancelada | denegada | pendente
  cStat         TEXT,
  xMotivo       TEXT,
  nProt         TEXT,
  dhRecbto      TEXT,
  vNF           TEXT NOT NULL,
  emit_doc      TEXT,
  dest_doc      TEXT,
  dest_nome     TEXT,
  dados_json    TEXT NOT NULL,          -- dados do formulário (para reabrir/duplicar)
  xml_nfe       TEXT NOT NULL,          -- NF-e assinada
  xml_proc      TEXT,                   -- nfeProc (só autorizada)
  xml_resposta  TEXT,
  criada_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX IF NOT EXISTS ix_notas_numero ON notas (serie, nNF);

CREATE TABLE IF NOT EXISTS eventos (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  nota_id       INTEGER NOT NULL REFERENCES notas(id),
  tipo          TEXT NOT NULL,          -- cancelamento | cce
  nSeqEvento    INTEGER NOT NULL,
  texto         TEXT NOT NULL,          -- justificativa ou correção
  cStat         TEXT,
  xMotivo       TEXT,
  nProt         TEXT,
  dhRegEvento   TEXT,
  registrado    INTEGER NOT NULL DEFAULT 0,
  xml_proc      TEXT,
  xml_resposta  TEXT,
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS inutilizacoes (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  serie         INTEGER NOT NULL,
  nNFIni        INTEGER NOT NULL,
  nNFFin        INTEGER NOT NULL,
  xJust         TEXT NOT NULL,
  cStat         TEXT,
  xMotivo       TEXT,
  nProt         TEXT,
  homologada    INTEGER NOT NULL DEFAULT 0,
  xml_proc      TEXT,
  xml_resposta  TEXT,
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS produtos (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  cProd         TEXT NOT NULL UNIQUE COLLATE NOCASE,
  xProd         TEXT NOT NULL,
  dados_json    TEXT NOT NULL,          -- mesmo formato de um item do formulário
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  alterado_em   TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
";

/// Migração 1: login, produtores e separação dos dados por produtor.
const MIGRACAO_1: &str = "
CREATE TABLE usuarios (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  login         TEXT NOT NULL UNIQUE COLLATE NOCASE,
  nome          TEXT NOT NULL,
  senha_hash    TEXT NOT NULL,          -- Argon2id (formato PHC)
  admin         INTEGER NOT NULL DEFAULT 0,
  ativo         INTEGER NOT NULL DEFAULT 1,
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  ultimo_acesso TEXT
);

CREATE TABLE produtores (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  documento     TEXT NOT NULL UNIQUE,   -- CPF ou CNPJ do certificado
  titular       TEXT NOT NULL,
  tipo          TEXT NOT NULL,          -- e-CPF | e-CNPJ
  emissor_cert  TEXT,
  valido_ate    TEXT,
  dono_id       INTEGER REFERENCES usuarios(id),
  pfx_cifrado   BLOB,                   -- AES-256-GCM com a chave-mestra (NULL = removido)
  senha_cifrada BLOB,
  emitente_json TEXT,                   -- nome, IE, regime e endereço padrão do emitente
  ativo         INTEGER NOT NULL DEFAULT 1,
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  alterado_em   TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE sessoes (
  token_hash    TEXT PRIMARY KEY,       -- SHA-256 do token do cookie (o token em si não é guardado)
  usuario_id    INTEGER NOT NULL REFERENCES usuarios(id) ON DELETE CASCADE,
  produtor_id   INTEGER REFERENCES produtores(id),
  criada_em     INTEGER NOT NULL,
  expira_em     INTEGER NOT NULL
);

ALTER TABLE notas ADD COLUMN produtor_id INTEGER REFERENCES produtores(id);
ALTER TABLE inutilizacoes ADD COLUMN produtor_id INTEGER REFERENCES produtores(id);
CREATE INDEX ix_notas_produtor ON notas (produtor_id, serie, nNF);

CREATE TABLE produtos_novo (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  produtor_id   INTEGER REFERENCES produtores(id),
  cProd         TEXT NOT NULL COLLATE NOCASE,
  xProd         TEXT NOT NULL,
  dados_json    TEXT NOT NULL,
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  alterado_em   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  UNIQUE (produtor_id, cProd)
);
INSERT INTO produtos_novo (id, cProd, xProd, dados_json, criado_em, alterado_em)
  SELECT id, cProd, xProd, dados_json, criado_em, alterado_em FROM produtos;
DROP TABLE produtos;
ALTER TABLE produtos_novo RENAME TO produtos;
";

/// Migração 2: cadastro de clientes (destinatários) por produtor.
const MIGRACAO_2: &str = "
CREATE TABLE clientes (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  produtor_id   INTEGER NOT NULL REFERENCES produtores(id),
  documento     TEXT NOT NULL,          -- CPF ou CNPJ, só os dígitos
  xNome         TEXT NOT NULL,
  dados_json    TEXT NOT NULL,          -- mesmo formato do destinatário do formulário
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  alterado_em   TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
  UNIQUE (produtor_id, documento)
);
";

/// Migração 3: movimentos de estoque. Excluir um produto apaga o histórico dele.
const MIGRACAO_3: &str = "
CREATE TABLE estoque_movimentos (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  produtor_id   INTEGER NOT NULL REFERENCES produtores(id),
  produto_id    INTEGER NOT NULL REFERENCES produtos(id) ON DELETE CASCADE,
  tipo          TEXT NOT NULL,          -- entrada | saida | ajuste | nota | estorno
  quantidade    INTEGER NOT NULL,       -- com sinal, em décimos de milésimo (ver ESCALA_ESTOQUE)
  nota_id       INTEGER REFERENCES notas(id),
  usuario_id    INTEGER REFERENCES usuarios(id),
  observacao    TEXT,
  criado_em     TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX ix_estoque_produto ON estoque_movimentos (produto_id, id);
CREATE INDEX ix_estoque_nota ON estoque_movimentos (nota_id);
";

/// Quantidades de estoque são guardadas como inteiros: qCom tem até 4 casas decimais.
pub const ESCALA_ESTOQUE: f64 = 10_000.0;

/// Números "consumidos": notas que a SEFAZ registrou (autorizadas, canceladas, denegadas)
/// e faixas inutilizadas. Número de nota rejeitada pode ser reutilizado.
const STATUS_QUE_CONSOMEM: &str = "('autorizada', 'cancelada', 'denegada')";

#[derive(Debug, Clone)]
pub struct Nota {
    pub id: i64,
    pub produtor_id: Option<i64>,
    pub chave: String,
    pub serie: i64,
    pub n_nf: i64,
    pub status: String,
    pub n_prot: Option<String>,
    pub emit_doc: Option<String>,
    pub dados_json: String,
    pub xml_nfe: String,
    pub xml_proc: Option<String>,
    pub xml_resposta: Option<String>,
}

#[derive(Debug, Default)]
pub struct NovaNota {
    pub produtor_id: i64,
    pub chave: String,
    pub serie: i64,
    pub n_nf: i64,
    pub status: String,
    pub v_nf: String,
    pub emit_doc: Option<String>,
    pub dest_doc: Option<String>,
    pub dest_nome: Option<String>,
    pub dados_json: String,
    pub xml_nfe: String,
}

#[derive(Debug, Default)]
pub struct Resultado {
    pub status: String,
    pub c_stat: Option<String>,
    pub x_motivo: Option<String>,
    pub n_prot: Option<String>,
    pub dh_recbto: Option<String>,
    pub xml_proc: Option<String>,
    pub xml_resposta: Option<String>,
}

#[derive(Debug, Default)]
pub struct NovoEvento {
    pub nota_id: i64,
    pub tipo: String,
    pub n_seq_evento: i64,
    pub texto: String,
    pub c_stat: Option<String>,
    pub x_motivo: Option<String>,
    pub n_prot: Option<String>,
    pub dh_reg_evento: Option<String>,
    pub registrado: bool,
    pub xml_proc: Option<String>,
    pub xml_resposta: Option<String>,
}

#[derive(Debug, Default)]
pub struct NovaInutilizacao {
    pub produtor_id: i64,
    pub serie: i64,
    pub n_nf_ini: i64,
    pub n_nf_fin: i64,
    pub x_just: String,
    pub c_stat: Option<String>,
    pub x_motivo: Option<String>,
    pub n_prot: Option<String>,
    pub homologada: bool,
    pub xml_proc: Option<String>,
    pub xml_resposta: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Usuario {
    pub id: i64,
    pub login: String,
    pub nome: String,
    pub senha_hash: String,
    pub admin: bool,
    pub ativo: bool,
}

#[derive(Debug, Clone)]
pub struct Sessao {
    pub usuario_id: i64,
    pub produtor_id: Option<i64>,
    pub expira_em: i64,
}

#[derive(Debug, Clone)]
pub struct Produtor {
    pub id: i64,
    pub documento: String,
    pub titular: String,
    pub tipo: String,
    pub valido_ate: Option<String>,
    pub dono_id: Option<i64>,
    pub pfx_cifrado: Option<Vec<u8>>,
    pub senha_cifrada: Option<Vec<u8>>,
    pub emitente_json: Option<String>,
    pub ativo: bool,
}

/// Dados do certificado gravados junto com o produtor.
pub struct CertificadoProdutor<'a> {
    pub documento: &'a str,
    pub titular: &'a str,
    pub tipo: &'a str,
    pub emissor: &'a str,
    pub valido_ate: &'a str,
    pub pfx_cifrado: &'a [u8],
    pub senha_cifrada: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoMovimento {
    Entrada,
    Saida,
    Ajuste,
    /// Baixa ou entrada automática de uma nota autorizada.
    Nota,
    /// Devolução ao estoque quando a nota é cancelada.
    Estorno,
}

impl TipoMovimento {
    fn nome(self) -> &'static str {
        match self {
            TipoMovimento::Entrada => "entrada",
            TipoMovimento::Saida => "saida",
            TipoMovimento::Ajuste => "ajuste",
            TipoMovimento::Nota => "nota",
            TipoMovimento::Estorno => "estorno",
        }
    }

    fn de_nome(nome: &str) -> Option<Self> {
        [Self::Entrada, Self::Saida, Self::Ajuste, Self::Nota, Self::Estorno].into_iter().find(|t| t.nome() == nome)
    }
}

#[derive(Debug, Clone)]
pub struct NovoMovimento {
    pub produtor_id: i64,
    pub produto_id: i64,
    pub tipo: TipoMovimento,
    /// Com sinal, em décimos de milésimo.
    pub quantidade: i64,
    pub nota_id: Option<i64>,
    pub usuario_id: Option<i64>,
    pub observacao: Option<String>,
}

/// Movimento já gravado de uma nota.
#[derive(Debug, Clone)]
pub struct MovimentoDaNota {
    pub produto_id: i64,
    pub tipo: TipoMovimento,
    pub quantidade: i64,
}

/// Produto do ponto de vista do estoque.
#[derive(Debug, Clone)]
pub struct ProdutoEstoque {
    pub id: i64,
    pub c_prod: String,
    pub x_prod: String,
    pub u_com: String,
    pub controla: bool,
    pub minimo: f64,
    /// Em décimos de milésimo.
    pub saldo: i64,
}

fn produto_estoque_de(r: &Row) -> rusqlite::Result<ProdutoEstoque> {
    let dados: String = r.get(3)?;
    let d: Value = serde_json::from_str(&dados).unwrap_or(Value::Null);
    Ok(ProdutoEstoque {
        id: r.get(0)?,
        c_prod: r.get(1)?,
        x_prod: r.get(2)?,
        u_com: d["uCom"].as_str().unwrap_or("").to_string(),
        controla: d["controlaEstoque"].as_bool().unwrap_or(false),
        minimo: d["estoqueMinimo"].as_str().and_then(crate::produtos::ler_numero).unwrap_or(0.0),
        saldo: r.get(4)?,
    })
}

const SQL_PRODUTO_ESTOQUE: &str = "SELECT p.id, p.cProd, p.xProd, p.dados_json,
       COALESCE((SELECT SUM(m.quantidade) FROM estoque_movimentos m WHERE m.produto_id = p.id), 0)
FROM produtos p";

/// Filtros da lista de notas (tudo opcional).
#[derive(Debug, Default, Clone)]
pub struct FiltroNotas {
    /// Datas no formato AAAA-MM-DD (inclusive).
    pub de: Option<String>,
    pub ate: Option<String>,
    pub status: Option<String>,
    /// Número da nota, nome ou CPF/CNPJ do destinatário, ou chave.
    pub busca: Option<String>,
}

/// Nota que entra na exportação para o contador.
#[derive(Debug, Clone)]
pub struct NotaExportada {
    pub chave: String,
    pub serie: i64,
    pub n_nf: i64,
    pub status: String,
    pub v_nf: String,
    pub dest_doc: Option<String>,
    pub dest_nome: Option<String>,
    pub n_prot: Option<String>,
    pub criada_em: String,
    pub xml_nfe: String,
    pub xml_proc: Option<String>,
}

/// Evento registrado ou inutilização homologada, para a exportação.
#[derive(Debug, Clone)]
pub struct DocumentoExportado {
    pub nome: String,
    pub xml: String,
}

/// Contagem e valor (em centavos) das notas de um período.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ResumoPeriodo {
    pub autorizadas: i64,
    pub canceladas: i64,
    pub rejeitadas: i64,
    pub pendentes: i64,
    pub denegadas: i64,
    pub valor_autorizado: i64,
}

pub struct Repositorio {
    db: Connection,
}

/// Escapa % e _ para usar o texto digitado em LIKE ... ESCAPE '\\'.
fn padrao_like(texto: &str) -> String {
    let mut p = String::from("%");
    for c in texto.chars() {
        if matches!(c, '%' | '_' | '\\') {
            p.push('\\');
        }
        p.push(c);
    }
    p.push('%');
    p
}

/// Valor em centavos, sem somar frações de real em ponto flutuante.
const CENTAVOS_SQL: &str = "CAST(ROUND(CAST(vNF AS REAL) * 100) AS INTEGER)";

const CAMPOS_USUARIO: &str = "id, login, nome, senha_hash, admin, ativo";
fn usuario_de(r: &Row) -> rusqlite::Result<Usuario> {
    Ok(Usuario {
        id: r.get(0)?,
        login: r.get(1)?,
        nome: r.get(2)?,
        senha_hash: r.get(3)?,
        admin: r.get::<_, i64>(4)? == 1,
        ativo: r.get::<_, i64>(5)? == 1,
    })
}

const CAMPOS_PRODUTOR: &str = "id, documento, titular, tipo, valido_ate, dono_id, pfx_cifrado, senha_cifrada, emitente_json, ativo";
fn produtor_de(r: &Row) -> rusqlite::Result<Produtor> {
    Ok(Produtor {
        id: r.get(0)?,
        documento: r.get(1)?,
        titular: r.get(2)?,
        tipo: r.get(3)?,
        valido_ate: r.get(4)?,
        dono_id: r.get(5)?,
        pfx_cifrado: r.get(6)?,
        senha_cifrada: r.get(7)?,
        emitente_json: r.get(8)?,
        ativo: r.get::<_, i64>(9)? == 1,
    })
}

/// Linha do SQLite → objeto JSON (nomes de coluna como chaves).
fn linha_json(r: &Row) -> rusqlite::Result<Map<String, Value>> {
    let mut m = Map::new();
    let st = r.as_ref();
    for i in 0..st.column_count() {
        let nome = st.column_name(i)?.to_string();
        let v = match r.get_ref(i)? {
            ValueRef::Null | ValueRef::Blob(_) => Value::Null,
            ValueRef::Integer(n) => Value::from(n),
            ValueRef::Real(f) => Value::from(f),
            ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).to_string()),
        };
        m.insert(nome, v);
    }
    Ok(m)
}

impl Repositorio {
    /// Abre (ou cria) o banco. Use ":memory:" para testes.
    pub fn abrir(arquivo: &str) -> Result<Self> {
        let db = if arquivo == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(pasta) = Path::new(arquivo).parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(pasta)?;
            }
            Connection::open(arquivo)?
        };
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.execute_batch(ESQUEMA)?;
        let versao: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if versao < 1 {
            // Recriar a tabela de produtos exige desligar as chaves estrangeiras durante a migração.
            db.pragma_update(None, "foreign_keys", "OFF")?;
            db.execute_batch(&format!("BEGIN; {MIGRACAO_1} PRAGMA user_version = 1; COMMIT;"))?;
            db.pragma_update(None, "foreign_keys", "ON")?;
        }
        if versao < 2 {
            db.execute_batch(&format!("BEGIN; {MIGRACAO_2} PRAGMA user_version = 2; COMMIT;"))?;
        }
        if versao < 3 {
            db.execute_batch(&format!("BEGIN; {MIGRACAO_3} PRAGMA user_version = 3; COMMIT;"))?;
        }
        Ok(Repositorio { db })
    }

    fn linhas<P: Params>(&self, sql: &str, p: P) -> Result<Vec<Value>> {
        let mut st = self.db.prepare(sql)?;
        let v = st.query_map(p, |r| linha_json(r).map(Value::Object))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn salvar_nota(&self, n: &NovaNota) -> Result<i64> {
        self.db.execute(
            "INSERT INTO notas (produtor_id, chave, serie, nNF, status, vNF, emit_doc, dest_doc, dest_nome, dados_json, xml_nfe)
             VALUES (:produtor, :chave, :serie, :nNF, :status, :vNF, :emit_doc, :dest_doc, :dest_nome, :dados_json, :xml_nfe)",
            named_params! {
                ":produtor": n.produtor_id, ":chave": n.chave, ":serie": n.serie, ":nNF": n.n_nf, ":status": n.status, ":vNF": n.v_nf,
                ":emit_doc": n.emit_doc, ":dest_doc": n.dest_doc, ":dest_nome": n.dest_nome,
                ":dados_json": n.dados_json, ":xml_nfe": n.xml_nfe,
            },
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Nota do produtor (None se não existe ou é de outro produtor).
    pub fn nota(&self, produtor: i64, id: i64) -> Result<Option<Nota>> {
        Ok(self
            .db
            .query_row(
                "SELECT id, chave, serie, nNF, status, nProt, emit_doc, dados_json, xml_nfe, xml_proc, xml_resposta, produtor_id
                 FROM notas WHERE id = ? AND produtor_id = ?",
                [id, produtor],
                |r| {
                    Ok(Nota {
                        id: r.get(0)?,
                        chave: r.get(1)?,
                        serie: r.get(2)?,
                        n_nf: r.get(3)?,
                        status: r.get(4)?,
                        n_prot: r.get(5)?,
                        emit_doc: r.get(6)?,
                        dados_json: r.get(7)?,
                        xml_nfe: r.get(8)?,
                        xml_proc: r.get(9)?,
                        xml_resposta: r.get(10)?,
                        produtor_id: r.get(11)?,
                    })
                },
            )
            .optional()?)
    }

    /// Nota como JSON para a API (sem os XMLs; `dados` já interpretado).
    pub fn nota_json(&self, produtor: i64, id: i64) -> Result<Option<Value>> {
        let mut v = self.linhas(
            "SELECT id, chave, serie, nNF, status, cStat, xMotivo, nProt, dhRecbto, vNF, emit_doc, dest_doc, dest_nome,
                    dados_json, criada_em, xml_proc IS NOT NULL AS tem_proc FROM notas WHERE id = ? AND produtor_id = ?",
            [id, produtor],
        )?;
        let Some(Value::Object(mut m)) = v.pop() else { return Ok(None) };
        let dados = m.remove("dados_json").and_then(|d| d.as_str().and_then(|s| serde_json::from_str(s).ok())).unwrap_or(Value::Null);
        m.insert("dados".into(), dados);
        let tem_proc = m.get("tem_proc").and_then(Value::as_i64) == Some(1);
        m.insert("tem_proc".into(), Value::Bool(tem_proc));
        m.insert("eventos".into(), Value::Array(self.eventos_da_nota(id)?));
        Ok(Some(Value::Object(m)))
    }

    pub fn listar_notas(&self, produtor: i64, filtro: &FiltroNotas, pagina: i64, limite: i64) -> Result<Value> {
        let vazio = |v: &Option<String>| v.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(String::from);
        let busca = vazio(&filtro.busca);
        let digitos = busca
            .as_deref()
            .filter(|b| !b.chars().any(char::is_alphabetic)) // nome com número não vira busca por documento
            .map(|b| b.chars().filter(char::is_ascii_digit).collect::<String>())
            .filter(|d| !d.is_empty());
        let so_numero = busca.as_deref().is_some_and(|b| b.chars().all(|c| c.is_ascii_digit()));
        let onde = "produtor_id = :produtor
             AND (:de IS NULL OR date(criada_em) >= :de)
             AND (:ate IS NULL OR date(criada_em) <= :ate)
             AND (:status IS NULL OR status = :status)
             AND (:busca IS NULL
                  OR dest_nome LIKE :busca ESCAPE '\\'
                  OR (:digitos IS NOT NULL AND (dest_doc LIKE :digitos ESCAPE '\\' OR chave LIKE :digitos ESCAPE '\\'))
                  OR (:numero IS NOT NULL AND nNF = :numero))";
        let numero = if so_numero { busca.as_deref().and_then(|b| b.parse::<i64>().ok()) } else { None };
        let (de, ate, status) = (vazio(&filtro.de), vazio(&filtro.ate), vazio(&filtro.status));
        let padrao_busca = busca.as_deref().map(padrao_like);
        let padrao_digitos = digitos.as_deref().map(padrao_like);
        let total: i64 = self.db.query_row(
            &format!("SELECT COUNT(*) FROM notas WHERE {onde}"),
            named_params! {
                ":produtor": produtor, ":de": de, ":ate": ate, ":status": status,
                ":busca": padrao_busca, ":digitos": padrao_digitos, ":numero": numero,
            },
            |r| r.get(0),
        )?;
        let notas = self.linhas(
            &format!(
                "SELECT id, chave, serie, nNF, status, cStat, xMotivo, nProt, dhRecbto, vNF, dest_doc, dest_nome, criada_em,
                        xml_proc IS NOT NULL AS tem_proc,
                        (SELECT COUNT(*) FROM eventos e WHERE e.nota_id = notas.id AND e.tipo = 'cce' AND e.registrado = 1) AS cces
                 FROM notas WHERE {onde} ORDER BY id DESC LIMIT :limite OFFSET :inicio"
            ),
            named_params! {
                ":produtor": produtor, ":de": de, ":ate": ate, ":status": status,
                ":busca": padrao_busca, ":digitos": padrao_digitos, ":numero": numero,
                ":limite": limite, ":inicio": (pagina - 1) * limite,
            },
        )?;
        Ok(serde_json::json!({ "total": total, "pagina": pagina, "porPagina": limite, "notas": notas }))
    }

    pub fn marcar_cancelada(&self, id: i64) -> Result<()> {
        self.db.execute("UPDATE notas SET status = 'cancelada' WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn atualizar_resultado(&self, id: i64, r: &Resultado) -> Result<()> {
        self.db.execute(
            "UPDATE notas SET status = :status, cStat = :cStat, xMotivo = :xMotivo, nProt = :nProt,
             dhRecbto = :dhRecbto, xml_proc = :xml_proc, xml_resposta = :xml_resposta WHERE id = :id",
            named_params! {
                ":status": r.status, ":cStat": r.c_stat, ":xMotivo": r.x_motivo, ":nProt": r.n_prot,
                ":dhRecbto": r.dh_recbto, ":xml_proc": r.xml_proc, ":xml_resposta": r.xml_resposta, ":id": id,
            },
        )?;
        Ok(())
    }

    /// A numeração é por emitente (produtor) e série.
    pub fn proximo_numero(&self, produtor: i64, serie: i64) -> Result<i64> {
        let a: Option<i64> = self.db.query_row(
            &format!("SELECT MAX(nNF) FROM notas WHERE produtor_id = ? AND serie = ? AND status IN {STATUS_QUE_CONSOMEM}"),
            [produtor, serie],
            |r| r.get(0),
        )?;
        let b: Option<i64> = self.db.query_row(
            "SELECT MAX(nNFFin) FROM inutilizacoes WHERE produtor_id = ? AND serie = ? AND homologada = 1",
            [produtor, serie],
            |r| r.get(0),
        )?;
        Ok(a.unwrap_or(0).max(b.unwrap_or(0)) + 1)
    }

    pub fn numero_ja_usado(&self, produtor: i64, serie: i64, n_nf: i64) -> Result<bool> {
        Ok(self
            .db
            .query_row(
                &format!(
                    "SELECT id FROM notas WHERE produtor_id = ? AND serie = ? AND nNF = ? AND status IN {STATUS_QUE_CONSOMEM} LIMIT 1"
                ),
                [produtor, serie, n_nf],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    }

    pub fn salvar_evento(&self, e: &NovoEvento) -> Result<i64> {
        self.db.execute(
            "INSERT INTO eventos (nota_id, tipo, nSeqEvento, texto, cStat, xMotivo, nProt, dhRegEvento, registrado, xml_proc, xml_resposta)
             VALUES (:nota_id, :tipo, :nSeq, :texto, :cStat, :xMotivo, :nProt, :dhReg, :registrado, :xml_proc, :xml_resposta)",
            named_params! {
                ":nota_id": e.nota_id, ":tipo": e.tipo, ":nSeq": e.n_seq_evento, ":texto": e.texto,
                ":cStat": e.c_stat, ":xMotivo": e.x_motivo, ":nProt": e.n_prot, ":dhReg": e.dh_reg_evento,
                ":registrado": e.registrado as i64, ":xml_proc": e.xml_proc, ":xml_resposta": e.xml_resposta,
            },
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn eventos_da_nota(&self, nota_id: i64) -> Result<Vec<Value>> {
        self.linhas(
            "SELECT id, tipo, nSeqEvento, texto, cStat, xMotivo, nProt, dhRegEvento, registrado, criado_em,
                    xml_proc IS NOT NULL AS tem_proc FROM eventos WHERE nota_id = ? ORDER BY id",
            [nota_id],
        )
    }

    /// (tipo, nSeqEvento, xml_proc, xml_resposta)
    pub fn evento(&self, produtor: i64, id: i64) -> Result<Option<(String, i64, Option<String>, Option<String>)>> {
        Ok(self
            .db
            .query_row(
                "SELECT e.tipo, e.nSeqEvento, e.xml_proc, e.xml_resposta FROM eventos e JOIN notas n ON n.id = e.nota_id
                 WHERE e.id = ? AND n.produtor_id = ?",
                [id, produtor],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?)
    }

    pub fn proximo_seq_cce(&self, nota_id: i64) -> Result<i64> {
        let n: Option<i64> = self.db.query_row(
            "SELECT MAX(nSeqEvento) FROM eventos WHERE nota_id = ? AND tipo = 'cce' AND registrado = 1",
            [nota_id],
            |r| r.get(0),
        )?;
        Ok(n.unwrap_or(0) + 1)
    }

    pub fn salvar_inutilizacao(&self, i: &NovaInutilizacao) -> Result<i64> {
        self.db.execute(
            "INSERT INTO inutilizacoes (produtor_id, serie, nNFIni, nNFFin, xJust, cStat, xMotivo, nProt, homologada, xml_proc, xml_resposta)
             VALUES (:produtor, :serie, :ini, :fim, :xJust, :cStat, :xMotivo, :nProt, :homologada, :xml_proc, :xml_resposta)",
            named_params! {
                ":produtor": i.produtor_id, ":serie": i.serie, ":ini": i.n_nf_ini, ":fim": i.n_nf_fin, ":xJust": i.x_just, ":cStat": i.c_stat,
                ":xMotivo": i.x_motivo, ":nProt": i.n_prot, ":homologada": i.homologada as i64,
                ":xml_proc": i.xml_proc, ":xml_resposta": i.xml_resposta,
            },
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn listar_inutilizacoes(&self, produtor: i64) -> Result<Vec<Value>> {
        self.linhas(
            "SELECT id, serie, nNFIni, nNFFin, xJust, cStat, xMotivo, nProt, homologada, criado_em FROM inutilizacoes
             WHERE produtor_id = ? ORDER BY id DESC",
            [produtor],
        )
    }

    // ---------- produtos ----------

    /// Produtos em ordem de descrição; cada um é o JSON do cadastro com o `id`.
    pub fn listar_produtos(&self, produtor: i64) -> Result<Vec<Value>> {
        let mut st = self.db.prepare(
            "SELECT p.id, p.dados_json,
                    COALESCE((SELECT SUM(m.quantidade) FROM estoque_movimentos m WHERE m.produto_id = p.id), 0)
             FROM produtos p WHERE p.produtor_id = ? ORDER BY p.xProd COLLATE NOCASE, p.cProd",
        )?;
        let linhas = st.query_map([produtor], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        let mut v = Vec::new();
        for l in linhas {
            let (id, dados, saldo) = l?;
            let mut m = serde_json::from_str::<Map<String, Value>>(&dados).unwrap_or_default();
            m.insert("id".into(), Value::from(id));
            m.insert("saldo".into(), Value::from(saldo as f64 / ESCALA_ESTOQUE));
            v.push(Value::Object(m));
        }
        Ok(v)
    }

    /// Id do produto com este código (sem diferenciar maiúsculas), se existir.
    pub fn produto_por_codigo(&self, produtor: i64, c_prod: &str) -> Result<Option<i64>> {
        Ok(self
            .db
            .query_row("SELECT id FROM produtos WHERE produtor_id = ? AND cProd = ?", params![produtor, c_prod], |r| r.get(0))
            .optional()?)
    }

    pub fn inserir_produto(&self, produtor: i64, c_prod: &str, x_prod: &str, dados_json: &str) -> Result<i64> {
        self.db.execute(
            "INSERT INTO produtos (produtor_id, cProd, xProd, dados_json) VALUES (?, ?, ?, ?)",
            params![produtor, c_prod, x_prod, dados_json],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Devolve false se o produto não existe.
    pub fn atualizar_produto(&self, produtor: i64, id: i64, c_prod: &str, x_prod: &str, dados_json: &str) -> Result<bool> {
        let n = self.db.execute(
            "UPDATE produtos SET cProd = ?, xProd = ?, dados_json = ?, alterado_em = datetime('now', 'localtime')
             WHERE id = ? AND produtor_id = ?",
            params![c_prod, x_prod, dados_json, id, produtor],
        )?;
        Ok(n > 0)
    }

    /// Devolve false se o produto não existe.
    pub fn excluir_produto(&self, produtor: i64, id: i64) -> Result<bool> {
        Ok(self.db.execute("DELETE FROM produtos WHERE id = ? AND produtor_id = ?", [id, produtor])? > 0)
    }

    // ---------- estoque ----------

    pub fn produto_estoque(&self, produtor: i64, id: i64) -> Result<Option<ProdutoEstoque>> {
        Ok(self
            .db
            .query_row(&format!("{SQL_PRODUTO_ESTOQUE} WHERE p.produtor_id = ? AND p.id = ?"), [produtor, id], produto_estoque_de)
            .optional()?)
    }

    pub fn produto_estoque_por_codigo(&self, produtor: i64, c_prod: &str) -> Result<Option<ProdutoEstoque>> {
        Ok(self
            .db
            .query_row(&format!("{SQL_PRODUTO_ESTOQUE} WHERE p.produtor_id = ? AND p.cProd = ?"), params![produtor, c_prod], produto_estoque_de)
            .optional()?)
    }

    /// Produtos com controle ligado e saldo abaixo do mínimo (ou negativo).
    pub fn produtos_com_estoque_baixo(&self, produtor: i64) -> Result<Vec<ProdutoEstoque>> {
        let mut st = self.db.prepare(&format!("{SQL_PRODUTO_ESTOQUE} WHERE p.produtor_id = ? ORDER BY p.xProd COLLATE NOCASE"))?;
        let todos = st.query_map([produtor], produto_estoque_de)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(todos.into_iter().filter(|p| p.controla && (p.saldo as f64) < p.minimo * ESCALA_ESTOQUE).collect())
    }

    /// Grava todos os movimentos ou nenhum.
    pub fn registrar_movimentos(&self, movimentos: &[NovoMovimento]) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        for m in movimentos {
            tx.execute(
                "INSERT INTO estoque_movimentos (produtor_id, produto_id, tipo, quantidade, nota_id, usuario_id, observacao)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![m.produtor_id, m.produto_id, m.tipo.nome(), m.quantidade, m.nota_id, m.usuario_id, m.observacao],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn movimentos_da_nota(&self, produtor: i64, nota_id: i64) -> Result<Vec<MovimentoDaNota>> {
        let mut st = self.db.prepare("SELECT produto_id, tipo, quantidade FROM estoque_movimentos WHERE produtor_id = ? AND nota_id = ? ORDER BY id")?;
        let v = st
            .query_map([produtor, nota_id], |r| {
                let tipo: String = r.get(1)?;
                Ok(MovimentoDaNota { produto_id: r.get(0)?, tipo: TipoMovimento::de_nome(&tipo).unwrap_or(TipoMovimento::Ajuste), quantidade: r.get(2)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Histórico mais recente de um produto, com o saldo depois de cada movimento.
    pub fn movimentos_do_produto(&self, produtor: i64, produto: i64, limite: i64) -> Result<Vec<Value>> {
        let mut v = self.linhas(
            "SELECT m.id, m.tipo, m.quantidade, m.observacao, m.criado_em, n.nNF, n.serie, u.nome AS usuario,
                    (SELECT SUM(a.quantidade) FROM estoque_movimentos a WHERE a.produto_id = m.produto_id AND a.id <= m.id) AS saldo
             FROM estoque_movimentos m
             LEFT JOIN notas n ON n.id = m.nota_id
             LEFT JOIN usuarios u ON u.id = m.usuario_id
             WHERE m.produtor_id = ? AND m.produto_id = ? ORDER BY m.id DESC LIMIT ?",
            params![produtor, produto, limite],
        )?;
        for m in &mut v {
            for campo in ["quantidade", "saldo"] {
                let q = m[campo].as_i64().unwrap_or(0) as f64 / ESCALA_ESTOQUE;
                m[campo] = Value::from(q);
            }
        }
        Ok(v)
    }

    // ---------- clientes ----------

    /// Clientes em ordem de nome; cada um é o JSON do cadastro com o `id`.
    pub fn listar_clientes(&self, produtor: i64) -> Result<Vec<Value>> {
        let mut st = self.db.prepare("SELECT id, dados_json FROM clientes WHERE produtor_id = ? ORDER BY xNome COLLATE NOCASE, documento")?;
        let linhas = st.query_map([produtor], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut v = Vec::new();
        for l in linhas {
            let (id, dados) = l?;
            let mut m = serde_json::from_str::<Map<String, Value>>(&dados).unwrap_or_default();
            m.insert("id".into(), Value::from(id));
            v.push(Value::Object(m));
        }
        Ok(v)
    }

    pub fn cliente_por_documento(&self, produtor: i64, documento: &str) -> Result<Option<i64>> {
        Ok(self
            .db
            .query_row("SELECT id FROM clientes WHERE produtor_id = ? AND documento = ?", params![produtor, documento], |r| r.get(0))
            .optional()?)
    }

    pub fn inserir_cliente(&self, produtor: i64, documento: &str, x_nome: &str, dados_json: &str) -> Result<i64> {
        self.db.execute(
            "INSERT INTO clientes (produtor_id, documento, xNome, dados_json) VALUES (?, ?, ?, ?)",
            params![produtor, documento, x_nome, dados_json],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Devolve false se o cliente não existe (ou é de outro produtor).
    pub fn atualizar_cliente(&self, produtor: i64, id: i64, documento: &str, x_nome: &str, dados_json: &str) -> Result<bool> {
        let n = self.db.execute(
            "UPDATE clientes SET documento = ?, xNome = ?, dados_json = ?, alterado_em = datetime('now', 'localtime')
             WHERE id = ? AND produtor_id = ?",
            params![documento, x_nome, dados_json, id, produtor],
        )?;
        Ok(n > 0)
    }

    pub fn excluir_cliente(&self, produtor: i64, id: i64) -> Result<bool> {
        Ok(self.db.execute("DELETE FROM clientes WHERE id = ? AND produtor_id = ?", [id, produtor])? > 0)
    }

    // ---------- painel e exportação ----------

    /// Resumo das notas com data de gravação entre `de` e `ate` (AAAA-MM-DD, inclusive).
    pub fn resumo_periodo(&self, produtor: i64, de: &str, ate: &str) -> Result<ResumoPeriodo> {
        Ok(self.db.query_row(
            &format!(
                "SELECT COALESCE(SUM(status = 'autorizada'), 0), COALESCE(SUM(status = 'cancelada'), 0),
                        COALESCE(SUM(status = 'rejeitada'), 0), COALESCE(SUM(status = 'pendente'), 0),
                        COALESCE(SUM(status = 'denegada'), 0),
                        COALESCE(SUM(CASE WHEN status = 'autorizada' THEN {CENTAVOS_SQL} END), 0)
                 FROM notas WHERE produtor_id = ? AND date(criada_em) BETWEEN ? AND ?"
            ),
            params![produtor, de, ate],
            |r| {
                Ok(ResumoPeriodo {
                    autorizadas: r.get(0)?,
                    canceladas: r.get(1)?,
                    rejeitadas: r.get(2)?,
                    pendentes: r.get(3)?,
                    denegadas: r.get(4)?,
                    valor_autorizado: r.get(5)?,
                })
            },
        )?)
    }

    /// Notas com valor fiscal no período: autorizadas, canceladas e denegadas.
    pub fn notas_para_exportar(&self, produtor: i64, de: &str, ate: &str) -> Result<Vec<NotaExportada>> {
        let mut st = self.db.prepare(&format!(
            "SELECT chave, serie, nNF, status, vNF, dest_doc, dest_nome, nProt, criada_em, xml_nfe, xml_proc FROM notas
             WHERE produtor_id = ? AND date(criada_em) BETWEEN ? AND ? AND status IN {STATUS_QUE_CONSOMEM}
             ORDER BY serie, nNF"
        ))?;
        let v = st
            .query_map(params![produtor, de, ate], |r| {
                Ok(NotaExportada {
                    chave: r.get(0)?,
                    serie: r.get(1)?,
                    n_nf: r.get(2)?,
                    status: r.get(3)?,
                    v_nf: r.get(4)?,
                    dest_doc: r.get(5)?,
                    dest_nome: r.get(6)?,
                    n_prot: r.get(7)?,
                    criada_em: r.get(8)?,
                    xml_nfe: r.get(9)?,
                    xml_proc: r.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Eventos registrados no período (pela data do evento, não da nota).
    pub fn eventos_para_exportar(&self, produtor: i64, de: &str, ate: &str) -> Result<Vec<DocumentoExportado>> {
        let mut st = self.db.prepare(
            "SELECT n.chave, e.tipo, e.nSeqEvento, e.xml_proc FROM eventos e JOIN notas n ON n.id = e.nota_id
             WHERE n.produtor_id = ? AND date(e.criado_em) BETWEEN ? AND ? AND e.registrado = 1 AND e.xml_proc IS NOT NULL
             ORDER BY e.id",
        )?;
        let v = st
            .query_map(params![produtor, de, ate], |r| {
                let (chave, tipo, seq): (String, String, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
                Ok(DocumentoExportado { nome: format!("{chave}-{tipo}-{seq}.xml"), xml: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn inutilizacoes_para_exportar(&self, produtor: i64, de: &str, ate: &str) -> Result<Vec<DocumentoExportado>> {
        let mut st = self.db.prepare(
            "SELECT serie, nNFIni, nNFFin, xml_proc FROM inutilizacoes
             WHERE produtor_id = ? AND date(criado_em) BETWEEN ? AND ? AND homologada = 1 AND xml_proc IS NOT NULL
             ORDER BY id",
        )?;
        let v = st
            .query_map(params![produtor, de, ate], |r| {
                let (serie, ini, fim): (i64, i64, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
                Ok(DocumentoExportado { nome: format!("inutilizacao-serie{serie}-{ini}-{fim}.xml"), xml: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    // ---------- usuários ----------

    pub fn contar_usuarios(&self) -> Result<i64> {
        Ok(self.db.query_row("SELECT COUNT(*) FROM usuarios", [], |r| r.get(0))?)
    }

    pub fn usuario(&self, id: i64) -> Result<Option<Usuario>> {
        Ok(self.db.query_row(&format!("SELECT {CAMPOS_USUARIO} FROM usuarios WHERE id = ?"), [id], usuario_de).optional()?)
    }

    pub fn usuario_por_login(&self, login: &str) -> Result<Option<Usuario>> {
        Ok(self.db.query_row(&format!("SELECT {CAMPOS_USUARIO} FROM usuarios WHERE login = ?"), [login], usuario_de).optional()?)
    }

    pub fn criar_usuario(&self, login: &str, nome: &str, senha_hash: &str, admin: bool) -> Result<i64> {
        self.db.execute(
            "INSERT INTO usuarios (login, nome, senha_hash, admin) VALUES (?, ?, ?, ?)",
            params![login, nome, senha_hash, admin as i64],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    pub fn listar_usuarios(&self) -> Result<Vec<Value>> {
        let mut v = self.linhas(
            "SELECT u.id, u.login, u.nome, u.admin, u.ativo, u.criado_em, u.ultimo_acesso,
                    (SELECT COUNT(*) FROM produtores p WHERE p.dono_id = u.id AND p.ativo = 1) AS produtores
             FROM usuarios u ORDER BY u.nome COLLATE NOCASE",
            [],
        )?;
        for u in &mut v {
            for campo in ["admin", "ativo"] {
                let b = u[campo].as_i64() == Some(1);
                u[campo] = Value::Bool(b);
            }
        }
        Ok(v)
    }

    pub fn atualizar_usuario(&self, id: i64, nome: &str, admin: bool, ativo: bool) -> Result<()> {
        self.db.execute(
            "UPDATE usuarios SET nome = ?, admin = ?, ativo = ? WHERE id = ?",
            params![nome, admin as i64, ativo as i64, id],
        )?;
        Ok(())
    }

    pub fn definir_senha(&self, id: i64, senha_hash: &str) -> Result<()> {
        self.db.execute("UPDATE usuarios SET senha_hash = ? WHERE id = ?", params![senha_hash, id])?;
        Ok(())
    }

    pub fn registrar_acesso(&self, id: i64) -> Result<()> {
        self.db.execute("UPDATE usuarios SET ultimo_acesso = datetime('now', 'localtime') WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn contar_admins_ativos(&self) -> Result<i64> {
        Ok(self.db.query_row("SELECT COUNT(*) FROM usuarios WHERE admin = 1 AND ativo = 1", [], |r| r.get(0))?)
    }

    // ---------- sessões ----------

    pub fn criar_sessao(&self, token_hash: &str, usuario_id: i64, agora: i64, expira_em: i64) -> Result<()> {
        self.db.execute("DELETE FROM sessoes WHERE expira_em <= ?", [agora])?;
        self.db.execute(
            "INSERT INTO sessoes (token_hash, usuario_id, criada_em, expira_em) VALUES (?, ?, ?, ?)",
            params![token_hash, usuario_id, agora, expira_em],
        )?;
        Ok(())
    }

    pub fn sessao(&self, token_hash: &str) -> Result<Option<Sessao>> {
        Ok(self
            .db
            .query_row("SELECT usuario_id, produtor_id, expira_em FROM sessoes WHERE token_hash = ?", [token_hash], |r| {
                Ok(Sessao { usuario_id: r.get(0)?, produtor_id: r.get(1)?, expira_em: r.get(2)? })
            })
            .optional()?)
    }

    pub fn renovar_sessao(&self, token_hash: &str, expira_em: i64) -> Result<()> {
        self.db.execute("UPDATE sessoes SET expira_em = ? WHERE token_hash = ?", params![expira_em, token_hash])?;
        Ok(())
    }

    pub fn excluir_sessao(&self, token_hash: &str) -> Result<()> {
        self.db.execute("DELETE FROM sessoes WHERE token_hash = ?", [token_hash])?;
        Ok(())
    }

    /// Encerra as sessões do usuário (todas, ou todas menos a atual).
    pub fn excluir_sessoes_do_usuario(&self, usuario_id: i64, exceto: Option<&str>) -> Result<()> {
        self.db.execute(
            "DELETE FROM sessoes WHERE usuario_id = ? AND token_hash != ?",
            params![usuario_id, exceto.unwrap_or("")],
        )?;
        Ok(())
    }

    pub fn selecionar_produtor(&self, token_hash: &str, produtor: Option<i64>) -> Result<()> {
        self.db.execute("UPDATE sessoes SET produtor_id = ? WHERE token_hash = ?", params![produtor, token_hash])?;
        Ok(())
    }

    // ---------- produtores ----------

    pub fn produtor(&self, id: i64) -> Result<Option<Produtor>> {
        Ok(self.db.query_row(&format!("SELECT {CAMPOS_PRODUTOR} FROM produtores WHERE id = ?"), [id], produtor_de).optional()?)
    }

    pub fn produtor_por_documento(&self, documento: &str) -> Result<Option<Produtor>> {
        Ok(self
            .db
            .query_row(&format!("SELECT {CAMPOS_PRODUTOR} FROM produtores WHERE documento = ?"), [documento], produtor_de)
            .optional()?)
    }

    /// Produtores ativos; `dono` = None lista todos (administrador).
    pub fn listar_produtores(&self, dono: Option<i64>) -> Result<Vec<Value>> {
        let mut v = self.linhas(
            "SELECT p.id, p.documento, p.titular, p.tipo, p.emissor_cert, p.valido_ate, p.dono_id, u.nome AS dono_nome,
                    p.emitente_json, p.criado_em,
                    (SELECT COUNT(*) FROM notas n WHERE n.produtor_id = p.id) AS notas
             FROM produtores p LEFT JOIN usuarios u ON u.id = p.dono_id
             WHERE p.ativo = 1 AND (?1 IS NULL OR p.dono_id = ?1)
             ORDER BY p.titular COLLATE NOCASE",
            [dono],
        )?;
        for p in &mut v {
            let emitente = p["emitente_json"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
            if let Some(m) = p.as_object_mut() {
                m.remove("emitente_json");
                m.insert("emitente".into(), emitente);
            }
        }
        Ok(v)
    }

    pub fn inserir_produtor(&self, c: &CertificadoProdutor, dono_id: i64) -> Result<i64> {
        self.db.execute(
            "INSERT INTO produtores (documento, titular, tipo, emissor_cert, valido_ate, dono_id, pfx_cifrado, senha_cifrada)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![c.documento, c.titular, c.tipo, c.emissor, c.valido_ate, dono_id, c.pfx_cifrado, c.senha_cifrada],
        )?;
        Ok(self.db.last_insert_rowid())
    }

    /// Troca o certificado (renovação) e reativa o produtor, se estava removido.
    pub fn atualizar_certificado(&self, id: i64, c: &CertificadoProdutor, dono_id: i64) -> Result<()> {
        self.db.execute(
            "UPDATE produtores SET titular = ?, tipo = ?, emissor_cert = ?, valido_ate = ?, pfx_cifrado = ?, senha_cifrada = ?,
                    dono_id = ?, ativo = 1, alterado_em = datetime('now', 'localtime') WHERE id = ?",
            params![c.titular, c.tipo, c.emissor, c.valido_ate, c.pfx_cifrado, c.senha_cifrada, dono_id, id],
        )?;
        Ok(())
    }

    pub fn definir_emitente(&self, id: i64, emitente_json: &str) -> Result<()> {
        self.db.execute(
            "UPDATE produtores SET emitente_json = ?, alterado_em = datetime('now', 'localtime') WHERE id = ?",
            params![emitente_json, id],
        )?;
        Ok(())
    }

    /// Remove o produtor: apaga o certificado guardado, mas mantém o histórico de notas.
    pub fn desativar_produtor(&self, id: i64) -> Result<()> {
        self.db.execute(
            "UPDATE produtores SET ativo = 0, pfx_cifrado = NULL, senha_cifrada = NULL, alterado_em = datetime('now', 'localtime')
             WHERE id = ?",
            [id],
        )?;
        self.db.execute("UPDATE sessoes SET produtor_id = NULL WHERE produtor_id = ?", [id])?;
        Ok(())
    }

    /// Dados de antes do login (sem produtor): as notas passam ao produtor com o mesmo
    /// documento do emitente; produtos e inutilizações, ao primeiro produtor cadastrado.
    pub fn adotar_dados_antigos(&self, produtor: i64, documento: &str) -> Result<()> {
        self.db.execute(
            "UPDATE notas SET produtor_id = ? WHERE produtor_id IS NULL AND emit_doc = ?",
            params![produtor, documento],
        )?;
        let total: i64 = self.db.query_row("SELECT COUNT(*) FROM produtores", [], |r| r.get(0))?;
        if total == 1 {
            self.db.execute("UPDATE produtos SET produtor_id = ? WHERE produtor_id IS NULL", [produtor])?;
            self.db.execute("UPDATE inutilizacoes SET produtor_id = ? WHERE produtor_id IS NULL", [produtor])?;
        }
        Ok(())
    }
}
