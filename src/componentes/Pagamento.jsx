import React from 'react';
import { Secao, Campo, Selecao } from './campos.jsx';
import { MOD_FRETE, T_PAG } from '../opcoes.js';
import { moeda, num } from '../modelo.js';

export function Pagamento({ nota, set, vNF }) {
  const pag = nota.pag;
  const unico = pag.length === 1;
  const atualizar = (i, campo) => (v) => set(['pag'], pag.map((p, j) => (j === i ? { ...p, [campo]: v } : p)));

  return (
    <Secao id="pagamento" numero="5" titulo="Pagamento e transporte">
      <div className="pagamentos">
        {pag.map((p, i) => (
          <div className="grade linha-pagamento" key={i}>
            <Selecao rotulo={unico ? 'Forma de pagamento' : `Forma ${i + 1}`} valor={p.tPag} onChange={atualizar(i, 'tPag')} opcoes={T_PAG} largura="m" />
            {p.tPag === '90' ? (
              <div className="campo l-p"><span className="rotulo">Valor</span><div className="valor-fixo">R$ 0,00</div></div>
            ) : (
              <Campo
                rotulo="Valor" valor={p.vPag} onChange={atualizar(i, 'vPag')} largura="p" inputMode="decimal"
                placeholder={unico ? moeda(vNF) : '0,00'} dica={unico && p.vPag === '' ? 'Igual ao total da nota' : undefined}
                erro={p.vPag !== '' && !Number.isFinite(num(p.vPag)) ? 'Número inválido' : !unico && p.vPag === '' ? 'Informe o valor' : undefined}
              />
            )}
            {!unico && (
              <button type="button" className="btn-icone" title="Remover" onClick={() => set(['pag'], pag.filter((_, j) => j !== i))}>✕</button>
            )}
          </div>
        ))}
        <button type="button" className="btn-texto" onClick={() => set(['pag'], [...pag, { tPag: '17', vPag: '' }])}>+ Outra forma de pagamento</button>
      </div>

      <div className="grade">
        <Selecao rotulo="Modalidade do frete" valor={nota.transp.modFrete} onChange={(v) => set(['transp', 'modFrete'], v)} opcoes={MOD_FRETE} largura="m" />
      </div>

      <h3 className="subtitulo">Informações complementares</h3>
      <textarea
        className="texto-livre" rows={3} value={nota.infAdic.infCpl} maxLength={5000}
        onChange={(e) => set(['infAdic', 'infCpl'], e.target.value)}
        placeholder="Texto que aparece no campo de informações complementares do DANFE"
      />
    </Secao>
  );
}
