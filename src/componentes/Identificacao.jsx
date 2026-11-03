import React from 'react';
import { Secao, Campo, Selecao } from './campos.jsx';
import { NATUREZAS, TP_NF, FIN_NFE, IND_FINAL, IND_PRES } from '../opcoes.js';
import { idDestDe } from '../modelo.js';

export function Identificacao({ nota, set, onSerie }) {
  const ide = nota.ide;
  const s = (campo) => (v) => set(['ide', campo], v);
  const idDest = idDestDe(nota);
  return (
    <Secao id="identificacao" numero="1" titulo="Identificação" descricao="Dados gerais da operação">
      <div className="grade">
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
    </Secao>
  );
}
