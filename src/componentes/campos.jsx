import React from 'react';

export function Secao({ id, numero, titulo, descricao, acoes, children }) {
  return (
    <section className="secao" id={id}>
      <header className="secao-cabecalho">
        <span className="secao-numero">{numero}</span>
        <div>
          <h2>{titulo}</h2>
          {descricao && <p>{descricao}</p>}
        </div>
        {acoes && <div className="secao-acoes">{acoes}</div>}
      </header>
      <div className="secao-corpo">{children}</div>
    </section>
  );
}

// Campo de texto. `erro` aparece só depois que o usuário mexeu no campo.
export function Campo({ rotulo, valor, onChange, erro, dica, largura = 'm', obrigatorio, lista, ...resto }) {
  const [tocado, setTocado] = React.useState(false);
  const mostrarErro = erro && tocado;
  const id = React.useId();
  return (
    <label className={`campo l-${largura} ${mostrarErro ? 'com-erro' : ''}`} htmlFor={id}>
      <span className="rotulo">
        {rotulo}
        {obrigatorio && <i className="obrigatorio" title="Obrigatório">*</i>}
      </span>
      <input
        id={id}
        value={valor ?? ''}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => setTocado(true)}
        list={lista}
        {...resto}
      />
      {mostrarErro ? <span className="msg-erro">{erro}</span> : dica && <span className="dica">{dica}</span>}
    </label>
  );
}

export function Selecao({ rotulo, valor, onChange, opcoes, largura = 'm', dica }) {
  const id = React.useId();
  return (
    <label className={`campo l-${largura}`} htmlFor={id}>
      <span className="rotulo">{rotulo}</span>
      <select id={id} value={valor} onChange={(e) => onChange(e.target.value)}>
        {opcoes.map((o) => {
          const [v, t] = Array.isArray(o) ? o : [o, o];
          return <option key={v} value={v}>{t}</option>;
        })}
      </select>
      {dica && <span className="dica">{dica}</span>}
    </label>
  );
}

export function Alternador({ valor, onChange, opcoes }) {
  return (
    <div className="alternador" role="radiogroup">
      {opcoes.map(([v, t]) => (
        <button type="button" key={v} role="radio" aria-checked={valor === v} className={valor === v ? 'ativo' : ''} onClick={() => onChange(v)}>
          {t}
        </button>
      ))}
    </div>
  );
}
