import React from 'react';

// Janela modal simples (fecha com Esc ou clicando fora).
export function Janela({ titulo, children, onFechar, largura = 520 }) {
  React.useEffect(() => {
    const tecla = (e) => e.key === 'Escape' && onFechar();
    window.addEventListener('keydown', tecla);
    return () => window.removeEventListener('keydown', tecla);
  }, [onFechar]);
  return (
    <div className="janela-fundo" onMouseDown={(e) => e.target === e.currentTarget && onFechar()}>
      <div className="janela" role="dialog" aria-modal="true" aria-label={titulo} style={{ maxWidth: largura }}>
        <header>
          <h2>{titulo}</h2>
          <button type="button" className="btn-icone fechar" onClick={onFechar} aria-label="Fechar">✕</button>
        </header>
        <div className="janela-corpo">{children}</div>
      </div>
    </div>
  );
}

// Formulário de texto com contador (justificativa de cancelamento, correção da CC-e, etc.).
export function JanelaTexto({ titulo, explicacao, rotulo, min, max, textoBotao, perigo, onConfirmar, onFechar }) {
  const [texto, setTexto] = React.useState('');
  const [enviando, setEnviando] = React.useState(false);
  const [erro, setErro] = React.useState('');
  const tamanho = texto.trim().length;
  const ok = tamanho >= min && tamanho <= max;

  async function confirmar() {
    setEnviando(true);
    setErro('');
    try {
      await onConfirmar(texto.trim());
    } catch (e) {
      setErro(e.message);
      setEnviando(false);
    }
  }

  return (
    <Janela titulo={titulo} onFechar={enviando ? () => {} : onFechar}>
      {explicacao && <div className="explicacao">{explicacao}</div>}
      <div className="campo">
        <label className="rotulo" htmlFor="janela-texto">{rotulo}</label>
        <textarea
          id="janela-texto" className="texto-livre" rows={4} value={texto} maxLength={max} autoFocus
          onChange={(e) => setTexto(e.target.value)} aria-describedby="janela-contador"
        />
        <span id="janela-contador" className={`dica contador ${tamanho > 0 && !ok ? 'ruim' : ''}`}>{tamanho} / mínimo {min}, máximo {max} caracteres</span>
      </div>
      {erro && <div className="faixa-erro-local">{erro}</div>}
      <div className="janela-botoes">
        <button type="button" className="btn-secundario" onClick={onFechar} disabled={enviando}>Voltar</button>
        <button type="button" className={perigo ? 'btn-perigo' : 'btn-primario'} onClick={confirmar} disabled={!ok || enviando}>
          {enviando ? 'Enviando à SEFAZ…' : textoBotao}
        </button>
      </div>
    </Janela>
  );
}
