import React from 'react';
import { Campo, Selecao } from './campos.jsx';
import { ORIGENS, CST_ICMS, CST_PIS_COFINS } from '../opcoes.js';
import { num } from '../modelo.js';

// ICMS, PIS e COFINS de um item (ou de um produto do cadastro).
// `valores` (opcional) mostra o imposto calculado embaixo da alíquota.
export function Tributos({ item, onChange, valores }) {
  const sImp = (grupo, campo) => (v) => onChange({ ...item, [grupo]: { ...item[grupo], [campo]: v } });
  const dica = (rotulo, v) => (valores ? `${rotulo}: ${valores[v]}` : undefined);
  const tributado = (cst) => ['01', '02'].includes(cst);

  return (
    <div className="tributos">
      <div className="tributo">
        <h4>ICMS</h4>
        <Selecao rotulo="Origem" valor={item.icms.orig} onChange={sImp('icms', 'orig')} opcoes={ORIGENS} largura="cheia" />
        <Selecao rotulo="Situação tributária" valor={item.icms.CST} onChange={sImp('icms', 'CST')} opcoes={CST_ICMS} largura="cheia" />
        {item.icms.CST === '00' && (
          <Campo rotulo="Alíquota %" valor={item.icms.pICMS} onChange={sImp('icms', 'pICMS')} largura="cheia" inputMode="decimal" obrigatorio
            erro={!(num(item.icms.pICMS) >= 0) && 'Informe a alíquota'} dica={dica('ICMS', 'vICMS')} />
        )}
      </div>
      <div className="tributo">
        <h4>PIS</h4>
        <Selecao rotulo="Situação tributária" valor={item.pis.CST} onChange={sImp('pis', 'CST')} opcoes={CST_PIS_COFINS} largura="cheia" />
        {tributado(item.pis.CST) && (
          <Campo rotulo="Alíquota %" valor={item.pis.pPIS} onChange={sImp('pis', 'pPIS')} largura="cheia" inputMode="decimal" obrigatorio
            erro={!(num(item.pis.pPIS) >= 0) && 'Informe a alíquota'} dica={dica('PIS', 'vPIS')} />
        )}
      </div>
      <div className="tributo">
        <h4>COFINS</h4>
        <Selecao rotulo="Situação tributária" valor={item.cofins.CST} onChange={sImp('cofins', 'CST')} opcoes={CST_PIS_COFINS} largura="cheia" />
        {tributado(item.cofins.CST) && (
          <Campo rotulo="Alíquota %" valor={item.cofins.pCOFINS} onChange={sImp('cofins', 'pCOFINS')} largura="cheia" inputMode="decimal" obrigatorio
            erro={!(num(item.cofins.pCOFINS) >= 0) && 'Informe a alíquota'} dica={dica('COFINS', 'vCOFINS')} />
        )}
      </div>
    </div>
  );
}
