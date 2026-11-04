import React from 'react';
import { api } from '../api.js';

function CampoAcesso({ rotulo, dica, ...props }) {
  const id = React.useId();
  return (
    <div className="campo">
      <label className="rotulo" htmlFor={id}>{rotulo}</label>
      <input id={id} {...props} aria-describedby={dica ? `${id}-dica` : undefined} />
      {dica && <span id={`${id}-dica`} className="dica">{dica}</span>}
    </div>
  );
}

function Moldura({ titulo, subtitulo, children }) {
  return (
    <div className="tela-acesso">
      <div className="cartao-acesso">
        <div className="marca marca-acesso">
          <div className="logo">NF</div>
          <div>
            <h1>Emissor NF-e</h1>
            <span>Modelo 55 · homologação</span>
          </div>
        </div>
        <h2>{titulo}</h2>
        {subtitulo && <p className="acesso-subtitulo">{subtitulo}</p>}
        {children}
      </div>
    </div>
  );
}

export function TelaLogin({ onEntrou, aviso }) {
  const [login, setLogin] = React.useState('');
  const [senha, setSenha] = React.useState('');
  const [erro, setErro] = React.useState('');
  const [entrando, setEntrando] = React.useState(false);

  async function entrar(e) {
    e.preventDefault();
    setEntrando(true);
    setErro('');
    try {
      onEntrou(await api.entrar(login, senha));
    } catch (e2) {
      setErro(e2.message);
      setSenha('');
      setEntrando(false);
    }
  }

  return (
    <Moldura titulo="Entrar" subtitulo="Use o usuário e a senha cadastrados pelo administrador.">
      {aviso && <div className="aviso-local falha">{aviso}</div>}
      <form onSubmit={entrar} className="form-acesso">
        <CampoAcesso rotulo="Usuário" value={login} onChange={(e) => setLogin(e.target.value)} autoComplete="username" autoFocus required />
        <CampoAcesso rotulo="Senha" type="password" value={senha} onChange={(e) => setSenha(e.target.value)} autoComplete="current-password" required />
        {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
        <button type="submit" className="btn-primario largo" disabled={entrando || !login || !senha}>{entrando ? 'Entrando…' : 'Entrar'}</button>
      </form>
    </Moldura>
  );
}

export function TelaConfiguracao({ onConfigurado }) {
  const [d, setD] = React.useState({ nome: '', login: '', senha: '', confirmacao: '' });
  const [erro, setErro] = React.useState('');
  const [enviando, setEnviando] = React.useState(false);
  const s = (c) => (e) => setD({ ...d, [c]: e.target.value });
  const conferem = d.senha === d.confirmacao;

  async function criar(e) {
    e.preventDefault();
    if (!conferem) return;
    setEnviando(true);
    setErro('');
    try {
      onConfigurado(await api.configurar({ nome: d.nome, login: d.login, senha: d.senha }));
    } catch (e2) {
      setErro(e2.message);
      setEnviando(false);
    }
  }

  return (
    <Moldura titulo="Primeiro acesso" subtitulo="Crie o usuário administrador. Ele cadastra os demais usuários e vê todos os produtores.">
      <form onSubmit={criar} className="form-acesso">
        <CampoAcesso rotulo="Seu nome" value={d.nome} onChange={s('nome')} autoComplete="name" autoFocus required />
        <CampoAcesso rotulo="Usuário" value={d.login} onChange={s('login')} autoComplete="username" required dica="Letras, números, ponto, hífen ou sublinhado" />
        <CampoAcesso rotulo="Senha" type="password" value={d.senha} onChange={s('senha')} autoComplete="new-password" required dica="Pelo menos 8 caracteres" />
        <CampoAcesso rotulo="Confirme a senha" type="password" value={d.confirmacao} onChange={s('confirmacao')} autoComplete="new-password" required />
        {!conferem && d.confirmacao && <div className="faixa-erro-local">As senhas não conferem.</div>}
        {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}
        <button type="submit" className="btn-primario largo" disabled={enviando || !conferem || d.senha.length < 8}>
          {enviando ? 'Criando…' : 'Criar administrador e entrar'}
        </button>
      </form>
    </Moldura>
  );
}
