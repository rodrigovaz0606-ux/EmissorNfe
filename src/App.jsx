import React from 'react';
import { api } from './api.js';
import { notaVazia, exemploSoja, totaisNota, pendencias, avisos, paraPayload } from './modelo.js';
import { Identificacao } from './componentes/Identificacao.jsx';
import { Emitente } from './componentes/Emitente.jsx';
import { Destinatario } from './componentes/Destinatario.jsx';
import { Itens } from './componentes/Itens.jsx';
import { Pagamento } from './componentes/Pagamento.jsx';
import { Resumo } from './componentes/Resumo.jsx';

const CHAVE_RASCUNHO = 'emissor.rascunho';
const CHAVE_EMITENTE = 'emissor.emitente';
const CHAVE_NUMERO = 'emissor.proximoNumero';

const ler = (k) => { try { return JSON.parse(localStorage.getItem(k)); } catch { return null; } };
const gravar = (k, v) => localStorage.setItem(k, JSON.stringify(v));

// Atualização imutável por caminho: set(['dest', 'ender'], valor)
function comCaminho(obj, [chave, ...resto], valor) {
  if (chave === undefined) return valor;
  const atual = obj?.[chave];
  return Array.isArray(obj)
    ? obj.map((v, i) => (i === chave ? comCaminho(atual, resto, valor) : v))
    : { ...obj, [chave]: comCaminho(atual, resto, valor) };
}

function notaInicial() {
  const base = notaVazia();
  const emit = ler(CHAVE_EMITENTE);
  const numero = ler(CHAVE_NUMERO);
  return {
    ...base,
    emit: emit ? { ...base.emit, ...emit } : base.emit,
    ide: { ...base.ide, ...(numero ? { nNF: String(numero) } : {}) },
  };
}

function StatusSefaz() {
  const [estado, setEstado] = React.useState({ carregando: true });
  const consultar = React.useCallback(async () => {
    setEstado({ carregando: true });
    try {
      const r = await api.status();
      setEstado({ ok: r.emOperacao, texto: r.emOperacao ? `SEFAZ em operação · ${r.tMed ?? '-'}s` : `${r.cStat} ${r.xMotivo}` });
    } catch (e) {
      setEstado({ ok: false, texto: 'SEFAZ indisponível', detalhe: e.message });
    }
  }, []);
  React.useEffect(() => { consultar(); }, [consultar]);
  return (
    <button type="button" className={`status-sefaz ${estado.carregando ? '' : estado.ok ? 'ok' : 'falha'}`} onClick={consultar} title={estado.detalhe ?? 'Clique para consultar de novo'}>
      <span className="ponto" /> {estado.carregando ? 'Consultando SEFAZ…' : estado.texto}
    </button>
  );
}

