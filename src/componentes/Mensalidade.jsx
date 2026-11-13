import React from 'react';
import { api } from '../api.js';
import { moeda } from '../modelo.js';

const dataBr = (iso) => (iso ? iso.split('-').reverse().join('/') : '—');
const reais = (centavos) => moeda(centavos);
const hora = (unix) => new Date(unix * 1000).toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' });

// Texto curto da situação, usado na faixa do topo e na tela.
export function textoSituacao(m) {
  if (!m) return '';
  const dias = Math.abs(m.dias ?? 0);
  switch (m.situacao) {
    case 'vence_em_breve':
      return m.dias === 0 ? `A mensalidade vence hoje (${reais(m.valor_centavos)}).` : `A mensalidade vence em ${dias} dia(s), em ${dataBr(m.vencimento)}.`;
    case 'atrasado':
      return `Mensalidade atrasada há ${dias} dia(s). A emissão será bloqueada em ${dataBr(m.bloqueio_em)} se não for paga.`;
    case 'bloqueado':
      return `Mensalidade atrasada há ${dias} dias: a emissão deste produtor está bloqueada até o pagamento.`;
    case 'sem_vencimento':
      return 'Este produtor ainda não tem vencimento da mensalidade. Peça ao administrador para definir.';
    default:
      return `Mensalidade em dia. Próximo vencimento: ${dataBr(m.vencimento)}.`;
  }
}

// Faixa no topo da tela, só quando há algo a avisar.
export function FaixaMensalidade({ mensalidade, onAbrir }) {
  if (!mensalidade || mensalidade.situacao === 'em_dia') return null;
  const classe = mensalidade.situacao === 'vence_em_breve' ? 'faixa-simulada' : 'faixa-erro';
  return (
    <div className={classe} role="status">
      {textoSituacao(mensalidade)}{' '}
      {mensalidade.pode_pagar && <button type="button" className="btn-texto" onClick={onAbrir}>Pagar com Pix</button>}
    </div>
  );
}

// Tela da mensalidade: situação, Pix (QR code e copia e cola) e pagamentos anteriores.
// Enquanto o Pix está na tela, confere a cada 5 segundos se já foi pago.
export function Mensalidade({ onPago }) {
  const [dados, setDados] = React.useState(null);
  const [pix, setPix] = React.useState(null);
  const [erro, setErro] = React.useState('');
  const [gerando, setGerando] = React.useState(false);
  const [copiado, setCopiado] = React.useState(false);

  const carregar = React.useCallback(async () => {
    try {
      const d = await api.mensalidade();
      setDados(d);
      setPix(d.pix);
      setErro('');
      return d;
    } catch (e) {
      setErro(e.message);
      return null;
    }
  }, []);
  React.useEffect(() => { carregar(); }, [carregar]);

  // Pago: o vencimento muda. Avisa o App para recarregar a situação (e desbloquear).
  const vencimentoInicial = React.useRef(null);
  React.useEffect(() => {
    if (!dados?.resumo?.vencimento) return;
    if (vencimentoInicial.current === null) vencimentoInicial.current = dados.resumo.vencimento;
    else if (dados.resumo.vencimento !== vencimentoInicial.current) onPago?.();
  }, [dados, onPago]);

  React.useEffect(() => {
    if (!pix) return undefined;
    const t = setInterval(carregar, 5000);
    return () => clearInterval(t);
  }, [pix, carregar]);

  async function gerar() {
    setGerando(true);
    setErro('');
    try {
      const r = await api.gerarPixMensalidade();
      if (r.paga) await carregar();
      else setPix(r);
    } catch (e) {
      setErro(e.message);
    } finally {
      setGerando(false);
    }
  }

  async function copiar() {
    try {
      await navigator.clipboard.writeText(pix.copia_cola);
      setCopiado(true);
      setTimeout(() => setCopiado(false), 2500);
    } catch {
      setErro('Não foi possível copiar. Selecione o código e copie manualmente.');
    }
  }

  const r = dados?.resumo;
  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>Mensalidade</h2>
          <span>Pagamento por Pix. A confirmação é automática, em poucos segundos depois de pagar.</span>
        </div>
      </div>
      <div className="conteudo-lista">
        {erro && <div className="aviso-local falha" role="alert">{erro}</div>}
        {!dados && !erro && <div className="carregando-tela">Carregando…</div>}
        {r && (
          <div className="cartoes-produtores">
            <div className={`cartao-produtor ${r.situacao === 'bloqueado' || r.situacao === 'atrasado' ? 'vence' : ''}`}>
              <div className="cartao-produtor-topo">
                <div>
                  <h3>{reais(r.valor_centavos)} por mês</h3>
                  <span className="doc">{textoSituacao(r)}</span>
                </div>
                {r.situacao === 'bloqueado' && <span className="selo selo-falha">Bloqueado</span>}
              </div>
              <dl>
                <div><dt>Vencimento</dt><dd>{dataBr(r.vencimento)}</dd></div>
                {r.situacao !== 'sem_vencimento' && <div><dt>Bloqueio se não pagar até</dt><dd>{dataBr(r.bloqueio_em)}</dd></div>}
              </dl>
              {!pix && r.situacao !== 'sem_vencimento' && (
                <div className="cartao-produtor-acoes">
                  {r.pode_pagar
                    ? <button type="button" className="btn-primario" onClick={gerar} disabled={gerando}>{gerando ? 'Gerando Pix…' : 'Gerar Pix'}</button>
                    : <span className="doc">O Pix desta mensalidade fica disponível a partir de {dataBr(r.pix_a_partir_de)}.</span>}
                </div>
              )}
            </div>

            {pix && (
              <div className="cartao-produtor">
                <div className="cartao-produtor-topo">
                  <div>
                    <h3>Pague com Pix · {reais(pix.valor_centavos)}</h3>
                    <span className="doc">Escaneie o QR code no app do banco ou use o Pix copia e cola. Válido até {hora(pix.expira_em)}.</span>
                  </div>
                </div>
                <img
                  className="qr-pix" alt="QR code do Pix da mensalidade" width="240" height="240"
                  src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(pix.qr_svg)}`}
                />
                <label className="rotulo" htmlFor="pix-copia-cola">Pix copia e cola</label>
                <textarea id="pix-copia-cola" readOnly rows={3} value={pix.copia_cola} onFocus={(e) => e.target.select()} />
                <div className="cartao-produtor-acoes">
                  <button type="button" className="btn-primario pequeno" onClick={copiar}>{copiado ? 'Copiado!' : 'Copiar código'}</button>
                  <button type="button" className="btn-texto" onClick={carregar}>Já paguei</button>
                </div>
              </div>
            )}

            {dados.pagas?.length > 0 && (
              <div className="cartao-produtor">
                <h3>Mensalidades pagas</h3>
                <dl>
                  {dados.pagas.map((p) => (
                    <div key={p.vencimento}><dt>Vencimento {dataBr(p.vencimento)}</dt><dd>{reais(p.valor_centavos)} · pago em {p.paga_em?.slice(0, 10).split('-').reverse().join('/')}</dd></div>
                  ))}
                </dl>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
