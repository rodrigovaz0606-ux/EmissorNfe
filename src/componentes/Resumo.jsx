import React from 'react';
import { moeda, formatarChave } from '../modelo.js';
import { api } from '../api.js';

function Linha({ rotulo, valor, destaque, negativo }) {
  return (
    <div className={`linha ${destaque ? 'destaque' : ''}`}>
      <span>{rotulo}</span>
      <span>{negativo && valor > 0 ? '− ' : ''}{moeda(valor)}</span>
    </div>
  );
}

function Passo({ ok, rotulo, detalhe }) {
  const icone = ok === true ? '✓' : ok === false ? '✕' : '•';
  return (
    <li className={`passo ${ok === true ? 'ok' : ok === false ? 'falha' : ''}`}>
      <span className="passo-icone">{icone}</span>
      <div>
        <strong>{rotulo}</strong>
        {detalhe && <div className="passo-detalhe">{detalhe}</div>}
      </div>
    </li>
  );
}

function Resultado({ r, onVerNotas, onNovaNota }) {
  if (r.erro) {
    return (
      <div className="resultado falha">
        <h3>Não foi possível concluir</h3>
        <p>{r.erro}</p>
      </div>
    );
  }
  const prot = r.protocolo;
  const autorizada = prot?.autorizada;
  const classe = autorizada ? 'sucesso' : r.enviada || !r.xsd?.valido || !r.assinaturaValida ? 'falha' : 'sucesso';
  return (
    <div className={`resultado ${classe}`}>
      <h3>
        {autorizada ? 'Nota autorizada' : r.enviada ? (prot ? 'Nota rejeitada pela SEFAZ' : `Lote: ${r.lote?.xMotivo}`) : r.modo === 'validar' && r.xsd?.valido ? 'Pronta para emitir' : 'Corrija antes de emitir'}
      </h3>
      <div className="chave">
        <span>Chave de acesso</span>
        <code>{formatarChave(r.chave)}</code>
      </div>
      <ul className="passos">
        <Passo ok={r.assinaturaValida} rotulo="Assinatura digital" detalhe={r.assinaturaValida ? 'Conferida' : 'Não confere'} />
        <Passo
          ok={r.xsd?.valido} rotulo="Schema (XSD)"
          detalhe={r.xsd?.valido ? 'Estrutura válida' : <ul className="erros">{r.xsd?.erros.map((e, i) => <li key={i}>{e}</li>)}</ul>}
        />
        {r.enviada && (
          <Passo
            ok={autorizada} rotulo="SEFAZ"
            detalhe={prot ? <><b>{prot.cStat}</b> — {prot.xMotivo}{prot.nProt && <div>Protocolo {prot.nProt}</div>}</> : `${r.lote?.cStat} — ${r.lote?.xMotivo}`}
          />
        )}
      </ul>
      {r.id && (
        <div className="downloads">
          <a className="btn-primario pequeno" href={api.urlDanfe(r.id)} target="_blank" rel="noreferrer">DANFE (PDF)</a>
          <a href={api.urlXml(r.id)}>{autorizada ? 'XML autorizado' : 'XML enviado'}</a>
          <a href={api.urlXml(r.id, 'resposta')}>Resposta da SEFAZ</a>
        </div>
      )}
      {r.enviada && (
        <div className="downloads">
          <button type="button" className="btn-texto" onClick={onVerNotas}>Ver em Notas emitidas ›</button>
          {autorizada && <button type="button" className="btn-texto" onClick={onNovaNota}>Emitir outra nota</button>}
        </div>
      )}
    </div>
  );
}

export function Resumo({ totais, pendencias, avisos, ocupado, resultado, onValidar, onEmitir, onPrevia, onVerNotas, onNovaNota }) {
  const completo = pendencias.length === 0;
  return (
    <aside className="resumo">
      <div className="cartao">
        <h2>Resumo</h2>
        <Linha rotulo="Produtos" valor={totais.vProd} />
        {totais.vDesc > 0 && <Linha rotulo="Descontos" valor={totais.vDesc} negativo />}
        {totais.vFrete > 0 && <Linha rotulo="Frete" valor={totais.vFrete} />}
        <Linha rotulo="Total da nota" valor={totais.vNF} destaque />
        <div className="impostos">
          <span>ICMS {moeda(totais.vICMS)}</span>
          <span>PIS {moeda(totais.vPIS)}</span>
          <span>COFINS {moeda(totais.vCOFINS)}</span>
        </div>
      </div>

      <div className="cartao">
        {completo ? (
          <p className="tudo-certo">✓ Todos os campos obrigatórios preenchidos</p>
        ) : (
          <details className="pendencias" open={pendencias.length <= 6}>
            <summary>{pendencias.length} {pendencias.length === 1 ? 'pendência' : 'pendências'}</summary>
            <ul>{pendencias.map((p) => <li key={p}>{p}</li>)}</ul>
          </details>
        )}
        {avisos.length > 0 && (
          <ul className="avisos">{avisos.map((a) => <li key={a}>⚠ {a}</li>)}</ul>
        )}
        <div className="botoes">
          <button type="button" className="btn-secundario" disabled={!completo || Boolean(ocupado)} onClick={onValidar}>
            {ocupado === 'validar' ? 'Validando…' : 'Validar'}
          </button>
          <button type="button" className="btn-primario" disabled={!completo || Boolean(ocupado)} onClick={onEmitir}>
            {ocupado === 'emitir' ? 'Enviando à SEFAZ…' : 'Emitir nota'}
          </button>
        </div>
        <button type="button" className="btn-texto previa" disabled={!completo || Boolean(ocupado)} onClick={onPrevia}>
          {ocupado === 'previa' ? 'Gerando prévia…' : 'Pré-visualizar DANFE'}
        </button>
        <p className="nota-rodape">“Validar” gera, assina e confere o XML sem enviar. “Emitir” envia para a SEFAZ em homologação.</p>
      </div>

      {resultado && <Resultado r={resultado} onVerNotas={onVerNotas} onNovaNota={onNovaNota} />}
    </aside>
  );
}
