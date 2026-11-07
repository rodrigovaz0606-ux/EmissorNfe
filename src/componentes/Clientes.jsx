import React from 'react';
import { api } from '../api.js';
import { EsqueletoTabela } from './Movimento.jsx';
import { Janela } from './Janela.jsx';
import { CamposDestinatario, mascara } from './Destinatario.jsx';
import { clienteVazio, destDoCliente, cpfValido, cnpjValido, digitos } from '../modelo.js';

function FormularioCliente({ inicial, onSalvo, onFechar }) {
  const [c, setC] = React.useState(() => (inicial?.id ? destDoCliente(inicial) : clienteVazio()));
  const [salvando, setSalvando] = React.useState(false);
  const [erro, setErro] = React.useState('');
  const novo = !inicial?.id;
  const e = c.ender;

  const problemas = [
    !(c.tipo === 'PF' ? cpfValido(c.CPF) : cnpjValido(c.CNPJ)) && (c.tipo === 'PF' ? 'CPF' : 'CNPJ'),
    !c.xNome.trim() && 'nome',
    c.indIEDest === '1' && !digitos(c.IE) && 'inscrição estadual',
    !e.xLgr.trim() && 'logradouro',
    !e.xBairro.trim() && 'bairro',
    !e.xMun.trim() && 'município',
    digitos(e.cMun).length !== 7 && 'código IBGE',
  ].filter(Boolean);

  async function salvar(ev) {
    ev.preventDefault();
    setSalvando(true);
    setErro('');
    try {
      const r = novo ? await api.criarCliente(c) : await api.alterarCliente(inicial.id, c);
      onSalvo(r, novo);
    } catch (e2) {
      setErro(e2.message);
      setSalvando(false);
    }
  }

  return (
    <Janela titulo={novo ? 'Novo cliente' : `Editar ${inicial.xNome}`} onFechar={salvando ? () => {} : onFechar} largura={820}>
      <form onSubmit={salvar} className="form-produto">
        <CamposDestinatario d={c} s={(campo, v) => setC((x) => ({ ...x, [campo]: v }))} />
        {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
        <div className="janela-botoes">
          {problemas.length > 0 && <span className="falta">Falta: {problemas.join(', ')}</span>}
          <button type="button" className="btn-secundario" onClick={onFechar} disabled={salvando}>Cancelar</button>
          <button type="submit" className="btn-primario" disabled={salvando || problemas.length > 0}>
            {salvando ? 'Salvando…' : novo ? 'Cadastrar cliente' : 'Salvar alterações'}
          </button>
        </div>
      </form>
    </Janela>
  );
}

export function Clientes({ onAlterados }) {
  const [lista, setLista] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [busca, setBusca] = React.useState('');
  const [editando, setEditando] = React.useState(null); // {} = novo
  const [aviso, setAviso] = React.useState(null);

  const carregar = React.useCallback(() => api.clientes().then(setLista).catch((e) => setErro(e.message)), []);
  React.useEffect(() => { carregar(); }, [carregar]);

  async function aposMudanca(texto) {
    setAviso({ ok: true, texto });
    await carregar();
    onAlterados?.();
  }

  async function excluir(c) {
    if (!window.confirm(`Excluir o cliente ${c.xNome} do cadastro? Notas já emitidas não mudam.`)) return;
    try {
      await api.excluirCliente(c.id);
      await aposMudanca(`Cliente ${c.xNome} excluído.`);
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    }
  }

  const termo = busca.trim().toUpperCase();
  const termoDoc = digitos(termo);
  const filtrados = (lista ?? []).filter((c) => !termo || c.xNome.includes(termo) || (termoDoc && (c.CPF || c.CNPJ).includes(termoDoc)));

  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>Clientes</h2>
          <span>{lista ? `${lista.length} ${lista.length === 1 ? 'cliente cadastrado' : 'clientes cadastrados'}` : 'Carregando…'}</span>
        </div>
        <div className="barra-botoes">
          {lista?.length > 0 && (
            <input type="search" className="busca" placeholder="Buscar por nome, CPF ou CNPJ" value={busca}
              onChange={(e) => setBusca(e.target.value)} aria-label="Buscar cliente" />
          )}
          <button type="button" className="btn-primario" onClick={() => { setAviso(null); setEditando({}); }}>+ Novo cliente</button>
        </div>
      </div>

      <div className="conteudo-lista">
        {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}
        {erro && <div className="aviso-local falha">{erro}</div>}
        {!lista && !erro && <EsqueletoTabela />}
        {lista && lista.length === 0 && (
          <div className="vazio">
            <div className="vazio-icone">👥</div>
            <h3>Nenhum cliente cadastrado</h3>
            <p>Cadastre quem compra de você para preencher o destinatário da nota com um clique.</p>
          </div>
        )}
        {lista && lista.length > 0 && (
          <table className="tabela-notas">
            <thead>
              <tr><th>Nome</th><th>CPF/CNPJ</th><th>Município</th><th>IE</th><th>E-mail</th><th /></tr>
            </thead>
            <tbody>
              {filtrados.map((c) => (
                <tr key={c.id} onClick={() => { setAviso(null); setEditando(c); }} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && setEditando(c)}>
                  <td className="cortar"><strong>{c.xNome}</strong></td>
                  <td>{mascara(c.CPF || c.CNPJ, c.tipo)}</td>
                  <td>{c.ender?.xMun} / {c.ender?.UF}</td>
                  <td>{c.IE || '—'}</td>
                  <td className="cortar">{c.email || '—'}</td>
                  <td className="direita">
                    <button type="button" className="btn-perigo-texto" onClick={(e) => { e.stopPropagation(); excluir(c); }} aria-label={`Excluir ${c.xNome}`}>Excluir</button>
                  </td>
                </tr>
              ))}
              {filtrados.length === 0 && (
                <tr><td colSpan={6} className="vazio-pequeno">Nenhum cliente encontrado para “{busca}”.</td></tr>
              )}
            </tbody>
          </table>
        )}
      </div>

      {editando && (
        <FormularioCliente
          inicial={editando}
          onFechar={() => setEditando(null)}
          onSalvo={(r, novo) => { setEditando(null); aposMudanca(novo ? `Cliente ${r.xNome} cadastrado.` : `Cliente ${r.xNome} alterado.`); }}
        />
      )}
    </div>
  );
}
