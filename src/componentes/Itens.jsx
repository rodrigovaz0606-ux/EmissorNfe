import React from 'react';
import { Secao, Campo } from './campos.jsx';
import { Tributos } from './Tributos.jsx';
import { CFOPS, UNIDADES } from '../opcoes.js';
import { itemVazio, totaisItem, moeda, num, digitos, itemDoProduto, produtoDoItem, itemEstaVazio, cfopDoTipo, formatarQuantidade } from '../modelo.js';

function ItemCartao({ item, indice, aberto, onAlternar, onChange, onRemover, podeRemover, onSalvarProduto }) {
  const s = (campo) => (v) => onChange({ ...item, [campo]: v });
  const [salvando, setSalvando] = React.useState(false);
  const [aviso, setAviso] = React.useState(null);

  async function salvarProduto() {
    setSalvando(true);
    setAviso(null);
    try {
      setAviso({ ok: true, texto: await onSalvarProduto(item) });
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    } finally {
      setSalvando(false);
    }
  }
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
        <span className="seta" aria-hidden="true">▾</span>
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

          <Tributos item={item} onChange={onChange} valores={{ vICMS: moeda(t.vICMS), vPIS: moeda(t.vPIS), vCOFINS: moeda(t.vCOFINS) }} />

          <div className="grade">
            <Campo rotulo="Informações adicionais do item" valor={item.infAdProd} onChange={s('infAdProd')} largura="cheia" />
          </div>

          {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}
          <div className="item-rodape">
            <button type="button" className="btn-texto" onClick={salvarProduto} disabled={salvando}
              title="Grava código, descrição, NCM, CFOP, unidade, preço e tributação no cadastro de produtos">
              {salvando ? 'Salvando…' : 'Salvar no cadastro de produtos'}
            </button>
            {podeRemover && <button type="button" className="btn-perigo-texto" onClick={onRemover}>Remover item</button>}
          </div>
        </div>
      )}
    </div>
  );
}

// Escolha de um produto cadastrado para virar item da nota.
function EscolherProduto({ produtos, onEscolher }) {
  const id = React.useId();
  if (!produtos) return null;
  return (
    <div className="campo escolher-produto">
      <label className="rotulo" htmlFor={id}>Produto cadastrado</label>
      <select
        id={id} value="" disabled={produtos.length === 0}
        onChange={(e) => { const p = produtos.find((x) => String(x.id) === e.target.value); if (p) onEscolher(p); }}
      >
        <option value="">{produtos.length === 0 ? 'Nenhum produto cadastrado' : 'Adicionar da lista…'}</option>
        {produtos.map((p) => <option key={p.id} value={p.id}>{p.xProd} ({p.cProd}){p.controlaEstoque ? ` · ${formatarQuantidade(p.saldo)} ${p.uCom}` : ''}</option>)}
      </select>
    </div>
  );
}

export function Itens({ nota, set, produtos, onSalvarProduto }) {
  const [aberto, setAberto] = React.useState(0);
  const itens = nota.itens;
  const atualizar = (i) => (novo) => set(['itens'], itens.map((it, j) => (j === i ? novo : it)));

  function adicionarDoCadastro(produto) {
    const novo = itemDoProduto(produto);
    // Em devolução, remessa ou entrada, vale o CFOP da operação, não o CFOP padrão do produto.
    const cfop = cfopDoTipo(nota);
    if (cfop) novo.CFOP = cfop;
    // Nota recém-aberta com um único item em branco: substitui em vez de acrescentar.
    if (itens.length === 1 && itemEstaVazio(itens[0])) {
      set(['itens'], [novo]);
      setAberto(0);
    } else {
      set(['itens'], [...itens, novo]);
      setAberto(itens.length);
    }
  }
  function adicionar() {
    // Novo item herda a tributação e o CFOP do último (costuma ser igual).
    const ultimo = itens.at(-1);
    const novo = { ...itemVazio(), CFOP: ultimo.CFOP, uCom: ultimo.uCom, icms: { ...ultimo.icms }, pis: { ...ultimo.pis }, cofins: { ...ultimo.cofins } };
    set(['itens'], [...itens, novo]);
    setAberto(itens.length);
  }

  return (
    <Secao
      id="itens" numero="4" titulo="Itens da nota" descricao={`${itens.length} ${itens.length === 1 ? 'item' : 'itens'}`}
      acoes={(
        <div className="acoes-itens">
          <EscolherProduto produtos={produtos} onEscolher={adicionarDoCadastro} />
          <button type="button" className="btn-secundario" onClick={adicionar}>+ Item em branco</button>
        </div>
      )}
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
            onSalvarProduto={(it) => onSalvarProduto(produtoDoItem(it))}
          />
        ))}
      </div>
    </Secao>
  );
}
