import React from 'react';
import { api, EVENTO_SESSAO } from './api.js';
import { notaVazia, totaisNota, pendencias, avisos, avisosEstoque, paraPayload, notaDoPayload, digitos } from './modelo.js';
import { Identificacao } from './componentes/Identificacao.jsx';
import { Emitente } from './componentes/Emitente.jsx';
import { Destinatario } from './componentes/Destinatario.jsx';
import { Itens } from './componentes/Itens.jsx';
import { Pagamento } from './componentes/Pagamento.jsx';
import { Resumo } from './componentes/Resumo.jsx';
import { Notas } from './componentes/Notas.jsx';
import { Produtos } from './componentes/Produtos.jsx';
import { Clientes } from './componentes/Clientes.jsx';
import { Painel } from './componentes/Painel.jsx';
import { Produtores, formatarDoc } from './componentes/Produtores.jsx';
import { Admin, JanelaSenha } from './componentes/Admin.jsx';
import { TelaLogin, TelaConfiguracao } from './componentes/Acesso.jsx';
import { Abas, BotaoTema } from './componentes/Movimento.jsx';

const ABAS_EMISSAO = [['painel', 'Painel'], ['emitir', 'Emitir nota'], ['notas', 'Notas emitidas'], ['clientes', 'Clientes'], ['produtos', 'Produtos']];
// Rascunho da nota por produtor (trocar de produtor não mistura os rascunhos).
const chaveRascunho = (produtorId) => `emissor.rascunho.${produtorId}`;

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

// Nota nova com o emitente padrão do produtor (guardado no servidor).
function notaInicial(produtor) {
  const base = notaVazia();
  const emit = produtor?.emitente;
  return { ...base, emit: emit ? { ...base.emit, ...emit, ender: { ...base.emit.ender, ...emit.ender } } : base.emit };
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
    <button type="button" className={`status-sefaz ${estado.carregando ? '' : estado.ok ? 'ok' : 'falha'}`} onClick={consultar} title={`${estado.texto ?? 'Consultando'}${estado.detalhe ? ` — ${estado.detalhe}` : ''} (clique para consultar de novo)`}>
      <span className="ponto" /> {estado.carregando ? 'Consultando SEFAZ…' : estado.texto}
    </button>
  );
}

function MenuUsuario({ usuario, onSenha, onSair }) {
  const [aberto, setAberto] = React.useState(false);
  const ref = React.useRef(null);
  React.useEffect(() => {
    if (!aberto) return undefined;
    const fora = (e) => !ref.current?.contains(e.target) && setAberto(false);
    const esc = (e) => e.key === 'Escape' && setAberto(false);
    document.addEventListener('mousedown', fora);
    document.addEventListener('keydown', esc);
    return () => { document.removeEventListener('mousedown', fora); document.removeEventListener('keydown', esc); };
  }, [aberto]);
  return (
    <div className="menu-usuario" ref={ref}>
      <button type="button" className="botao-usuario" aria-haspopup="menu" aria-expanded={aberto} onClick={() => setAberto(!aberto)}>
        <span className="avatar" aria-hidden="true">{usuario.nome.trim()[0]?.toUpperCase()}</span>
        <span className="nome-usuario">{usuario.nome}{usuario.admin && <small>Administrador</small>}</span>
        <span aria-hidden="true">▾</span>
      </button>
      {aberto && (
        <div className="menu-lista" role="menu">
          <div className="menu-cabecalho">Entrou como <strong>{usuario.login}</strong></div>
          <button type="button" role="menuitem" onClick={() => { setAberto(false); onSenha(); }}>Alterar minha senha</button>
          <button type="button" role="menuitem" onClick={() => { setAberto(false); onSair(); }}>Sair</button>
        </div>
      )}
    </div>
  );
}

