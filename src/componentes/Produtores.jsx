import React from 'react';
import { api, arquivoEmBase64 } from '../api.js';
import { Janela } from './Janela.jsx';

export const formatarDoc = (d = '') =>
  d.length === 11 ? d.replace(/(\d{3})(\d{3})(\d{3})(\d{2})/, '$1.$2.$3-$4')
    : d.length === 14 ? d.replace(/(\d{2})(\d{3})(\d{3})(\d{4})(\d{2})/, '$1.$2.$3/$4-$5') : d;

const data = (iso) => (iso ? new Date(iso).toLocaleDateString('pt-BR') : '—');
const diasAte = (iso) => (iso ? Math.floor((new Date(iso) - Date.now()) / 86400000) : null);
const tamanhoArquivo = (bytes) => `${(bytes / 1024).toLocaleString('pt-BR', { maximumFractionDigits: 1 })} KB`;

// Envio do certificado A1 (.pfx/.p12) com a senha. Usado para adicionar e para renovar.
function JanelaCertificado({ titulo, explicacao, textoBotao, onEnviar, onFechar }) {
  const [arquivo, setArquivo] = React.useState(null);
  const [senha, setSenha] = React.useState('');
  const [erro, setErro] = React.useState('');
  const [enviando, setEnviando] = React.useState(false);
  const idArq = React.useId();
  const idSenha = React.useId();

  async function enviar(e) {
    e.preventDefault();
    setEnviando(true);
    setErro('');
    try {
      await onEnviar(await arquivoEmBase64(arquivo), senha);
    } catch (e2) {
      setErro(e2.message);
      setEnviando(false);
    }
  }

  return (
    <Janela titulo={titulo} onFechar={enviando ? () => {} : onFechar}>
      <form onSubmit={enviar} className="form-acesso">
        <div className="explicacao">{explicacao}</div>
        <div className="campo">
          <label className="rotulo" htmlFor={idArq}>Arquivo do certificado A1</label>
          <label className={`seletor-arquivo ${arquivo ? 'escolhido' : ''}`}>
            <input id={idArq} type="file" accept=".pfx,.p12,application/x-pkcs12" onChange={(e) => setArquivo(e.target.files[0] ?? null)} disabled={enviando} required />
            <span className="seletor-icone" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                {arquivo
                  ? <path d="M20 6 9 17l-5-5" />
                  : <><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><path d="M14 2v6h6" /></>}
              </svg>
            </span>
            <span className="seletor-texto">
              <strong>{arquivo ? arquivo.name : 'Nenhum certificado selecionado'}</strong>
              <small>{arquivo ? tamanhoArquivo(arquivo.size) : 'Arquivo .pfx ou .p12'}</small>
            </span>
            <span className="seletor-acao">{arquivo ? 'Trocar' : 'Selecionar arquivo'}</span>
          </label>
        </div>
        <div className="campo">
          <label className="rotulo" htmlFor={idSenha}>Senha do certificado</label>
          <input id={idSenha} type="password" value={senha} onChange={(e) => setSenha(e.target.value)} autoComplete="off" />
        </div>
        {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
        <div className="janela-botoes">
          <button type="button" className="btn-secundario" onClick={onFechar} disabled={enviando}>Cancelar</button>
          <button type="submit" className="btn-primario" disabled={!arquivo || enviando}>{enviando ? 'Conferindo certificado…' : textoBotao}</button>
        </div>
      </form>
    </Janela>
  );
}

const EXPLICACAO_GUARDA =
  'O certificado é aberto aqui para conferir a senha, o CPF/CNPJ e a validade. Depois fica guardado cifrado neste computador ' +
  '(AES-256) e só é usado para assinar e transmitir as notas deste produtor. Esta versão só opera em homologação.';

export function Produtores({ usuario, produtorAtivo, onSelecionado, onAlterados, primeiraVez }) {
  const [lista, setLista] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [aviso, setAviso] = React.useState(null);
  const [janela, setJanela] = React.useState(null); // { tipo: 'novo' } | { tipo: 'renovar', produtor }
  const [trocando, setTrocando] = React.useState(null);

  const carregar = React.useCallback(() => api.produtores().then(setLista).catch((e) => setErro(e.message)), []);
  React.useEffect(() => { carregar(); }, [carregar]);

  async function selecionar(p) {
    setTrocando(p.id);
    setAviso(null);
    try {
      onSelecionado(await api.selecionarProdutor(p.id));
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    } finally {
      setTrocando(null);
    }
  }

  async function remover(p) {
    const msg = `Remover ${p.titular}?\n\nO certificado guardado será apagado. As notas já emitidas continuam no banco e voltam a aparecer se o produtor for cadastrado de novo.`;
    if (!window.confirm(msg)) return;
    try {
      await api.removerProdutor(p.id);
      setAviso({ ok: true, texto: `${p.titular} removido.` });
      await carregar();
      onAlterados?.(p.id);
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    }
  }

  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>{primeiraVez ? 'Escolha o produtor' : 'Produtores'}</h2>
          <span>
            {primeiraVez
              ? 'As notas são emitidas em nome do produtor escolhido, com o certificado dele.'
              : usuario?.admin ? 'Como administrador, você vê os produtores de todos os usuários.' : 'Produtores que você cadastrou.'}
          </span>
        </div>
        <div className="barra-botoes">
          <button type="button" className="btn-primario" onClick={() => { setAviso(null); setJanela({ tipo: 'novo' }); }}>+ Adicionar produtor</button>
        </div>
      </div>

      <div className="conteudo-lista">
        {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}
        {erro && <div className="aviso-local falha">{erro}</div>}
        {lista && lista.length === 0 && (
          <div className="vazio">
            <div className="vazio-icone">🔐</div>
            <h3>Nenhum produtor cadastrado</h3>
            <p>Adicione um produtor enviando o certificado digital A1 dele (arquivo .pfx e senha).</p>
          </div>
        )}
        {lista && lista.length > 0 && (
          <div className="cartoes-produtores">
            {lista.map((p) => {
              const ativo = p.id === produtorAtivo;
              const dias = diasAte(p.valido_ate);
              return (
                <div key={p.id} className={`cartao-produtor ${ativo ? 'ativo' : ''}`} data-documento={p.documento}>
                  <div className="cartao-produtor-topo">
                    <div>
                      <h3>{p.titular}</h3>
                      <span className="doc">{p.tipo} · {formatarDoc(p.documento)}</span>
                    </div>
                    {ativo && <span className="selo selo-ok">Em uso</span>}
                  </div>
                  <dl>
                    <div><dt>Certificado válido até</dt><dd className={dias !== null && dias < 30 ? 'vence' : ''}>{data(p.valido_ate)}{dias !== null && dias < 30 ? ` (${dias} dias)` : ''}</dd></div>
                    <div><dt>Notas</dt><dd>{p.notas}</dd></div>
                    {usuario?.admin && <div><dt>Cadastrado por</dt><dd>{p.dono_nome ?? '—'}</dd></div>}
                    {p.emitente?.IE && <div><dt>Inscrição estadual</dt><dd>{p.emitente.IE}</dd></div>}
                  </dl>
                  <div className="cartao-produtor-acoes">
                    {!ativo && (
                      <button type="button" className="btn-primario pequeno" onClick={() => selecionar(p)} disabled={trocando !== null}>
                        {trocando === p.id ? 'Abrindo…' : 'Usar este produtor'}
                      </button>
                    )}
                    <button type="button" className="btn-texto" onClick={() => { setAviso(null); setJanela({ tipo: 'renovar', produtor: p }); }}>Renovar certificado</button>
                    <button type="button" className="btn-perigo-texto" onClick={() => remover(p)}>Remover</button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {janela?.tipo === 'novo' && (
        <JanelaCertificado
          titulo="Adicionar produtor" textoBotao="Adicionar e usar"
          explicacao={<>O produtor é identificado pelo CPF/CNPJ do certificado. {EXPLICACAO_GUARDA}</>}
          onFechar={() => setJanela(null)}
          onEnviar={async (pfx, senha) => {
            const p = await api.adicionarProdutor(pfx, senha);
            setJanela(null);
            await carregar();
            onSelecionado(await api.selecionarProdutor(p.id));
          }}
        />
      )}
      {janela?.tipo === 'renovar' && (
        <JanelaCertificado
          titulo={`Renovar certificado — ${janela.produtor.titular}`} textoBotao="Trocar certificado"
          explicacao={<>Envie o certificado novo do mesmo {janela.produtor.tipo === 'e-CPF' ? 'CPF' : 'CNPJ'} ({formatarDoc(janela.produtor.documento)}). O certificado antigo é descartado. {EXPLICACAO_GUARDA}</>}
          onFechar={() => setJanela(null)}
          onEnviar={async (pfx, senha) => {
            const r = await api.renovarCertificado(janela.produtor.id, pfx, senha);
            setJanela(null);
            setAviso({ ok: true, texto: `Certificado de ${r.titular} renovado (válido até ${data(r.validoAte)}).` });
            await carregar();
            onAlterados?.();
          }}
        />
      )}
    </div>
  );
}
