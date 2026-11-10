import React from 'react';
import { Secao, Campo, Selecao } from './campos.jsx';
import { MOD_FRETE, T_PAG, TIPOS_GUIA, UFS } from '../opcoes.js';
import { moeda, num, digitos, guiaVazia, temAnimalVivo, guiaPreenchida } from '../modelo.js';

// Guia de trânsito (NT 2024.003): aparece quando a nota tem animal vivo ou já tem guia preenchida.
function GuiaTransito({ nota, set }) {
  const g = nota.guia ?? guiaVazia();
  const s = (campo) => (v) => set(['guia'], { ...g, [campo]: v });
  return (
    <div className="bloco-guia">
      <h3 className="subtitulo">Guia de trânsito</h3>
      <p className="dica">
        Animal vivo exige a GTA emitida pela Agrodefesa. A SEFAZ confere se a guia existe e se já foi usada em outra nota.
      </p>
      <div className="grade">
        <Selecao rotulo="Tipo de guia" valor={g.tpGuia} onChange={s('tpGuia')} opcoes={TIPOS_GUIA} largura="g" />
        <Selecao rotulo="UF da guia" valor={g.UFGuia} onChange={s('UFGuia')} opcoes={UFS} largura="pp" />
        <Campo rotulo="Série" valor={g.serieGuia} onChange={s('serieGuia')} largura="pp" maxLength={9} dica="Se a guia tiver" />
        <Campo
          rotulo="Número da guia" valor={g.nGuia} onChange={(v) => s('nGuia')(digitos(v).slice(0, 9))} largura="pp" inputMode="numeric" obrigatorio
          erro={!digitos(g.nGuia) && 'Informe o número'}
        />
      </div>
    </div>
  );
}

export function Pagamento({ nota, set, vNF }) {
  const pag = nota.pag;
  const unico = pag.length === 1;
  const atualizar = (i, campo) => (v) => set(['pag'], pag.map((p, j) => (j === i ? { ...p, [campo]: v } : p)));

  return (
    <Secao id="pagamento" numero="5" titulo="Pagamento e transporte">
      <div className="pagamentos">
        {pag.map((p, i) => (
          <div className="grade linha-pagamento" key={i}>
            <Selecao rotulo={unico ? 'Forma de pagamento' : `Forma ${i + 1}`} valor={p.tPag} onChange={atualizar(i, 'tPag')} opcoes={T_PAG} largura="m" />
            {p.tPag === '90' ? (
              <div className="campo l-p"><span className="rotulo">Valor</span><div className="valor-fixo">R$ 0,00</div></div>
            ) : (
              <Campo
                rotulo="Valor" valor={p.vPag} onChange={atualizar(i, 'vPag')} largura="p" inputMode="decimal"
                placeholder={unico ? moeda(vNF) : '0,00'} dica={unico && p.vPag === '' ? 'Igual ao total da nota' : undefined}
                erro={p.vPag !== '' && !Number.isFinite(num(p.vPag)) ? 'Número inválido' : !unico && p.vPag === '' ? 'Informe o valor' : undefined}
              />
            )}
            {!unico && (
              <button type="button" className="btn-icone" title="Remover" onClick={() => set(['pag'], pag.filter((_, j) => j !== i))}>✕</button>
            )}
          </div>
        ))}
        <button type="button" className="btn-texto" onClick={() => set(['pag'], [...pag, { tPag: '17', vPag: '' }])}>+ Outra forma de pagamento</button>
      </div>

      <div className="grade">
        <Selecao rotulo="Modalidade do frete" valor={nota.transp.modFrete} onChange={(v) => set(['transp', 'modFrete'], v)} opcoes={MOD_FRETE} largura="m" />
      </div>

      {(temAnimalVivo(nota) || guiaPreenchida(nota)) && <GuiaTransito nota={nota} set={set} />}

      <h3 className="subtitulo">Informações complementares</h3>
      <textarea
        className="texto-livre" rows={3} value={nota.infAdic.infCpl} maxLength={5000}
        onChange={(e) => set(['infAdic', 'infCpl'], e.target.value)}
        placeholder="Texto que aparece no campo de informações complementares do DANFE"
      />
    </Secao>
  );
}