// Formulário de emissão do produtor ativo.
function Emissao({ produtor, emissor, onVerNotas, produtos, carregarProdutos, clientes, carregarClientes }) {
  const chave = chaveRascunho(produtor.id);
  const [nota, setNota] = React.useState(() => ler(chave) ?? notaInicial(produtor));
  const [ocupado, setOcupado] = React.useState(null);
  const [resultado, setResultado] = React.useState(null);
  const [emitenteSalvo, setEmitenteSalvo] = React.useState(false);

  const set = React.useCallback((caminho, valor) => {
    setNota((n) => comCaminho(n, caminho, valor));
    setResultado(null);
    if (caminho[0] === 'emit') setEmitenteSalvo(false);
  }, []);

  // Rascunho salvo automaticamente no navegador.
  React.useEffect(() => { gravar(chave, nota); }, [chave, nota]);

  // O documento do emitente vem sempre do certificado do produtor.
  React.useEffect(() => {
    if (!emissor) return;
    const doc = emissor.certificado.documento ?? '';
    setNota((n) => ({
      ...n,
      emit: {
        ...n.emit,
        CPF: doc.length === 11 ? doc : '',
        CNPJ: doc.length === 14 ? doc : '',
        xNome: n.emit.xNome || emissor.certificado.titular,
        ender: { ...n.emit.ender, UF: n.emit.ender.UF || emissor.uf },
      },
    }));
  }, [emissor]);

  // Sugere o próximo número livre da série (pelo banco), se o campo ainda estiver no padrão.
  const sugerirNumero = React.useCallback(async (serie, forcar = false) => {
    try {
      const { proximo } = await api.proximoNumero(serie || 1);
      setNota((n) => (forcar || n.ide.nNF === '' || n.ide.nNF === '1' ? { ...n, ide: { ...n.ide, nNF: String(proximo) } } : n));
    } catch { /* API fora do ar: mantém o número digitado */ }
  }, []);
  React.useEffect(() => { sugerirNumero(nota.ide.serie); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const totais = totaisNota(nota);
  const listaPendencias = pendencias(nota);
  const listaAvisos = [...avisos(nota), ...avisosEstoque(nota, produtos)];
  const homologacao = emissor?.ambiente === 'homologacao';

  async function executar(modo) {
    const destino = emissor?.simulada ? 'a SEFAZ SIMULADA (nada é enviado)' : `a SEFAZ-${emissor?.uf} em HOMOLOGAÇÃO (sem valor fiscal)`;
    if (modo === 'emitir' && !window.confirm(`Enviar a nota nº ${nota.ide.nNF} de ${produtor.titular} para ${destino}?`)) return;
    setOcupado(modo);
    setResultado(null);
    try {
      const payload = paraPayload(nota);
      if (modo === 'previa') {
        await api.previaDanfe(payload);
      } else {
        const r = modo === 'validar' ? await api.validar(payload) : await api.emitir(payload);
        setResultado({ ...r, modo });
        if (modo === 'emitir' && r.estoque) carregarProdutos(); // o saldo mudou
      }
    } catch (e) {
      setResultado({ erro: e.message, modo });
    } finally {
      setOcupado(null);
    }
  }

  async function salvarEmitentePadrao() {
    const { CPF, CNPJ, ...resto } = nota.emit;
    try {
      await api.definirEmitente(produtor.id, resto);
      setEmitenteSalvo(true);
    } catch (e) {
      window.alert(e.message);
    }
  }

  async function salvarProduto(p) {
    const existente = produtos?.find((x) => x.cProd.toUpperCase() === p.cProd.trim().toUpperCase());
    if (existente && !window.confirm(`Já existe o produto ${existente.cProd} (${existente.xProd}). Substituir pelos dados deste item?`)) {
      return 'Cadastro mantido como estava.';
    }
    // O item da nota não tem os campos de estoque: mantém os que já estavam no cadastro.
    const r = existente
      ? await api.alterarProduto(existente.id, { ...p, controlaEstoque: existente.controlaEstoque, estoqueMinimo: existente.estoqueMinimo })
      : await api.criarProduto(p);
    await carregarProdutos();
    return existente ? `Produto ${r.cProd} atualizado no cadastro.` : `Produto ${r.cProd} cadastrado.`;
  }

  async function salvarCliente(d) {
    const doc = digitos(d.tipo === 'PJ' ? d.CNPJ : d.CPF);
    const existente = clientes?.find((c) => digitos(c.CPF || c.CNPJ) === doc);
    if (existente && !window.confirm(`Já existe o cliente ${existente.xNome} com este documento. Substituir pelos dados desta nota?`)) {
      return 'Cadastro mantido como estava.';
    }
    const r = existente ? await api.alterarCliente(existente.id, d) : await api.criarCliente(d);
    await carregarClientes();
    return existente ? `Cliente ${r.xNome} atualizado no cadastro.` : `Cliente ${r.xNome} cadastrado.`;
  }

  function novaNota() {
    if (!window.confirm('Começar uma nota nova? Os dados do destinatário e dos itens serão apagados.')) return;
    const base = notaVazia();
    setNota({ ...base, emit: nota.emit, ide: { ...base.ide, serie: nota.ide.serie } });
    setResultado(null);
    sugerirNumero(nota.ide.serie, true);
  }

  return (
    <>
      <div className="barra-acoes">
        <div>
          <h2>Nota fiscal nº {nota.ide.nNF || '—'} <small>série {nota.ide.serie}</small></h2>
          <span>Rascunho salvo automaticamente neste navegador</span>
        </div>
        <div className="barra-botoes">
          <button type="button" className="btn-secundario" onClick={novaNota}>Nova nota</button>
        </div>
      </div>

      <main className="conteudo">
        <div className="formulario">
          <Identificacao nota={nota} set={set} onSerie={(s) => sugerirNumero(s, true)} />
          <Emitente nota={nota} set={set} certificado={emissor?.certificado} onSalvarPadrao={salvarEmitentePadrao} salvo={emitenteSalvo} />
          <Destinatario nota={nota} set={set} homologacao={homologacao} clientes={clientes} onSalvarCliente={salvarCliente} />
          <Itens nota={nota} set={set} produtos={produtos} onSalvarProduto={salvarProduto} />
          <Pagamento nota={nota} set={set} vNF={totais.vNF} />
        </div>
        <Resumo
          totais={totais} pendencias={listaPendencias} avisos={listaAvisos} ocupado={ocupado} resultado={resultado}
          onValidar={() => executar('validar')} onEmitir={() => executar('emitir')} onPrevia={() => executar('previa')}
          onVerNotas={onVerNotas} onNovaNota={novaNota}
        />
      </main>
    </>
  );
}

// Área logada. `produtor` = produtor ativo (ou null: só dá para escolher/cadastrar produtor).
function Principal({ estado, onEstado, onSair }) {
  const { usuario, produtor } = estado;
  const abas = [
    ...(produtor ? ABAS_EMISSAO : []),
    ['produtores', 'Produtores'],
    ...(usuario.admin ? [['admin', 'Administração']] : []),
  ];
  const [aba, setAba] = React.useState(() => abas.find(([a]) => `#${a}` === window.location.hash)?.[0] ?? (produtor ? 'emitir' : 'produtores'));
  const [emissor, setEmissor] = React.useState(null);
  const [erroEmissor, setErroEmissor] = React.useState('');
  const [produtos, setProdutos] = React.useState(null);
  const [clientes, setClientes] = React.useState(null);
  const [filtroNotas, setFiltroNotas] = React.useState(null);
  const [trocandoSenha, setTrocandoSenha] = React.useState(false);
  const abaValida = abas.some(([a]) => a === aba) ? aba : abas[0][0];

  const trocarAba = (a) => {
    setAba(a);
    history.replaceState(null, '', a === 'emitir' ? '#' : `#${a}`);
  };

  React.useEffect(() => {
    if (!produtor) return;
    api.emissor().then(setEmissor).catch((e) => setErroEmissor(e.message));
  }, [produtor]);

  const carregarProdutos = React.useCallback(() => api.produtos().then(setProdutos).catch(() => setProdutos(null)), []);
  React.useEffect(() => { if (produtor) carregarProdutos(); }, [produtor, carregarProdutos]);

  const carregarClientes = React.useCallback(() => api.clientes().then(setClientes).catch(() => setClientes(null)), []);
  React.useEffect(() => { if (produtor) carregarClientes(); }, [produtor, carregarClientes]);

  // Abre Notas emitidas já filtrada (atalhos do painel).
  const verNotas = (filtro = null) => { setFiltroNotas(filtro); trocarAba('notas'); };

  // Duplicar: a cópia vira o rascunho do produtor, e a tela de emissão abre com ela.
  function duplicarNota(dados) {
    if (!window.confirm('Abrir uma nota nova com os dados desta? O rascunho atual será substituído.')) return;
    const chave = chaveRascunho(produtor.id);
    gravar(chave, notaDoPayload(dados, ler(chave) ?? notaInicial(produtor)));
    trocarAba('emitir');
  }

  const aoSelecionarProdutor = (p) => {
    history.replaceState(null, '', '#');
    onEstado({ ...estado, produtor: p, aviso: null });
  };

  return (
    <div className="app">
      <header className="topo">
        <div className="marca">
          <div className="logo">NF</div>
          <div>
            <h1>Emissor NF-e</h1>
            <span>Modelo 55 · SEFAZ-{estado.uf ?? 'GO'}</span>
          </div>
          <span className="selo-ambiente" title="As notas não têm valor fiscal">HOMOLOGAÇÃO</span>
        </div>
        <Abas abas={abas} ativa={abaValida} onTrocar={(a) => { setFiltroNotas(null); trocarAba(a); }} />
        <div className="topo-direita">
          {produtor ? (
            <button type="button" className="produtor-atual" onClick={() => trocarAba('produtores')} title="Trocar de produtor">
              <span>🔐 {produtor.titular}</span>
              <small>{produtor.tipo} {formatarDoc(produtor.documento)} · trocar</small>
            </button>
          ) : (
            <span className="produtor-atual sem-produtor">Nenhum produtor selecionado</span>
          )}
          {produtor && <StatusSefaz />}
          <BotaoTema />
          <MenuUsuario usuario={usuario} onSenha={() => setTrocandoSenha(true)} onSair={onSair} />
        </div>
      </header>

      {estado.simulada && (
        <div className="faixa-simulada">SEFAZ SIMULADA — modo de teste da interface. Nada é enviado à SEFAZ e as “autorizações” são fictícias.</div>
      )}
      {estado.aviso && <div className="faixa-erro">{estado.aviso}</div>}
      {erroEmissor && <div className="faixa-erro">Não foi possível carregar o produtor: {erroEmissor}</div>}

      {/* A chave recria o bloco a cada aba, e isso dispara a animação de entrada. */}
      <div className="transicao-pagina" key={abaValida}>
      {abaValida === 'emitir' && produtor && (
        <Emissao
          produtor={produtor} emissor={emissor} onVerNotas={() => verNotas()} produtos={produtos} carregarProdutos={carregarProdutos}
          clientes={clientes} carregarClientes={carregarClientes}
        />
      )}
      {abaValida === 'painel' && produtor && <Painel onVerNotas={verNotas} onVerProdutores={() => trocarAba('produtores')} onVerProdutos={() => trocarAba('produtos')} />}
      {abaValida === 'notas' && <Notas emissor={emissor} filtroInicial={filtroNotas} onDuplicar={duplicarNota} />}
      {abaValida === 'clientes' && <Clientes onAlterados={carregarClientes} />}
      {abaValida === 'produtos' && <Produtos onAlterados={carregarProdutos} />}
      {abaValida === 'produtores' && (
        <Produtores
          usuario={usuario} produtorAtivo={produtor?.id} primeiraVez={!produtor}
          onSelecionado={aoSelecionarProdutor}
          onAlterados={(removido) => { if (removido && removido === produtor?.id) onEstado({ ...estado, produtor: null }); }}
        />
      )}
      {abaValida === 'admin' && <Admin usuario={usuario} />}
      </div>
      {trocandoSenha && <JanelaSenha onFechar={() => setTrocandoSenha(false)} />}
    </div>
  );
}

export default function App() {
  const [estado, setEstado] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [avisoLogin, setAvisoLogin] = React.useState('');

  const carregar = React.useCallback(() => {
    api.estado().then((e) => { setEstado(e); setErro(''); }).catch((e) => setErro(e.message));
  }, []);
  React.useEffect(() => { carregar(); }, [carregar]);

  // Sessão expirada (401) → login; produtor indisponível (409) → recarrega o estado.
  React.useEffect(() => {
    const ouvir = (e) => {
      if (e.detail.status === 401) {
        setEstado((s) => (s ? { ...s, usuario: null, produtor: null } : s));
        setAvisoLogin('Sua sessão terminou. Entre novamente.');
      } else {
        carregar();
      }
    };
    window.addEventListener(EVENTO_SESSAO, ouvir);
    return () => window.removeEventListener(EVENTO_SESSAO, ouvir);
  }, [carregar]);

  async function sair() {
    await api.sair().catch(() => {});
    setAvisoLogin('');
    setEstado((s) => ({ ...s, usuario: null, produtor: null }));
    history.replaceState(null, '', '#');
  }

  const entrou = (e) => { setAvisoLogin(''); setEstado(e); };

  if (erro) {
    return <div className="faixa-erro">Não foi possível falar com a API local: {erro}. Ela está rodando (npm run dev)?</div>;
  }
  if (!estado) return <div className="carregando-tela">Carregando…</div>;
  if (!estado.configurado) return <TelaConfiguracao onConfigurado={entrou} />;
  if (!estado.usuario) return <TelaLogin onEntrou={entrou} aviso={avisoLogin} />;
  // A chave remonta tudo ao trocar de usuário ou produtor (nada de dados misturados na tela).
  return <Principal key={`${estado.usuario.id}-${estado.produtor?.id ?? 'nenhum'}`} estado={estado} onEstado={setEstado} onSair={sair} />;
}
