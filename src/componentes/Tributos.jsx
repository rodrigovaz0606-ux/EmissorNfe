import React from 'react';
import { Campo, Selecao } from './campos.jsx';
import { ORIGENS, CST_ICMS, CST_PIS_COFINS, CLASSIFICACOES_IBSCBS } from '../opcoes.js';
import { num, pedeCbenef, cBenefValido } from '../modelo.js';

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
        {(pedeCbenef(item) || item.cBenef) && (
          <Campo
            rotulo="Cód. benefício fiscal (cBenef)" valor={item.cBenef ?? ''} largura="cheia" maxLength={10} placeholder="8 ou 10 caracteres"
            onChange={(v) => onChange({ ...item, cBenef: v.toUpperCase() })}
            erro={!cBenefValido(item.cBenef) && '8 ou 10 caracteres'}
            dica="Goiás exige para este CST. O código vem da tabela da SEFAZ-GO: peça ao contador."
          />
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
      <div className="tributo tributo-largo">
        <h4>IBS / CBS (reforma tributária)</h4>
        <Selecao
          rotulo="Classificação (cClassTrib)" valor={item.ibscbs?.cClassTrib ?? '410014'} opcoes={CLASSIFICACOES_IBSCBS} largura="cheia"
          onChange={(v) => onChange({ ...item, ibscbs: { cClassTrib: v } })}
          dica={(item.ibscbs?.cClassTrib ?? '410014') === '410014'
            ? 'Sem valores de IBS/CBS: produtor rural com receita até R$ 3,6 milhões/ano não é contribuinte (art. 164 da LC 214/2025).'
            : valores ? `Em 2026: IBS ${valores.vIBS} · CBS ${valores.vCBS} (fase de teste, sem recolhimento).` : 'Em 2026: IBS 0,1% e CBS 0,9% (fase de teste, sem recolhimento).'}
        />
      </div>
    </div>
  );
}