export default function App() {
  const [emissor, setEmissor] = React.useState(null);
  const [erroEmissor, setErroEmissor] = React.useState('');
  const [nota, setNota] = React.useState(() => ler(CHAVE_RASCUNHO) ?? notaInicial());
  const [ocupado, setOcupado] = React.useState(null);
  const [resultado, setResultado] = React.useState(null);
  const [emitenteSalvo, setEmitenteSalvo] = React.useState(false);

  const set = React.useCallback((caminho, valor) => {
    setNota((n) => comCaminho(n, caminho, valor));
    setResultado(null);
    if (caminho[0] === 'emit') setEmitenteSalvo(false);
  }, []);

  // Rascunho salvo automaticamente no navegador.
  React.useEffect(() => { gravar(CHAVE_RASCUNHO, nota); }, [nota]);

  // Dados do emissor: o documento do emitente vem sempre do certificado.
  React.useEffect(() => {
    api.emissor().then((e) => {
      setEmissor(e);
      const doc = e.certificado.documento;
      setNota((n) => ({
        ...n,
        emit: {
          ...n.emit,
          CPF: doc.length === 11 ? doc : '',
          CNPJ: doc.length === 14 ? doc : '',
          xNome: n.emit.xNome || e.certificado.titular,
          ender: { ...n.emit.ender, UF: n.emit.ender.UF || e.uf },
        },
      }));
    }).catch((e) => setErroEmissor(e.message));
  }, []);

  const totais = totaisNota(nota);
  const listaPendencias = pendencias(nota);
  const listaAvisos = avisos(nota);
  const homologacao = emissor?.ambiente === 'homologacao';

  async function executar(modo) {
    if (modo === 'emitir' && !window.confirm(`Enviar a nota nº ${nota.ide.nNF} para a SEFAZ-${emissor?.uf} em HOMOLOGAÇÃO (sem valor fiscal)?`)) return;
    setOcupado(modo);
    setResultado(null);
    try {
      const payload = paraPayload(nota);
      const r = modo === 'validar' ? await api.validar(payload) : await api.emitir(payload);
      setResultado({ ...r, modo });
      if (r.protocolo?.autorizada) gravar(CHAVE_NUMERO, Number(nota.ide.nNF) + 1);
    } catch (e) {
      setResultado({ erro: e.message, modo });
    } finally {
      setOcupado(null);
    }
  }

  function salvarEmitentePadrao() {
    const { CPF, CNPJ, ...resto } = nota.emit;
    gravar(CHAVE_EMITENTE, resto);
    setEmitenteSalvo(true);
  }

  function novaNota() {
    if (!window.confirm('Começar uma nota nova? Os dados do destinatário e dos itens serão apagados.')) return;
    const proxima = resultado?.protocolo?.autorizada ? Number(nota.ide.nNF) + 1 : Number(ler(CHAVE_NUMERO) ?? nota.ide.nNF);
    const base = notaVazia();
    setNota({ ...base, emit: nota.emit, ide: { ...base.ide, serie: nota.ide.serie, nNF: String(proxima) } });
    setResultado(null);
  }

  return (
    <div className="app">
      <header className="topo">
        <div className="marca">
          <div className="logo">NF</div>
          <div>
            <h1>Emissor NF-e</h1>
            <span>Modelo 55 · {emissor ? `SEFAZ-${emissor.uf}` : '…'}</span>
          </div>
          {homologacao && <span className="selo-ambiente" title="As notas não têm valor fiscal">HOMOLOGAÇÃO</span>}
        </div>
        <nav className="atalhos">
          <a href="#identificacao">Identificação</a>
          <a href="#emitente">Emitente</a>
          <a href="#destinatario">Destinatário</a>
          <a href="#itens">Produtos</a>
          <a href="#pagamento">Pagamento</a>
        </nav>
        <div className="topo-direita">
          {emissor && (
            <div className="certificado" title={`Emitido por ${emissor.certificado.emissor}`}>
              <span>🔐 {emissor.certificado.titular}</span>
              <small>{emissor.certificado.tipo} · válido até {new Date(emissor.certificado.validoAte).toLocaleDateString('pt-BR')}</small>
            </div>
          )}
          <StatusSefaz />
        </div>
      </header>

      {erroEmissor && <div className="faixa-erro">Não foi possível falar com a API local: {erroEmissor}. Ela está rodando (npm run dev)?</div>}

      <div className="barra-acoes">
        <div>
          <h2>Nota fiscal nº {nota.ide.nNF || '—'} <small>série {nota.ide.serie}</small></h2>
          <span>Rascunho salvo automaticamente neste navegador</span>
        </div>
        <div className="barra-botoes">
          <button type="button" className="btn-texto" onClick={() => { setNota((n) => exemploSoja(n)); setResultado(null); }}>Carregar exemplo (soja)</button>
          <button type="button" className="btn-secundario" onClick={novaNota}>Nova nota</button>
        </div>
      </div>

      <main className="conteudo">
        <div className="formulario">
          <Identificacao nota={nota} set={set} />
          <Emitente nota={nota} set={set} certificado={emissor?.certificado} onSalvarPadrao={salvarEmitentePadrao} salvo={emitenteSalvo} />
          <Destinatario nota={nota} set={set} homologacao={homologacao} />
          <Itens nota={nota} set={set} />
          <Pagamento nota={nota} set={set} vNF={totais.vNF} />
        </div>
        <Resumo
          totais={totais} pendencias={listaPendencias} avisos={listaAvisos} ocupado={ocupado} resultado={resultado}
          onValidar={() => executar('validar')} onEmitir={() => executar('emitir')}
        />
      </main>
    </div>
  );
}
