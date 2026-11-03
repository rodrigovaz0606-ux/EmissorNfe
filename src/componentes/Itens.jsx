import React from 'react';
import { Secao, Campo, Selecao } from './campos.jsx';
import { CFOPS, UNIDADES, ORIGENS, CST_ICMS, CST_PIS_COFINS } from '../opcoes.js';
import { itemVazio, totaisItem, moeda, num, digitos } from '../modelo.js';

function ItemCartao({ item, indice, aberto, onAlternar, onChange, onRemover, podeRemover }) {
  const s = (campo) => (v) => onChange({ ...item, [campo]: v });
  const sImp = (grupo, campo) => (v) => onChange({ ...item, [grupo]: { ...item[grupo], [campo]: v } });
  const t = totaisItem(item);
  const decimal = (v) => (v === '' || Number.isFinite(num(v)) ? undefined : 'Número inválido');

  return (
    <div className={`item ${aberto ? 'aberto' : ''}`}>
      <button type="button" className="item-cabecalho" onClick={onAlternar} aria-expanded={aberto}>
        <span className="item-numero">{indice + 1}</span>
        <span className="item-descricao">{item.xProd || <em>Novo item</em>}</span>
        <span className="item-conta">
          {(num(item.qCom) || 0).toLocaleString('pt-BR', { maximumFractionDigits: 4 })} {item.uCom} ×{' '}
          {(num(item.vUnCom) || 0).toLocaleString('pt-BR', { style: 'currency', currency: 'BRL', maximumFractionDigits: 4 })}
        </span>
        <strong className="item-total">{moeda(t.vProd)}</strong>
        <span className="seta">{aberto ? '▴' : '▾'}</span>
      </button>

      {aberto && (
        <div className="item-corpo">
          <div className="grade">
            <Campo rotulo="Código" valor={item.cProd} onChange={s('cProd')} largura="p" />
            <Campo rotulo="Descrição" valor={item.xProd} onChange={(v) => s('xProd')(v.toUpperCase())} largura="g" obrigatorio erro={!item.xProd.trim() && 'Informe a descrição'} />
            <Campo
              rotulo="NCM" valor={item.NCM} onChange={s('NCM')} largura="p" inputMode="numeric" obrigatorio placeholder="0000.00.00"
              erro={!/^\d{8}$/.test(digitos(item.NCM)) && '8 dígitos'}
            />
            <Campo
              rotulo="CFOP" valor={item.CFOP} onChange={s('CFOP')} largura="pp" inputMode="numeric" obrigatorio lista="cfops"
              erro={!/^\d{4}$/.test(digitos(item.CFOP)) && '4 dígitos'}
              dica={CFOPS.find(([c]) => c === digitos(item.CFOP))?.[1]}
            />
            <Campo rotulo="Unidade" valor={item.uCom} onChange={(v) => s('uCom')(v.toUpperCase())} largura="pp" lista="unidades" />
            <Campo rotulo="Quantidade" valor={item.qCom} onChange={s('qCom')} largura="p" inputMode="decimal" obrigatorio erro={!(num(item.qCom) > 0) && 'Maior que zero'} />
            <Campo rotulo="Valor unitário" valor={item.vUnCom} onChange={s('vUnCom')} largura="p" inputMode="decimal" obrigatorio placeholder="0,00" erro={!(num(item.vUnCom) > 0) && 'Maior que zero'} />
            <Campo rotulo="Desconto" valor={item.vDesc} onChange={s('vDesc')} largura="p" inputMode="decimal" placeholder="0,00" erro={decimal(item.vDesc)} />
            <Campo rotulo="Frete" valor={item.vFrete} onChange={s('vFrete')} largura="p" inputMode="decimal" placeholder="0,00" erro={decimal(item.vFrete)} />
          </div>

          <div className="tributos">
            <div className="tributo">
              <h4>ICMS</h4>
              <Selecao rotulo="Origem" valor={item.icms.orig} onChange={sImp('icms', 'orig')} opcoes={ORIGENS} largura="cheia" />
              <Selecao rotulo="Situação tributária" valor={item.icms.CST} onChange={sImp('icms', 'CST')} opcoes={CST_ICMS} largura="cheia" />
              {item.icms.CST === '00' && (
                <Campo rotulo="Alíquota %" valor={item.icms.pICMS} onChange={sImp('icms', 'pICMS')} largura="cheia" inputMode="decimal" obrigatorio
                  erro={!(num(item.icms.pICMS) >= 0) && 'Informe a alíquota'} dica={`ICMS: ${moeda(t.vICMS)}`} />
              )}
            </div>
            <div className="tributo">
              <h4>PIS</h4>
              <Selecao rotulo="Situação tributária" valor={item.pis.CST} onChange={sImp('pis', 'CST')} opcoes={CST_PIS_COFINS} largura="cheia" />
              {['01', '02'].includes(item.pis.CST) && (
                <Campo rotulo="Alíquota %" valor={item.pis.pPIS} onChange={sImp('pis', 'pPIS')} largura="cheia" inputMode="decimal" obrigatorio
                  erro={!(num(item.pis.pPIS) >= 0) && 'Informe a alíquota'} dica={`PIS: ${moeda(t.vPIS)}`} />
              )}
            </div>
            <div className="tributo">
              <h4>COFINS</h4>
              <Selecao rotulo="Situação tributária" valor={item.cofins.CST} onChange={sImp('cofins', 'CST')} opcoes={CST_PIS_COFINS} largura="cheia" />
              {['01', '02'].includes(item.cofins.CST) && (
                <Campo rotulo="Alíquota %" valor={item.cofins.pCOFINS} onChange={sImp('cofins', 'pCOFINS')} largura="cheia" inputMode="decimal" obrigatorio
                  erro={!(num(item.cofins.pCOFINS) >= 0) && 'Informe a alíquota'} dica={`COFINS: ${moeda(t.vCOFINS)}`} />
              )}
            </div>
          </div>

          <div className="grade">
            <Campo rotulo="Informações adicionais do item" valor={item.infAdProd} onChange={s('infAdProd')} largura="cheia" />
          </div>

          {podeRemover && (
            <div className="item-rodape">
              <button type="button" className="btn-perigo-texto" onClick={onRemover}>Remover item</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function Itens({ nota, set }) {
  const [aberto, setAberto] = React.useState(0);
  const itens = nota.itens;
  const atualizar = (i) => (novo) => set(['itens'], itens.map((it, j) => (j === i ? novo : it)));

  function adicionar() {
    // Novo item herda a tributação e o CFOP do último (costuma ser igual).
    const ultimo = itens.at(-1);
    const novo = { ...itemVazio(), CFOP: ultimo.CFOP, uCom: ultimo.uCom, icms: { ...ultimo.icms }, pis: { ...ultimo.pis }, cofins: { ...ultimo.cofins } };
    set(['itens'], [...itens, novo]);
    setAberto(itens.length);
  }

  return (
    <Secao
      id="itens" numero="4" titulo="Produtos" descricao={`${itens.length} ${itens.length === 1 ? 'item' : 'itens'}`}
      acoes={<button type="button" className="btn-secundario" onClick={adicionar}>+ Adicionar item</button>}
    >
      <datalist id="cfops">{CFOPS.map(([c, d]) => <option key={c} value={c}>{d}</option>)}</datalist>
      <datalist id="unidades">{UNIDADES.map((u) => <option key={u} value={u} />)}</datalist>
      <div className="itens">
        {itens.map((item, i) => (
          <ItemCartao
            key={i} item={item} indice={i} aberto={aberto === i}
            onAlternar={() => setAberto(aberto === i ? -1 : i)}
            onChange={atualizar(i)}
            onRemover={() => { set(['itens'], itens.filter((_, j) => j !== i)); setAberto(-1); }}
            podeRemover={itens.length > 1}
          />
        ))}
      </div>
    </Secao>
  );
}
