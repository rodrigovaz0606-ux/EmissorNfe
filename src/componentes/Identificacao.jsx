import React from 'react';
import { Secao, Campo, Selecao } from './campos.jsx';
import { NATUREZAS, TP_NF, FIN_NFE, IND_FINAL, IND_PRES, TIPOS_NOTA } from '../opcoes.js';
import { idDestDe, aplicarTipoNota, aceitaReferencia, precisaReferencia, digitos } from '../modelo.js';

const OPCOES_TIPO = [...TIPOS_NOTA.map((t) => [t.id, t.rotulo]), ['', 'Personalizada']];

// Chaves das notas referenciadas (devolução, complementar, ajuste).
function Referencias({ nota, set }) {
  const lista = nota.ide.refNFe?.length ? nota.ide.refNFe : [''];
  const mudar = (i) => (v) => set(['ide', 'refNFe'], lista.map((r, j) => (j === i ? v : r)));
  return (
    <div className="referencias">
      <h3 className="subtitulo">Notas referenciadas</h3>
      {lista.map((r, i) => (
        <div className="grade linha-pagamento" key={i}>
          <Campo
            rotulo={lista.length === 1 ? 'Chave da NF-e referenciada' : `Chave da NF-e referenciada ${i + 1}`} valor={r} onChange={mudar(i)} largura="g"
            inputMode="numeric" obrigatorio={precisaReferencia(nota) && i === 0} placeholder="44 dígitos"
            erro={digitos(r).length !== 44 && (r || precisaReferencia(nota)) ? `${digitos(r).length} de 44 dígitos` : undefined}
            dica="A chave da nota original aparece no DANFE e no XML"
          />
          {lista.length > 1 && (
            <button type="button" className="btn-icone" title="Remover" onClick={() => set(['ide', 'refNFe'], lista.filter((_, j) => j !== i))}>✕</button>
          )}
        </div>
      ))}
      <button type="button" className="btn-texto" onClick={() => set(['ide', 'refNFe'], [...lista, ''])}>+ Outra nota referenciada</button>
    </div>
  );
}

export function Identificacao({ nota, set, onSerie }) {
  const ide = nota.ide;
  const s = (campo) => (v) => set(['ide', campo], v);
  const idDest = idDestDe(nota);
  return (
    <Secao id="identificacao" numero="1" titulo="Identificação" descricao="Dados gerais da operação">
      <div className="grade">
        <Selecao
          rotulo="Tipo de nota" valor={ide.tipoNota ?? ''} onChange={(id) => set([], aplicarTipoNota(nota, id))} opcoes={OPCOES_TIPO} largura="cheia"
          dica="Preenche natureza, CFOP e pagamento. Confira com o seu contador se servem para a sua operação."
        />
        <Campo
          rotulo="Natureza da operação" valor={ide.natOp} onChange={(v) => s('natOp')(v.toUpperCase())} largura="g" obrigatorio
          lista="naturezas" erro={!ide.natOp.trim() && 'Informe a natureza da operação'}
        />
        <datalist id="naturezas">{NATUREZAS.map((n) => <option key={n} value={n} />)}</datalist>
        <Campo rotulo="Série" valor={ide.serie} onChange={s('serie')} onBlur={() => onSerie?.(ide.serie)} largura="pp" inputMode="numeric" />
        <Campo rotulo="Número" valor={ide.nNF} onChange={s('nNF')} largura="p" inputMode="numeric" obrigatorio erro={!(Number(ide.nNF) >= 1) && 'Inválido'} />
        <Selecao rotulo="Tipo" valor={ide.tpNF} onChange={s('tpNF')} opcoes={TP_NF} largura="p" />
        <Selecao rotulo="Finalidade" valor={ide.finNFe} onChange={s('finNFe')} opcoes={FIN_NFE} largura="p" />
        <Selecao rotulo="Consumidor final?" valor={ide.indFinal} onChange={s('indFinal')} opcoes={IND_FINAL} largura="m" />
        <Selecao rotulo="Presença do comprador" valor={ide.indPres} onChange={s('indPres')} opcoes={IND_PRES} largura="m" />
        <div className="campo l-m">
          <span className="rotulo">Destino da operação</span>
          <div className={`etiqueta ${idDest === 1 ? 'neutra' : 'alerta'}`}>
            {idDest === 1 ? 'Interna (mesma UF)' : 'Interestadual'}
          </div>
          <span className="dica">Calculado pelas UFs do emitente e do destinatário</span>
        </div>
      </div>
      {aceitaReferencia(nota) && <Referencias nota={nota} set={set} />}
    </Secao>
  );
}
