// Peças visuais com movimento: número em contagem, abas com marcador deslizante,
// botão de tema e esqueletos de carregamento.
import React from 'react';
import { trocarTema, useTema } from '../tema.js';

const movimentoReduzido = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

// Número que conta até o valor. O último quadro é sempre o valor exato.
export function NumeroAnimado({ valor, formatar = String, duracao = 700 }) {
  const [mostrado, setMostrado] = React.useState(() => (movimentoReduzido() ? valor : 0));
  const anterior = React.useRef(mostrado);

  React.useEffect(() => {
    const de = anterior.current;
    if (de === valor || movimentoReduzido()) {
      anterior.current = valor;
      setMostrado(valor);
      return undefined;
    }
    let quadro;
    const inicio = performance.now();
    const passo = (agora) => {
      const t = Math.min((agora - inicio) / duracao, 1);
      const suavizado = 1 - (1 - t) ** 3;
      const atual = t === 1 ? valor : Math.round(de + (valor - de) * suavizado);
      anterior.current = atual;
      setMostrado(atual);
      if (t < 1) quadro = requestAnimationFrame(passo);
    };
    quadro = requestAnimationFrame(passo);
    return () => cancelAnimationFrame(quadro);
  }, [valor, duracao]);

  return <>{formatar(mostrado)}</>;
}

// Abas com um marcador que desliza até a aba ativa.
export function Abas({ abas, ativa, onTrocar }) {
  const nav = React.useRef(null);
  const [marcador, setMarcador] = React.useState(null);

  const medir = React.useCallback(() => {
    const b = nav.current?.querySelector('button.ativa');
    if (b) setMarcador({ x: b.offsetLeft, largura: b.offsetWidth });
  }, []);

  React.useLayoutEffect(medir, [ativa, abas.length, medir]);
  React.useEffect(() => {
    const observador = new ResizeObserver(medir);
    observador.observe(nav.current);
    document.fonts?.ready.then(medir); // a largura muda quando a fonte termina de carregar
    return () => observador.disconnect();
  }, [medir]);

  return (
    <nav className="abas" role="tablist" ref={nav}>
      {marcador && <span className="abas-marcador" aria-hidden="true" style={{ width: marcador.largura, transform: `translateX(${marcador.x}px)` }} />}
      {abas.map(([a, rotulo]) => (
        <button key={a} type="button" role="tab" aria-selected={ativa === a} className={ativa === a ? 'ativa' : ''} onClick={() => onTrocar(a)}>{rotulo}</button>
      ))}
    </nav>
  );
}

export function BotaoTema({ claro }) {
  const tema = useTema();
  const rotulo = tema === 'escuro' ? 'Ativar tema claro' : 'Ativar tema escuro';
  return (
    <button type="button" className={`botao-tema ${claro ? 'sobre-claro' : ''}`} onClick={trocarTema} aria-label={rotulo} title={rotulo}>
      <svg className="icone-sol" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
      </svg>
      <svg className="icone-lua" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
      </svg>
    </button>
  );
}

// Tabela "fantasma" enquanto a lista carrega. Só aparece se a resposta demorar (ver movimento.css).
export function EsqueletoTabela({ linhas = 5 }) {
  return (
    <div className="esqueleto-tabela" role="status" aria-label="Carregando">
      {Array.from({ length: linhas }, (_, i) => (
        <div className="esqueleto-linha" key={i}>
          <span className="esqueleto" style={{ width: '12%' }} />
          <span className="esqueleto" style={{ width: `${34 - i * 3}%` }} />
          <span className="esqueleto" style={{ width: '16%' }} />
          <span className="esqueleto" style={{ width: '10%' }} />
        </div>
      ))}
    </div>
  );
}

export function EsqueletoPainel() {
  return (
    <div role="status" aria-label="Carregando">
      <div className="painel-indicadores">
        {Array.from({ length: 5 }, (_, i) => (
          <div className="indicador esqueleto-cartao" key={i}>
            <span className="esqueleto" style={{ width: '60%' }} />
            <span className="esqueleto grande" style={{ width: '45%' }} />
          </div>
        ))}
      </div>
      <div className="painel-cartao esqueleto-cartao"><span className="esqueleto" style={{ width: '30%' }} /><span className="esqueleto bloco" /></div>
    </div>
  );
}
