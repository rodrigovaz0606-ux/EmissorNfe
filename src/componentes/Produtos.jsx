import React from 'react';
import { api } from '../api.js';
import { Campo, Selecao } from './campos.jsx';
import { EsqueletoTabela } from './Movimento.jsx';
import { Janela } from './Janela.jsx';
import { Tributos } from './Tributos.jsx';
import { CFOPS, UNIDADES, CST_ICMS } from '../opcoes.js';
import { produtoVazio, num, digitos, formatarQuantidade, estoqueBaixo } from '../modelo.js';

const preco = (v) => {
  const n = num(v);
  return v === '' || v === undefined ? '—' : Number.isFinite(n) ? n.toLocaleString('pt-BR', { style: 'currency', currency: 'BRL', maximumFractionDigits: 4 }) : v;
};
const ncmFormatado = (n) => (n?.length === 8 ? `${n.slice(0, 4)}.${n.slice(4, 6)}.${n.slice(6)}` : n);
const nomeCst = (cst) => CST_ICMS.find(([c]) => c === cst)?.[1].split(' — ')[1] ?? cst;

const TIPOS_MOVIMENTO = {
  entrada: 'Entrada', saida: 'Saída', ajuste: 'Ajuste de contagem', nota: 'Nota', estorno: 'Cancelamento de nota',
};
const dataHora = (s) => new Date(s.replace(' ', 'T')).toLocaleString('pt-BR', { dateStyle: 'short', timeStyle: 'short' });

