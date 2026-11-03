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
// O <label> contém só o nome do campo; dica e erro ficam ligados por aria-describedby.
export function Campo({ rotulo, valor, onChange, onBlur, erro, dica, largura = 'm', obrigatorio, lista, ...resto }) {
  const [tocado, setTocado] = React.useState(false);
  const mostrarErro = erro && tocado;
  const id = React.useId();
  const idAjuda = `${id}-ajuda`;
  const ajuda = mostrarErro ? erro : dica;
  return (
    <div className={`campo l-${largura} ${mostrarErro ? 'com-erro' : ''}`}>
      <label className="rotulo" htmlFor={id}>
        {rotulo}
        {obrigatorio && <i className="obrigatorio" aria-hidden="true">*</i>}
      </label>
      <input
        id={id}
        value={valor ?? ''}
        onChange={(e) => onChange(e.target.value)}
        onBlur={(e) => { setTocado(true); onBlur?.(e); }}
        list={lista}
        required={obrigatorio}
        aria-invalid={mostrarErro ? true : undefined}
        aria-describedby={ajuda ? idAjuda : undefined}
        {...resto}
      />
      {ajuda && <span id={idAjuda} className={mostrarErro ? 'msg-erro' : 'dica'}>{ajuda}</span>}
    </div>
  );
}

export function Selecao({ rotulo, valor, onChange, opcoes, largura = 'm', dica }) {
  const id = React.useId();
  return (
    <div className={`campo l-${largura}`}>
      <label className="rotulo" htmlFor={id}>{rotulo}</label>
      <select id={id} value={valor} onChange={(e) => onChange(e.target.value)} aria-describedby={dica ? `${id}-ajuda` : undefined}>
        {opcoes.map((o) => {
          const [v, t] = Array.isArray(o) ? o : [o, o];
          return <option key={v} value={v}>{t}</option>;
        })}
      </select>
      {dica && <span id={`${id}-ajuda`} className="dica">{dica}</span>}
    </div>
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
