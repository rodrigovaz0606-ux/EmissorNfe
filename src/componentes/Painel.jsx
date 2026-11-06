import React from 'react';
import { api } from '../api.js';
import { moeda } from '../modelo.js';
import { formatarDoc } from './Produtores.jsx';

const NOMES_MES = ['jan', 'fev', 'mar', 'abr', 'mai', 'jun', 'jul', 'ago', 'set', 'out', 'nov', 'dez'];
const nomeMes = (aaaaMm) => {
  const [a, m] = aaaaMm.split('-');
  return `${NOMES_MES[Number(m) - 1]}/${a.slice(2)}`;
};
const data = (iso) => new Date(iso).toLocaleDateString('pt-BR');

// Primeiro e último dia do mês atual, para os atalhos que abrem a lista filtrada.
function esteMes() {
  const h = new Date();
  const dia = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  return { de: dia(new Date(h.getFullYear(), h.getMonth(), 1)), ate: dia(new Date(h.getFullYear(), h.getMonth() + 1, 0)) };
}

function Indicador({ rotulo, valor, nota, onClick }) {
  const conteudo = (
    <>
      <span className="indicador-rotulo">{rotulo}</span>
      <span className="indicador-valor">{valor}</span>
      {nota && <span className="indicador-nota">{nota}</span>}
    </>
  );
  return onClick
    ? <button type="button" className="indicador" onClick={onClick}>{conteudo}</button>
    : <div className="indicador">{conteudo}</div>;
}

// Barras de uma série só: valor autorizado por mês. O título do cartão nomeia a série.
function GraficoMeses({ meses }) {
  const maior = Math.max(...meses.map((m) => m.valor));
  if (maior === 0) return <p className="vazio-pequeno">Nenhuma nota autorizada nos últimos 6 meses.</p>;
  const ultimoComValor = meses.reduce((u, m, i) => (m.valor > 0 ? i : u), -1);
  return (
    <>
      <div className="grafico-barras">
        {meses.map((m, i) => (
          <div className="coluna-barra" key={m.mes} tabIndex={0} aria-label={`${nomeMes(m.mes)}: ${moeda(m.valor)}, ${m.notas} notas`}>
            <span className="dica-barra">{nomeMes(m.mes)} · {moeda(m.valor)} · {m.notas} {m.notas === 1 ? 'nota' : 'notas'}</span>
            {i === ultimoComValor && <span className="valor-barra">{moeda(m.valor)}</span>}
            {m.valor > 0 && <div className="barra" style={{ height: `${Math.max((m.valor / maior) * 78, 2)}%` }} />}
          </div>
        ))}
      </div>
      <div className="eixo-meses">{meses.map((m) => <span key={m.mes}>{nomeMes(m.mes)}</span>)}</div>
      <details>
        <summary>Ver como tabela</summary>
        <table className="tabela-painel">
          <thead><tr><th>Mês</th><th>Valor autorizado</th><th>Notas</th></tr></thead>
          <tbody>{meses.map((m) => <tr key={m.mes}><td>{nomeMes(m.mes)}</td><td>{moeda(m.valor)}</td><td>{m.notas}</td></tr>)}</tbody>
        </table>
      </details>
    </>
  );
}

export function Painel({ onVerNotas, onVerProdutores }) {
  const [p, setP] = React.useState(null);
  const [erro, setErro] = React.useState('');
  React.useEffect(() => { api.painel().then(setP).catch((e) => setErro(e.message)); }, []);
  const notasDoMes = (status) => () => onVerNotas({ ...esteMes(), status });

  return (
    <div className="pagina-notas">
      <div className="barra-acoes">
        <div>
          <h2>Painel</h2>
          <span>Mês atual do produtor em uso. Datas pelo dia em que a nota foi gravada.</span>
        </div>
      </div>
      <div className="conteudo-lista">
        {erro && <div className="aviso-local falha">{erro}</div>}
        {p && (
          <>
            <div className="painel-indicadores">
              <Indicador rotulo="Total autorizado no mês" valor={moeda(p.mes.valor_autorizado)} nota="Não soma notas canceladas" onClick={notasDoMes('autorizada')} />
              <Indicador rotulo="Autorizadas" valor={p.mes.autorizadas} onClick={notasDoMes('autorizada')} />
              <Indicador rotulo="Rejeitadas" valor={p.mes.rejeitadas} nota="Clique para ver os motivos" onClick={notasDoMes('rejeitada')} />
              <Indicador rotulo="Pendentes" valor={p.mes.pendentes} nota="Sem resposta final da SEFAZ" onClick={notasDoMes('pendente')} />
              <Indicador rotulo="Canceladas" valor={p.mes.canceladas} onClick={notasDoMes('cancelada')} />
            </div>

            <section className="painel-cartao">
              <h3>Certificados perto de vencer</h3>
              {p.certificados.length === 0 ? (
                <p className="vazio-pequeno">Nenhum certificado vence nos próximos 30 dias.</p>
              ) : (
                <ul className="lista-certificados">
                  {p.certificados.map((c) => (
                    <li key={c.id}>
                      <span className={`selo ${c.dias < 0 ? 'selo-falha' : 'selo-alerta'}`}>⚠ {c.dias < 0 ? 'Vencido' : c.dias === 0 ? 'Vence hoje' : `${c.dias} dias`}</span>
                      <span><strong>{c.titular}</strong> · {formatarDoc(c.documento)} · válido até {data(c.validoAte)}</span>
                      <button type="button" className="btn-texto" onClick={onVerProdutores}>Renovar</button>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <section className="painel-cartao">
              <h3>Valor autorizado por mês</h3>
              <GraficoMeses meses={p.meses} />
            </section>
          </>
        )}
      </div>
    </div>
  );
}