// Saldo, novo movimento e histórico de um produto.
function JanelaEstoque({ produto, onAlterado, onFechar }) {
  const [dados, setDados] = React.useState(null);
  const [tipo, setTipo] = React.useState('entrada');
  const [quantidade, setQuantidade] = React.useState('');
  const [observacao, setObservacao] = React.useState('');
  const [erro, setErro] = React.useState('');
  const [salvando, setSalvando] = React.useState(false);

  const carregar = React.useCallback(() => api.estoqueProduto(produto.id).then(setDados).catch((e) => setErro(e.message)), [produto.id]);
  React.useEffect(() => { carregar(); }, [carregar]);

  async function registrar(e) {
    e.preventDefault();
    setSalvando(true);
    setErro('');
    try {
      await api.movimentarEstoque(produto.id, { tipo, quantidade, observacao });
      setQuantidade('');
      setObservacao('');
      await carregar();
      onAlterado();
    } catch (e2) {
      setErro(e2.message);
    } finally {
      setSalvando(false);
    }
  }

  const baixo = dados && estoqueBaixo({ controlaEstoque: true, saldo: dados.saldo, estoqueMinimo: String(dados.minimo) });
  const quantidadeOk = num(quantidade) > 0 || (tipo === 'ajuste' && num(quantidade) === 0);
  return (
    <Janela titulo={`Estoque de ${produto.xProd}`} onFechar={onFechar} largura={760}>
      {dados && (
        <div className="estoque-saldo">
          <span className="indicador-rotulo">Saldo atual</span>
          <strong className="indicador-valor">{formatarQuantidade(dados.saldo)} {dados.uCom}</strong>
          {baixo && <span className="selo selo-alerta">⚠ Abaixo do mínimo de {formatarQuantidade(dados.minimo)}</span>}
        </div>
      )}
      <form onSubmit={registrar} className="grade form-estoque">
        <Selecao rotulo="Movimento" valor={tipo} onChange={setTipo} largura="m" opcoes={[['entrada', 'Entrada'], ['saida', 'Saída'], ['ajuste', 'Ajuste de contagem']]} />
        <Campo
          rotulo={tipo === 'ajuste' ? 'Saldo contado' : 'Quantidade'} valor={quantidade} onChange={setQuantidade} largura="m" inputMode="decimal" obrigatorio
          dica={tipo === 'ajuste' ? 'O saldo passa a ser este número' : undefined}
        />
        <Campo rotulo="Observação" valor={observacao} onChange={setObservacao} largura="m" maxLength={200} dica="Opcional" />
        <div className="l-cheia janela-botoes">
          <button type="submit" className="btn-primario" disabled={salvando || !quantidadeOk}>{salvando ? 'Registrando…' : 'Registrar movimento'}</button>
        </div>
      </form>
      {erro && <div className="faixa-erro-local" role="alert">{erro}</div>}

      <h3 className="subtitulo">Histórico</h3>
      {dados && dados.movimentos.length === 0 && <p className="vazio-pequeno">Nenhum movimento ainda.</p>}
      {dados && dados.movimentos.length > 0 && (
        <table className="tabela-movimentos">
          <thead><tr><th>Data</th><th>Movimento</th><th className="direita">Quantidade</th><th className="direita">Saldo</th><th>Detalhe</th></tr></thead>
          <tbody>
            {dados.movimentos.map((m) => (
              <tr key={m.id}>
                <td>{dataHora(m.criado_em)}</td>
                <td>{TIPOS_MOVIMENTO[m.tipo] ?? m.tipo}</td>
                <td className="direita">{m.quantidade > 0 ? '+' : ''}{formatarQuantidade(m.quantidade)}</td>
                <td className="direita">{formatarQuantidade(m.saldo)}</td>
                <td>{[m.nNF ? `Nota nº ${m.nNF}` : null, m.observacao, m.usuario].filter(Boolean).join(' · ') || '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Janela>
  );
}

function FormularioProduto({ inicial, onSalvo, onFechar }) {
  const [p, setP] = React.useState(() => ({ ...produtoVazio(), ...inicial }));
  const [salvando, setSalvando] = React.useState(false);
  const [erro, setErro] = React.useState('');
  const s = (campo, maiusculas) => (v) => setP((x) => ({ ...x, [campo]: maiusculas ? v.toUpperCase() : v }));
  const novo = !inicial?.id;

  const problemas = [
    !p.cProd.trim() && 'código',
    !p.xProd.trim() && 'descrição',
    !/^\d{8}$/.test(digitos(p.NCM)) && 'NCM',
    !/^\d{4}$/.test(digitos(p.CFOP)) && 'CFOP',
    !p.uCom.trim() && 'unidade',
    p.controlaEstoque && p.estoqueMinimo !== '' && !(num(p.estoqueMinimo) >= 0) && 'estoque mínimo',
    p.controlaEstoque && novo && p.estoqueInicial !== '' && !(num(p.estoqueInicial) >= 0) && 'saldo inicial',
  ].filter(Boolean);

  async function salvar(e) {
    e.preventDefault();
    setSalvando(true);
    setErro('');
    try {
      const { id, ...dados } = p;
      const r = novo ? await api.criarProduto(dados) : await api.alterarProduto(id, dados);
      onSalvo(r, novo);
    } catch (e2) {
      setErro(e2.message);
      setSalvando(false);
    }
  }

  return (
    <Janela titulo={novo ? 'Novo produto' : `Editar ${inicial.xProd}`} onFechar={salvando ? () => {} : onFechar} largura={820}>
      <form onSubmit={salvar} className="form-produto">
        <div className="grade">
          <Campo rotulo="Código" valor={p.cProd} onChange={s('cProd')} largura="p" obrigatorio maxLength={60} autoFocus
            erro={!p.cProd.trim() && 'Informe o código'} />
          <Campo rotulo="Descrição" valor={p.xProd} onChange={s('xProd', true)} largura="g" obrigatorio maxLength={120}
            erro={!p.xProd.trim() && 'Informe a descrição'} />
          <Campo rotulo="NCM" valor={p.NCM} onChange={s('NCM')} largura="p" inputMode="numeric" obrigatorio placeholder="0000.00.00"
            erro={!/^\d{8}$/.test(digitos(p.NCM)) && '8 dígitos'} />
          <Campo rotulo="CFOP padrão" valor={p.CFOP} onChange={s('CFOP')} largura="p" inputMode="numeric" obrigatorio lista="cfops-produto"
            erro={!/^\d{4}$/.test(digitos(p.CFOP)) && '4 dígitos'} dica={CFOPS.find(([c]) => c === digitos(p.CFOP))?.[1]} />
          <Campo rotulo="Unidade" valor={p.uCom} onChange={s('uCom', true)} largura="pp" obrigatorio maxLength={6} lista="unidades-produto" />
          <Campo rotulo="Preço unitário" valor={p.vUnCom} onChange={s('vUnCom')} largura="p" inputMode="decimal" placeholder="0,00"
            erro={p.vUnCom !== '' && !(num(p.vUnCom) >= 0) && 'Número inválido'} dica="Opcional: dá para mudar na nota" />
        </div>
        <datalist id="cfops-produto">{CFOPS.map(([c, d]) => <option key={c} value={c}>{d}</option>)}</datalist>
        <datalist id="unidades-produto">{UNIDADES.map((u) => <option key={u} value={u} />)}</datalist>

        <div className="bloco-estoque">
          <label className="marcador">
            <input type="checkbox" checked={Boolean(p.controlaEstoque)} onChange={(e) => setP((x) => ({ ...x, controlaEstoque: e.target.checked }))} />
            Controlar estoque deste produto
          </label>
          {p.controlaEstoque && (
            <div className="grade">
              {novo && (
                <Campo rotulo="Saldo inicial" valor={p.estoqueInicial} onChange={s('estoqueInicial')} largura="m" inputMode="decimal" placeholder="0"
                  dica={`Quanto você tem hoje, em ${p.uCom || 'unidades'}`} />
              )}
              {!novo && (
                <div className="campo l-m">
                  <span className="rotulo">Saldo atual</span>
                  <div className="valor-fixo">{formatarQuantidade(p.saldo)} {p.uCom}</div>
                  <span className="dica">Para mudar, use o botão Estoque na lista</span>
                </div>
              )}
              <Campo rotulo="Estoque mínimo" valor={p.estoqueMinimo} onChange={s('estoqueMinimo')} largura="m" inputMode="decimal" placeholder="0"
                dica="Abaixo disso, o produto aparece com alerta" />
            </div>
          )}
          <span className="dica">Com o controle ligado, notas autorizadas baixam ou aumentam o saldo, e o cancelamento devolve.</span>
        </div>

        <Tributos item={p} onChange={setP} />

        {erro && <div className="faixa-erro-local">{erro}</div>}
        <div className="janela-botoes">
          {problemas.length > 0 && <span className="falta">Falta: {problemas.join(', ')}</span>}
          <button type="button" className="btn-secundario" onClick={onFechar} disabled={salvando}>Cancelar</button>
          <button type="submit" className="btn-primario" disabled={salvando || problemas.length > 0}>
            {salvando ? 'Salvando…' : novo ? 'Cadastrar produto' : 'Salvar alterações'}
          </button>
        </div>
      </form>
    </Janela>
  );
}

export function Produtos({ onAlterados }) {
  const [lista, setLista] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [busca, setBusca] = React.useState('');
  const [editando, setEditando] = React.useState(null); // {} = novo
  const [aviso, setAviso] = React.useState(null);
  const [estoqueDe, setEstoqueDe] = React.useState(null);
  const [soBaixo, setSoBaixo] = React.useState(false);

  const carregar = React.useCallback(() => api.produtos().then(setLista).catch((e) => setErro(e.message)), []);
  React.useEffect(() => { carregar(); }, [carregar]);

  async function aposMudanca(texto) {
    setAviso({ ok: true, texto });
    await carregar();
    onAlterados?.();
  }

  async function excluir(p) {
    if (!window.confirm(`Excluir o produto ${p.xProd} (${p.cProd}) do cadastro? O histórico de estoque dele também será apagado. Notas já emitidas não mudam.`)) return;
    try {
      await api.excluirProduto(p.id);
      await aposMudanca(`Produto ${p.xProd} excluído.`);
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    }
  }

  const termo = busca.trim().toUpperCase();
  const filtrados = (lista ?? []).filter((p) => !termo || p.xProd.includes(termo) || p.cProd.toUpperCase().includes(termo) || (digitos(termo) && p.NCM.includes(digitos(termo)))).filter((p) => !soBaixo || estoqueBaixo(p));
  const quantosBaixos = (lista ?? []).filter(estoqueBaixo).length;

  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>Produtos</h2>
          <span>{lista ? `${lista.length} ${lista.length === 1 ? 'produto cadastrado' : 'produtos cadastrados'}` : 'Carregando…'}</span>
        </div>
        <div className="barra-botoes">
          {quantosBaixos > 0 && (
            <label className="marcador">
              <input type="checkbox" checked={soBaixo} onChange={(e) => setSoBaixo(e.target.checked)} />
              Só estoque baixo ({quantosBaixos})
            </label>
          )}
          {lista?.length > 0 && (
            <input type="search" className="busca" placeholder="Buscar por descrição, código ou NCM" value={busca}
              onChange={(e) => setBusca(e.target.value)} aria-label="Buscar produto" />
          )}
          <button type="button" className="btn-primario" onClick={() => { setAviso(null); setEditando({}); }}>+ Novo produto</button>
        </div>
      </div>

      <div className="conteudo-lista">
        {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}
        {erro && <div className="aviso-local falha">{erro}</div>}
        {!lista && !erro && <EsqueletoTabela />}
        {lista && lista.length === 0 && (
          <div className="vazio">
            <div className="vazio-icone">📦</div>
            <h3>Nenhum produto cadastrado</h3>
            <p>Cadastre os produtos que você vende para preencher os itens da nota com um clique.</p>
          </div>
        )}
        {lista && lista.length > 0 && (
          <table className="tabela-notas">
            <thead>
              <tr><th>Código</th><th>Descrição</th><th>NCM</th><th>CFOP</th><th>Un.</th><th className="direita">Preço</th><th>ICMS</th><th>Estoque</th><th /></tr>
            </thead>
            <tbody>
              {filtrados.map((p) => (
                <tr key={p.id} onClick={() => { setAviso(null); setEditando(p); }} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && setEditando(p)}>
                  <td><strong>{p.cProd}</strong></td>
                  <td className="cortar">{p.xProd}</td>
                  <td>{ncmFormatado(p.NCM)}</td>
                  <td>{p.CFOP}</td>
                  <td>{p.uCom}</td>
                  <td className="direita">{preco(p.vUnCom)}</td>
                  <td>{p.icms.CST} · {nomeCst(p.icms.CST)}{p.icms.CST === '00' ? ` ${p.icms.pICMS}%` : ''}</td>
                  <td>
                    {p.controlaEstoque ? (
                      <>
                        <strong>{formatarQuantidade(p.saldo)} {p.uCom}</strong>
                        {estoqueBaixo(p) && <span className="selo selo-alerta selo-baixo">⚠ Baixo</span>}
                      </>
                    ) : '—'}
                  </td>
                  <td className="direita acoes-linha">
                    {p.controlaEstoque && (
                      <button type="button" className="btn-texto" onClick={(e) => { e.stopPropagation(); setEstoqueDe(p); }} aria-label={`Estoque de ${p.xProd}`}>Estoque</button>
                    )}
                    <button type="button" className="btn-perigo-texto" onClick={(e) => { e.stopPropagation(); excluir(p); }} aria-label={`Excluir ${p.xProd}`}>Excluir</button>
                  </td>
                </tr>
              ))}
              {filtrados.length === 0 && (
                <tr><td colSpan={9} className="vazio-pequeno">Nenhum produto encontrado{busca ? ` para “${busca}”` : ''}.</td></tr>
              )}
            </tbody>
          </table>
        )}
      </div>

      {estoqueDe && <JanelaEstoque produto={estoqueDe} onAlterado={() => { carregar(); onAlterados?.(); }} onFechar={() => setEstoqueDe(null)} />}
      {editando && (
        <FormularioProduto
          inicial={editando}
          onFechar={() => setEditando(null)}
          onSalvo={(r, novo) => { setEditando(null); aposMudanca(novo ? `Produto ${r.xProd} cadastrado.` : `Produto ${r.xProd} alterado.`); }}
        />
      )}
    </div>
  );
}
