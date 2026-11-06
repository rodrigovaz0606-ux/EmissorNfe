import React from 'react';
import { api } from '../api.js';
import { formatarChave } from '../modelo.js';
import { Janela, JanelaTexto } from './Janela.jsx';

const STATUS = {
  autorizada: ['Autorizada', 'ok'],
  rejeitada: ['Rejeitada', 'falha'],
  cancelada: ['Cancelada', 'neutro'],
  denegada: ['Denegada', 'falha'],
  pendente: ['Pendente', 'alerta'],
};
const TIPO_EVENTO = { cancelamento: 'Cancelamento', cce: 'Carta de correção' };

export const Selo = ({ status }) => {
  const [texto, classe] = STATUS[status] ?? [status, 'neutro'];
  return <span className={`selo selo-${classe}`}>{texto}</span>;
};
const moeda = (v) => Number(v).toLocaleString('pt-BR', { style: 'currency', currency: 'BRL' });
const dataHora = (s) => (s ? new Date(s.replace(' ', 'T')).toLocaleString('pt-BR', { dateStyle: 'short', timeStyle: 'short' }) : '');

function DetalheNota({ id, onFechar, onAlterada, onDuplicar }) {
  const [nota, setNota] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [acao, setAcao] = React.useState(null); // 'cancelar' | 'cce'
  const [aviso, setAviso] = React.useState(null);
  const [consultando, setConsultando] = React.useState(false);

  const carregar = React.useCallback(() => api.nota(id).then(setNota).catch((e) => setErro(e.message)), [id]);
  React.useEffect(() => { carregar(); }, [carregar]);

  async function aposEvento(r, rotulo) {
    setAcao(null);
    setAviso({ ok: r.registrado, texto: `${rotulo}: ${r.cStat} — ${r.xMotivo}${r.nProt ? ` (protocolo ${r.nProt})` : ''}` });
    await carregar();
    onAlterada();
  }

  async function consultar() {
    setConsultando(true);
    try {
      const r = await api.consultar(id);
      setAviso({ ok: ['100', '101', '150', '151'].includes(r.cStat), texto: `Situação na SEFAZ: ${r.cStat} — ${r.xMotivo}` });
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    } finally {
      setConsultando(false);
    }
  }

  if (erro) return <Janela titulo="Nota" onFechar={onFechar}><p>{erro}</p></Janela>;
  if (!nota) return <Janela titulo="Carregando…" onFechar={onFechar}><p>Carregando…</p></Janela>;

  const autorizada = nota.status === 'autorizada';
  if (acao === 'cancelar') {
    return (
      <JanelaTexto
        titulo={`Cancelar nota nº ${nota.nNF}`} perigo textoBotao="Cancelar a nota" rotulo="Justificativa" min={15} max={255}
        explicacao="O cancelamento é definitivo. Normalmente só é aceito em até 24 horas após a autorização e se a mercadoria não circulou."
        onConfirmar={async (t) => aposEvento(await api.cancelar(id, t), 'Cancelamento')} onFechar={() => setAcao(null)}
      />
    );
  }
  if (acao === 'cce') {
    return (
      <JanelaTexto
        titulo={`Carta de correção — nota nº ${nota.nNF}`} textoBotao="Enviar carta de correção" rotulo="Correção" min={15} max={1000}
        explicacao="A CC-e não pode alterar valores, impostos, quantidades, remetente/destinatário nem a data de emissão. Cada nova carta substitui a anterior, então descreva todas as correções."
        onConfirmar={async (t) => aposEvento(await api.cce(id, t), 'Carta de correção')} onFechar={() => setAcao(null)}
      />
    );
  }

  return (
    <Janela titulo={`Nota nº ${nota.nNF} · série ${nota.serie}`} onFechar={onFechar} largura={720}>
      <div className="detalhe-topo">
        <Selo status={nota.status} />
        <strong>{moeda(nota.vNF)}</strong>
      </div>
      <dl className="detalhes">
        <dt>Chave</dt><dd><code>{formatarChave(nota.chave)}</code></dd>
        <dt>Destinatário</dt><dd>{nota.dados?.dest?.xNome} · {nota.dest_doc}</dd>
        <dt>SEFAZ</dt><dd>{nota.cStat} — {nota.xMotivo}</dd>
        {nota.nProt && (<><dt>Protocolo</dt><dd>{nota.nProt} · {nota.dhRecbto}</dd></>)}
        <dt>Gravada em</dt><dd>{dataHora(nota.criada_em)}</dd>
      </dl>

      {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}

      <div className="acoes-nota">
        <a className="btn-secundario" href={api.urlDanfe(id)} target="_blank" rel="noreferrer">DANFE (PDF)</a>
        <a className="btn-secundario" href={api.urlXml(id)}>{nota.tem_proc ? 'XML autorizado' : 'XML enviado'}</a>
        <a className="btn-texto" href={api.urlXml(id, 'resposta')}>Resposta da SEFAZ</a>
        <span className="espaco" />
        <button type="button" className="btn-secundario" onClick={() => onDuplicar(nota.dados)} disabled={!nota.dados} title="Abre uma nota nova com os mesmos dados">Duplicar nota</button>
        <button type="button" className="btn-secundario" onClick={consultar} disabled={consultando}>{consultando ? 'Consultando…' : 'Consultar na SEFAZ'}</button>
        <button type="button" className="btn-secundario" onClick={() => setAcao('cce')} disabled={!autorizada} title={autorizada ? '' : 'Só para nota autorizada'}>Carta de correção</button>
        <button type="button" className="btn-perigo" onClick={() => setAcao('cancelar')} disabled={!autorizada} title={autorizada ? '' : 'Só para nota autorizada'}>Cancelar nota</button>
      </div>

      <h3 className="subtitulo">Eventos</h3>
      {nota.eventos.length === 0 ? (
        <p className="vazio-pequeno">Nenhum evento registrado.</p>
      ) : (
        <ul className="eventos">
          {nota.eventos.map((e) => (
            <li key={e.id}>
              <span className={`selo selo-${e.registrado ? 'ok' : 'falha'}`}>{e.registrado ? 'Registrado' : 'Rejeitado'}</span>
              <div>
                <strong>{TIPO_EVENTO[e.tipo]}{e.tipo === 'cce' ? ` nº ${e.nSeqEvento}` : ''}</strong>
                <div className="evento-texto">{e.texto}</div>
                <div className="evento-meta">{e.cStat} — {e.xMotivo} · {dataHora(e.criado_em)}</div>
              </div>
              {Boolean(e.tem_proc) && <a className="btn-texto" href={api.urlEventoXml(e.id)}>XML</a>}
            </li>
          ))}
        </ul>
      )}
    </Janela>
  );
}

