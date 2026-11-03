import React from 'react';
import { Secao, Campo, Selecao, Alternador } from './campos.jsx';
import { Endereco } from './Endereco.jsx';
import { IND_IE_DEST } from '../opcoes.js';
import { cpfValido, cnpjValido, digitos } from '../modelo.js';

// Aplica a máscara por grupos: CPF 000.000.000-00, CNPJ 00.000.000/0000-00.
function mascara(v, tipo) {
  const [grupos, seps] = tipo === 'PF' ? [[3, 3, 3, 2], ['.', '.', '-']] : [[2, 3, 3, 4, 2], ['.', '.', '/', '-']];
  const d = digitos(v).slice(0, grupos.reduce((a, b) => a + b, 0));
  let saida = '';
  let pos = 0;
  grupos.forEach((tam, i) => {
    const parte = d.slice(pos, pos + tam);
    if (!parte) return;
    saida += (i > 0 ? seps[i - 1] : '') + parte;
    pos += tam;
  });
  return saida;
}

export function Destinatario({ nota, set, homologacao }) {
  const d = nota.dest;
  const s = (campo) => (v) => set(['dest', campo], v);
  const pf = d.tipo === 'PF';
  const docValor = pf ? d.CPF : d.CNPJ;
  const docOk = pf ? cpfValido(d.CPF) : cnpjValido(d.CNPJ);
  return (
    <Secao id="destinatario" numero="3" titulo="Destinatário" descricao="Quem recebe a mercadoria">
      <div className="grade">
        <div className="campo l-p">
          <span className="rotulo">Pessoa</span>
          <Alternador valor={d.tipo} onChange={s('tipo')} opcoes={[['PF', 'Física'], ['PJ', 'Jurídica']]} />
        </div>
        <Campo
          rotulo={pf ? 'CPF' : 'CNPJ'} valor={docValor} largura="p" inputMode="numeric" obrigatorio
          onChange={(v) => s(pf ? 'CPF' : 'CNPJ')(mascara(v, d.tipo))}
          erro={!docOk && `${pf ? 'CPF' : 'CNPJ'} inválido`}
          dica={docOk ? '✓ Documento válido' : undefined}
        />
        <Campo
          rotulo="Nome / Razão social" valor={d.xNome} onChange={(v) => s('xNome')(v.toUpperCase())} largura="g" obrigatorio
          erro={!d.xNome.trim() && 'Informe o nome'}
          dica={homologacao ? 'Em homologação a SEFAZ exige um texto padrão no lugar do nome; ele é trocado automaticamente.' : undefined}
        />
        <Selecao rotulo="Indicador de IE" valor={d.indIEDest} onChange={s('indIEDest')} opcoes={IND_IE_DEST} largura="m" />
        {d.indIEDest === '1' && (
          <Campo rotulo="Inscrição estadual" valor={d.IE} onChange={s('IE')} largura="p" inputMode="numeric" obrigatorio erro={!digitos(d.IE) && 'Informe a IE'} />
        )}
        <Campo rotulo="E-mail" valor={d.email} onChange={s('email')} largura="m" type="email" dica="Opcional" />
      </div>
      <h3 className="subtitulo">Endereço</h3>
      <Endereco valor={d.ender} onChange={(v) => set(['dest', 'ender'], v)} />
    </Secao>
  );
}
