// Janela de confirmação do próprio sistema (substitui as caixas nativas do navegador:
// window.confirm, window.prompt e window.alert não são usados em lugar nenhum).
//
//   if (await confirmar({ titulo: 'Excluir?', mensagem: '...' })) { ... }
//   await avisar({ titulo: 'Não foi possível salvar', mensagem: e.message });
import React from 'react';

let abrir = null; // registrado pelo <Confirmacoes /> montado na raiz do App

export function confirmar(opcoes) {
  return new Promise((resolver) => {
    if (!abrir) {
      console.error('Confirmacoes não está montado; resposta tratada como "Não".');
      resolver(false);
      return;
    }
    abrir({ ...opcoes, resolver });
  });
}

export const avisar = (opcoes) => confirmar({ ...opcoes, somenteOk: true });

function Icone({ tipo }) {
  if (tipo === 'perigo') {
    return (
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z" /><path d="M12 9v4M12 17h.01" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="10" /><path d="M12 16v-4M12 8h.01" />
    </svg>
  );
}

export function Confirmacoes() {
  const [pedido, setPedido] = React.useState(null);
  const id = React.useId();

  React.useEffect(() => {
    abrir = (novo) => setPedido((anterior) => {
      anterior?.resolver(false); // só uma pergunta por vez
      return novo;
    });
    return () => { abrir = null; };
  }, []);

  const responder = React.useCallback((resposta) => {
    setPedido((p) => {
      p?.resolver(resposta);
      return null;
    });
  }, []);

  React.useEffect(() => {
    if (!pedido) return undefined;
    const tecla = (e) => { if (e.key === 'Escape') responder(false); };
    window.addEventListener('keydown', tecla);
    return () => window.removeEventListener('keydown', tecla);
  }, [pedido, responder]);

  if (!pedido) return null;
  const tipo = pedido.perigo ? 'perigo' : 'info';
  return (
    <div className="janela-fundo confirmacao-fundo" onMouseDown={(e) => e.target === e.currentTarget && !pedido.somenteOk && responder(false)}>
      <div className={`confirmacao ${tipo}`} role="alertdialog" aria-modal="true" aria-labelledby={`${id}-t`} aria-describedby={`${id}-m`}>
        <div className="confirmacao-cabeca">
          <span className="confirmacao-icone"><Icone tipo={tipo} /></span>
          <h2 id={`${id}-t`}>{pedido.titulo}</h2>
        </div>
        <div className="confirmacao-texto" id={`${id}-m`}>
          {pedido.mensagem && <p>{pedido.mensagem}</p>}
          {pedido.detalhes && (
            <dl className="confirmacao-detalhes">
              {pedido.detalhes.map(([rotulo, valor]) => (
                <div key={rotulo}><dt>{rotulo}</dt><dd>{valor}</dd></div>
              ))}
            </dl>
          )}
          {pedido.destaque && <p className="confirmacao-destaque">{pedido.destaque}</p>}
        </div>
        <div className="janela-botoes">
          {!pedido.somenteOk && (
            <button type="button" className="btn-secundario" data-resposta="nao" onClick={() => responder(false)}>{pedido.nao ?? 'Não'}</button>
          )}
          <button type="button" autoFocus className={pedido.perigo ? 'btn-perigo' : 'btn-primario'} data-resposta="sim" onClick={() => responder(true)}>
            {pedido.somenteOk ? 'OK' : pedido.sim ?? 'Sim'}
          </button>
        </div>
      </div>
    </div>
  );
}