function JanelaInutilizar({ onFechar, onConcluida }) {
  const [dados, setDados] = React.useState({ serie: '1', nNFIni: '', nNFFin: '' });
  const [etapa, setEtapa] = React.useState('faixa');
  if (etapa === 'justificativa') {
    return (
      <JanelaTexto
        titulo={`Inutilizar nº ${dados.nNFIni} a ${dados.nNFFin} (série ${dados.serie})`} perigo textoBotao="Inutilizar"
        rotulo="Justificativa" min={15} max={255} explicacao="A inutilização é definitiva: esses números nunca poderão ser usados."
        onConfirmar={async (t) => onConcluida(await api.inutilizar({ ...dados, xJust: t }))} onFechar={onFechar}
      />
    );
  }
  const ini = Number(dados.nNFIni);
  const fim = Number(dados.nNFFin);
  const ok = ini >= 1 && fim >= ini;
  return (
    <Janela titulo="Inutilizar numeração" onFechar={onFechar}>
      <div className="explicacao">Use quando uma faixa de números foi pulada e não será usada.</div>
      <div className="grade">
        {[['serie', 'Série'], ['nNFIni', 'Número inicial'], ['nNFFin', 'Número final']].map(([k, r]) => (
          <label className="campo l-m" key={k}>
            <span className="rotulo">{r}</span>
            <input inputMode="numeric" value={dados[k]} onChange={(e) => setDados({ ...dados, [k]: e.target.value.replace(/\D/g, '') })} />
          </label>
        ))}
      </div>
      <div className="janela-botoes">
        <button type="button" className="btn-secundario" onClick={onFechar}>Voltar</button>
        <button type="button" className="btn-primario" disabled={!ok} onClick={() => setEtapa('justificativa')}>Continuar</button>
      </div>
    </Janela>
  );
}

const SITUACOES = [['', 'Todas'], ['autorizada', 'Autorizada'], ['rejeitada', 'Rejeitada'], ['cancelada', 'Cancelada'], ['denegada', 'Denegada'], ['pendente', 'Pendente']];

