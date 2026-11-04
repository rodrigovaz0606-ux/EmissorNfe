import React from 'react';
import { api } from '../api.js';
import { Janela } from './Janela.jsx';

const dataHora = (s) => (s ? new Date(s.replace(' ', 'T')).toLocaleString('pt-BR', { dateStyle: 'short', timeStyle: 'short' }) : 'nunca');

function CampoTexto({ rotulo, dica, ...props }) {
  const id = React.useId();
  return (
    <div className="campo">
      <label className="rotulo" htmlFor={id}>{rotulo}</label>
      <input id={id} {...props} />
      {dica && <span className="dica">{dica}</span>}
    </div>
  );
}

function Marcador({ rotulo, marcado, onChange, desabilitado, dica }) {
  return (
    <label className={`marcador ${desabilitado ? 'desabilitado' : ''}`} title={dica}>
      <input type="checkbox" checked={marcado} onChange={(e) => onChange(e.target.checked)} disabled={desabilitado} />
      {rotulo}
    </label>
  );
}

// Novo usuário (inicial vazio) ou edição de um existente.
function JanelaUsuario({ inicial, eu, onSalvo, onFechar }) {
  const novo = !inicial;
  const [d, setD] = React.useState(() => ({ nome: inicial?.nome ?? '', login: inicial?.login ?? '', senha: '', admin: inicial?.admin ?? false, ativo: inicial?.ativo ?? true }));
  const [erro, setErro] = React.useState('');
  const [salvando, setSalvando] = React.useState(false);
  const s = (c) => (v) => setD((x) => ({ ...x, [c]: v }));
  const souEu = inicial?.id === eu;

  async function salvar(e) {
    e.preventDefault();
    setSalvando(true);
    setErro('');
    try {
      const r = novo
        ? await api.criarUsuario({ nome: d.nome, login: d.login, senha: d.senha, admin: d.admin })
        : await api.alterarUsuario(inicial.id, { nome: d.nome, admin: d.admin, ativo: d.ativo, ...(d.senha ? { senha: d.senha } : {}) });
      onSalvo(r, novo);
    } catch (e2) {
      setErro(e2.message);
      setSalvando(false);
    }
  }

  return (
    <Janela titulo={novo ? 'Novo usuário' : `Editar ${inicial.login}`} onFechar={salvando ? () => {} : onFechar}>
      <form onSubmit={salvar} className="form-acesso">
        <CampoTexto rotulo="Nome" value={d.nome} onChange={(e) => s('nome')(e.target.value)} required autoFocus />
        {novo && <CampoTexto rotulo="Usuário (login)" value={d.login} onChange={(e) => s('login')(e.target.value)} required autoComplete="off" dica="Letras, números, ponto, hífen ou sublinhado" />}
        <CampoTexto
          rotulo={novo ? 'Senha inicial' : 'Nova senha'} type="password" value={d.senha} onChange={(e) => s('senha')(e.target.value)}
          required={novo} autoComplete="new-password"
          dica={novo ? 'Pelo menos 8 caracteres. Passe ao usuário; ele pode trocar depois.' : 'Deixe em branco para manter. Se trocar, o usuário sai de todas as sessões.'}
        />
        <div className="marcadores">
          <Marcador rotulo="Administrador" marcado={d.admin} onChange={s('admin')} desabilitado={souEu} dica={souEu ? 'Você não pode tirar o seu próprio acesso' : ''} />
          {!novo && <Marcador rotulo="Ativo (pode entrar)" marcado={d.ativo} onChange={s('ativo')} desabilitado={souEu} />}
        </div>
        {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
        <div className="janela-botoes">
          <button type="button" className="btn-secundario" onClick={onFechar} disabled={salvando}>Cancelar</button>
          <button type="submit" className="btn-primario" disabled={salvando}>{salvando ? 'Salvando…' : novo ? 'Criar usuário' : 'Salvar'}</button>
        </div>
      </form>
    </Janela>
  );
}

export function Admin({ usuario }) {
  const [lista, setLista] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [aviso, setAviso] = React.useState(null);
  const [editando, setEditando] = React.useState(null); // 'novo' | usuário

  const carregar = React.useCallback(() => api.usuarios().then(setLista).catch((e) => setErro(e.message)), []);
  React.useEffect(() => { carregar(); }, [carregar]);

  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>Administração</h2>
          <span>Usuários que podem entrar no emissor. Cada um cadastra e usa os próprios produtores.</span>
        </div>
        <div className="barra-botoes">
          <button type="button" className="btn-primario" onClick={() => { setAviso(null); setEditando('novo'); }}>+ Novo usuário</button>
        </div>
      </div>
      <div className="conteudo-lista">
        {aviso && <div className="aviso-local ok">{aviso}</div>}
        {erro && <div className="aviso-local falha">{erro}</div>}
        {lista && (
          <table className="tabela-notas">
            <thead>
              <tr><th>Nome</th><th>Usuário</th><th>Perfil</th><th>Produtores</th><th>Último acesso</th><th>Situação</th><th /></tr>
            </thead>
            <tbody>
              {lista.map((u) => (
                <tr key={u.id} onClick={() => { setAviso(null); setEditando(u); }} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && setEditando(u)}>
                  <td><strong>{u.nome}</strong>{u.id === usuario.id && <span className="selo selo-neutro voce">você</span>}</td>
                  <td>{u.login}</td>
                  <td>{u.admin ? 'Administrador' : 'Usuário'}</td>
                  <td>{u.produtores}</td>
                  <td>{dataHora(u.ultimo_acesso)}</td>
                  <td>{u.ativo ? <span className="selo selo-ok">Ativo</span> : <span className="selo selo-falha">Desativado</span>}</td>
                  <td className="direita"><span className="btn-texto">Editar ›</span></td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {editando && (
        <JanelaUsuario
          inicial={editando === 'novo' ? null : editando} eu={usuario.id}
          onFechar={() => setEditando(null)}
          onSalvo={(r, novo) => { setEditando(null); setAviso(novo ? `Usuário ${r.login} criado.` : `Usuário ${r.login} atualizado.`); carregar(); }}
        />
      )}
    </div>
  );
}

export function JanelaSenha({ onFechar }) {
  const [d, setD] = React.useState({ atual: '', nova: '', confirmacao: '' });
  const [erro, setErro] = React.useState('');
  const [ok, setOk] = React.useState(false);
  const [enviando, setEnviando] = React.useState(false);
  const s = (c) => (e) => setD({ ...d, [c]: e.target.value });
  const conferem = d.nova === d.confirmacao;

  async function trocar(e) {
    e.preventDefault();
    setEnviando(true);
    setErro('');
    try {
      await api.trocarSenha(d.atual, d.nova);
      setOk(true);
    } catch (e2) {
      setErro(e2.message);
    } finally {
      setEnviando(false);
    }
  }

  return (
    <Janela titulo="Alterar minha senha" onFechar={enviando ? () => {} : onFechar} largura={440}>
      {ok ? (
        <>
          <div className="aviso-local ok">Senha alterada. As outras sessões abertas com o seu usuário foram encerradas.</div>
          <div className="janela-botoes"><button type="button" className="btn-primario" onClick={onFechar}>Fechar</button></div>
        </>
      ) : (
        <form onSubmit={trocar} className="form-acesso">
          <CampoTexto rotulo="Senha atual" type="password" value={d.atual} onChange={s('atual')} autoComplete="current-password" required autoFocus />
          <CampoTexto rotulo="Nova senha" type="password" value={d.nova} onChange={s('nova')} autoComplete="new-password" required dica="Pelo menos 8 caracteres" />
          <CampoTexto rotulo="Confirme a nova senha" type="password" value={d.confirmacao} onChange={s('confirmacao')} autoComplete="new-password" required />
          {!conferem && d.confirmacao && <div className="faixa-erro-local">As senhas não conferem.</div>}
          {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
          <div className="janela-botoes">
            <button type="button" className="btn-secundario" onClick={onFechar} disabled={enviando}>Cancelar</button>
            <button type="submit" className="btn-primario" disabled={enviando || !conferem || d.nova.length < 8}>{enviando ? 'Alterando…' : 'Alterar senha'}</button>
          </div>
        </form>
      )}
    </Janela>
  );
}
