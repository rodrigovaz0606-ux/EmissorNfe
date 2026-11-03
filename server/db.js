// Persistência em SQLite: notas, eventos (cancelamento/CC-e) e inutilizações.
import fs from 'node:fs';
import path from 'node:path';
import Database from 'better-sqlite3';

const ESQUEMA = `
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
`;

// Números "consumidos": notas que a SEFAZ registrou (autorizadas, canceladas, denegadas)
// e faixas inutilizadas. Número de nota rejeitada pode ser reutilizado.
const STATUS_QUE_CONSOMEM = ['autorizada', 'cancelada', 'denegada'];

export function abrirBanco(arquivo = process.env.NFE_DB || path.resolve('data', 'emissor.sqlite')) {
  if (arquivo !== ':memory:') fs.mkdirSync(path.dirname(arquivo), { recursive: true });
  const db = new Database(arquivo);
  db.pragma('journal_mode = WAL');
  db.pragma('foreign_keys = ON');
  db.exec(ESQUEMA);
  return db;
}

export function repositorio(db) {
  const st = {
    inserirNota: db.prepare(`INSERT INTO notas (chave, serie, nNF, status, cStat, xMotivo, nProt, dhRecbto, vNF, emit_doc, dest_doc, dest_nome, dados_json, xml_nfe, xml_proc, xml_resposta)
      VALUES (@chave, @serie, @nNF, @status, @cStat, @xMotivo, @nProt, @dhRecbto, @vNF, @emit_doc, @dest_doc, @dest_nome, @dados_json, @xml_nfe, @xml_proc, @xml_resposta)`),
    nota: db.prepare('SELECT * FROM notas WHERE id = ?'),
    listar: db.prepare(`SELECT id, chave, serie, nNF, status, cStat, xMotivo, nProt, dhRecbto, vNF, dest_doc, dest_nome, criada_em,
        xml_proc IS NOT NULL AS tem_proc,
        (SELECT COUNT(*) FROM eventos e WHERE e.nota_id = notas.id AND e.tipo = 'cce' AND e.registrado = 1) AS cces
      FROM notas ORDER BY id DESC LIMIT ? OFFSET ?`),
    contar: db.prepare('SELECT COUNT(*) AS total FROM notas'),
    atualizarStatus: db.prepare('UPDATE notas SET status = ? WHERE id = ?'),
    atualizarResultado: db.prepare(`UPDATE notas SET status = @status, cStat = @cStat, xMotivo = @xMotivo, nProt = @nProt,
      dhRecbto = @dhRecbto, xml_proc = @xml_proc, xml_resposta = @xml_resposta WHERE id = @id`),
    maiorNota: db.prepare(`SELECT MAX(nNF) AS n FROM notas WHERE serie = ? AND status IN (${STATUS_QUE_CONSOMEM.map(() => '?').join(',')})`),
    maiorInut: db.prepare('SELECT MAX(nNFFin) AS n FROM inutilizacoes WHERE serie = ? AND homologada = 1'),
    numeroUsado: db.prepare(`SELECT id FROM notas WHERE serie = ? AND nNF = ? AND status IN (${STATUS_QUE_CONSOMEM.map(() => '?').join(',')}) LIMIT 1`),
    inserirEvento: db.prepare(`INSERT INTO eventos (nota_id, tipo, nSeqEvento, texto, cStat, xMotivo, nProt, dhRegEvento, registrado, xml_proc, xml_resposta)
      VALUES (@nota_id, @tipo, @nSeqEvento, @texto, @cStat, @xMotivo, @nProt, @dhRegEvento, @registrado, @xml_proc, @xml_resposta)`),
    eventosDaNota: db.prepare('SELECT id, tipo, nSeqEvento, texto, cStat, xMotivo, nProt, dhRegEvento, registrado, criado_em, xml_proc IS NOT NULL AS tem_proc FROM eventos WHERE nota_id = ? ORDER BY id'),
    evento: db.prepare('SELECT * FROM eventos WHERE id = ?'),
    maiorSeqCce: db.prepare("SELECT MAX(nSeqEvento) AS n FROM eventos WHERE nota_id = ? AND tipo = 'cce' AND registrado = 1"),
    inserirInut: db.prepare(`INSERT INTO inutilizacoes (serie, nNFIni, nNFFin, xJust, cStat, xMotivo, nProt, homologada, xml_proc, xml_resposta)
      VALUES (@serie, @nNFIni, @nNFFin, @xJust, @cStat, @xMotivo, @nProt, @homologada, @xml_proc, @xml_resposta)`),
    listarInut: db.prepare('SELECT id, serie, nNFIni, nNFFin, xJust, cStat, xMotivo, nProt, homologada, criado_em FROM inutilizacoes ORDER BY id DESC'),
  };

  return {
    salvarNota(n) {
      const valores = {
        cStat: null, xMotivo: null, nProt: null, dhRecbto: null, emit_doc: null, dest_doc: null, dest_nome: null, xml_proc: null, xml_resposta: null, ...n,
      };
      return Number(st.inserirNota.run(valores).lastInsertRowid);
    },
    nota: (id) => st.nota.get(id) ?? null,
    listarNotas({ limite = 50, pagina = 1 } = {}) {
      return { total: st.contar.get().total, notas: st.listar.all(limite, (pagina - 1) * limite) };
    },
    marcarCancelada: (id) => st.atualizarStatus.run('cancelada', id),
    atualizarResultado(id, r) {
      st.atualizarResultado.run({ cStat: null, xMotivo: null, nProt: null, dhRecbto: null, xml_proc: null, xml_resposta: null, ...r, id });
    },

    proximoNumero(serie) {
      const a = st.maiorNota.get(serie, ...STATUS_QUE_CONSOMEM).n ?? 0;
      const b = st.maiorInut.get(serie).n ?? 0;
      return Math.max(a, b) + 1;
    },
    numeroJaUsado: (serie, nNF) => Boolean(st.numeroUsado.get(serie, nNF, ...STATUS_QUE_CONSOMEM)),

    salvarEvento(e) {
      return Number(st.inserirEvento.run({ cStat: null, xMotivo: null, nProt: null, dhRegEvento: null, xml_proc: null, xml_resposta: null, ...e, registrado: e.registrado ? 1 : 0 }).lastInsertRowid);
    },
    eventosDaNota: (notaId) => st.eventosDaNota.all(notaId),
    evento: (id) => st.evento.get(id) ?? null,
    proximoSeqCce: (notaId) => (st.maiorSeqCce.get(notaId).n ?? 0) + 1,

    salvarInutilizacao(i) {
      return Number(st.inserirInut.run({ cStat: null, xMotivo: null, nProt: null, xml_proc: null, xml_resposta: null, ...i, homologada: i.homologada ? 1 : 0 }).lastInsertRowid);
    },
    listarInutilizacoes: () => st.listarInut.all(),
  };
}