const aaaaMmDd = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
// Primeiro e último dia do mês, `atras` meses antes do atual.
function periodoDoMes(atras) {
  const h = new Date();
  return { de: aaaaMmDd(new Date(h.getFullYear(), h.getMonth() - atras, 1)), ate: aaaaMmDd(new Date(h.getFullYear(), h.getMonth() - atras + 1, 0)) };
}

function JanelaExportar({ onFechar }) {
  const [p, setP] = React.useState(() => periodoDoMes(1));
  const [enviando, setEnviando] = React.useState(false);
  const [erro, setErro] = React.useState('');
  const [ok, setOk] = React.useState(false);

  async function exportar() {
    setEnviando(true);
    setErro('');
    setOk(false);
    try {
      await api.exportarNotas(p.de, p.ate);
      setOk(true);
    } catch (e) {
      setErro(e.message);
    } finally {
      setEnviando(false);
    }
  }

  return (
    <Janela titulo="Exportar para o contador" onFechar={enviando ? () => {} : onFechar}>
      <div className="explicacao">
        Gera um ZIP com os XMLs das notas autorizadas, canceladas e denegadas, os eventos e as inutilizações do período, mais um resumo.csv que abre no Excel.
        Notas rejeitadas ou pendentes não entram, porque não têm valor fiscal.
      </div>
      <div className="atalhos-periodo">
        <button type="button" className="btn-texto" onClick={() => setP(periodoDoMes(1))}>Mês passado</button>
        <button type="button" className="btn-texto" onClick={() => setP(periodoDoMes(0))}>Este mês</button>
      </div>
      <div className="grade">
        <label className="campo l-g"><span className="rotulo">De</span><input type="date" value={p.de} onChange={(e) => setP({ ...p, de: e.target.value })} /></label>
        <label className="campo l-g"><span className="rotulo">Até</span><input type="date" value={p.ate} onChange={(e) => setP({ ...p, ate: e.target.value })} /></label>
      </div>
      {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
      {ok && <div className="aviso-local ok">Arquivo gerado. Veja a pasta de downloads do navegador.</div>}
      <div className="janela-botoes">
        <button type="button" className="btn-secundario" onClick={onFechar} disabled={enviando}>Fechar</button>
        <button type="button" className="btn-primario" onClick={exportar} disabled={enviando || !p.de || !p.ate}>{enviando ? 'Gerando…' : 'Baixar ZIP'}</button>
      </div>
    </Janela>
  );
}

const FILTROS_VAZIOS = { de: '', ate: '', status: '', busca: '' };

export function Notas({ emissor, filtroInicial, onDuplicar }) {
  const [dados, setDados] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [aberta, setAberta] = React.useState(null);
  const [inutilizando, setInutilizando] = React.useState(false);
  const [exportando, setExportando] = React.useState(false);
  const [aviso, setAviso] = React.useState(null);
  const [filtros, setFiltros] = React.useState(() => ({ ...FILTROS_VAZIOS, ...filtroInicial }));
  const [pagina, setPagina] = React.useState(1);
  const [buscaAplicada, setBuscaAplicada] = React.useState(filtros.busca);
  const pedido = React.useRef(0);

  // A busca espera a pessoa parar de digitar.
  React.useEffect(() => {
    const t = setTimeout(() => { setBuscaAplicada(filtros.busca); setPagina(1); }, 300);
    return () => clearTimeout(t);
  }, [filtros.busca]);

  const carregar = React.useCallback(() => {
    pedido.current += 1;
    const este = pedido.current;
    setErro('');
    return api.notas(pagina, { de: filtros.de, ate: filtros.ate, status: filtros.status, busca: buscaAplicada })
      .then((d) => { if (este === pedido.current) setDados(d); }) // ignora resposta atrasada de um filtro antigo
      .catch((e) => { if (este === pedido.current) setErro(e.message); });
  }, [pagina, filtros.de, filtros.ate, filtros.status, buscaAplicada]);
  React.useEffect(() => { carregar(); }, [carregar]);

  const filtrar = (campo) => (e) => {
    setFiltros((f) => ({ ...f, [campo]: e.target.value }));
    if (campo !== 'busca') setPagina(1);
  };
  const temFiltro = Object.values(filtros).some(Boolean);
  const paginas = dados ? Math.max(1, Math.ceil(dados.total / (dados.porPagina || 50))) : 1;

  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>Notas emitidas</h2>
          <span>{dados ? `${dados.total} ${dados.total === 1 ? 'nota' : 'notas'}${temFiltro ? (dados.total === 1 ? ' encontrada' : ' encontradas') : ''}` : 'Carregando…'}</span>
        </div>
        <div className="barra-botoes">
          <button type="button" className="btn-secundario" onClick={() => setExportando(true)}>Exportar para o contador</button>
          <button
            type="button" className="btn-secundario" onClick={() => setInutilizando(true)} disabled={!emissor?.podeInutilizar}
            title={emissor?.podeInutilizar ? '' : 'A inutilização exige emitente com CNPJ'}
          >
            Inutilizar numeração
          </button>
          <button type="button" className="btn-secundario" onClick={carregar}>Atualizar</button>
        </div>
      </div>

      <div className="conteudo-lista">
        <div className="filtros-notas">
          <label className="campo filtro-busca"><span className="rotulo">Buscar</span>
            <input type="search" value={filtros.busca} onChange={filtrar('busca')} placeholder="Número, destinatário, CPF/CNPJ ou chave" />
          </label>
          <label className="campo"><span className="rotulo">De</span><input type="date" value={filtros.de} onChange={filtrar('de')} /></label>
          <label className="campo"><span className="rotulo">Até</span><input type="date" value={filtros.ate} onChange={filtrar('ate')} /></label>
          <label className="campo"><span className="rotulo">Situação</span>
            <select value={filtros.status} onChange={filtrar('status')}>
              {SITUACOES.map(([v, t]) => <option key={v} value={v}>{t}</option>)}
            </select>
          </label>
          {temFiltro && <button type="button" className="btn-texto" onClick={() => { setFiltros(FILTROS_VAZIOS); setBuscaAplicada(''); setPagina(1); }}>Limpar filtros</button>}
        </div>

        {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}
        {erro && <div className="aviso-local falha">{erro}</div>}
        {dados && dados.notas.length === 0 && (
          <div className="vazio">
            <div className="vazio-icone">🧾</div>
            {temFiltro ? (
              <>
                <h3>Nenhuma nota encontrada</h3>
                <p>Nenhuma nota combina com esses filtros.</p>
              </>
            ) : (
              <>
                <h3>Nenhuma nota ainda</h3>
                <p>As notas enviadas à SEFAZ aparecem aqui, inclusive as rejeitadas.</p>
              </>
            )}
          </div>
        )}
        {dados && dados.notas.length > 0 && (
          <table className="tabela-notas">
            <thead>
              <tr><th>Nº</th><th>Série</th><th>Data</th><th>Destinatário</th><th className="direita">Valor</th><th>Situação</th><th /></tr>
            </thead>
            <tbody>
              {dados.notas.map((n) => (
                <tr key={n.id} onClick={() => setAberta(n.id)} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && setAberta(n.id)}>
                  <td><strong>{n.nNF}</strong></td>
                  <td>{n.serie}</td>
                  <td>{dataHora(n.criada_em)}</td>
                  <td className="cortar">{n.dest_nome}</td>
                  <td className="direita">{moeda(n.vNF)}</td>
                  <td>
                    <Selo status={n.status} />
                    {n.cces > 0 && <span className="selo selo-neutro">CC-e {n.cces}</span>}
                    {n.status === 'rejeitada' && <div className="motivo" title={n.xMotivo}>{n.cStat} — {n.xMotivo}</div>}
                  </td>
                  <td className="direita"><span className="btn-texto">Abrir ›</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {paginas > 1 && (
          <div className="paginacao">
            <button type="button" className="btn-secundario" onClick={() => setPagina(pagina - 1)} disabled={pagina <= 1}>‹ Anterior</button>
            <span>Página {pagina} de {paginas}</span>
            <button type="button" className="btn-secundario" onClick={() => setPagina(pagina + 1)} disabled={pagina >= paginas}>Próxima ›</button>
          </div>
        )}
      </div>

      {aberta && <DetalheNota id={aberta} onFechar={() => setAberta(null)} onAlterada={carregar} onDuplicar={onDuplicar} />}
      {exportando && <JanelaExportar onFechar={() => setExportando(false)} />}
      {inutilizando && (
        <JanelaInutilizar
          onFechar={() => setInutilizando(false)}
          onConcluida={(r) => { setInutilizando(false); setAviso({ ok: r.homologada, texto: `Inutilização: ${r.cStat} — ${r.xMotivo}` }); carregar(); }}
        />
      )}
    </div>
  );
}
