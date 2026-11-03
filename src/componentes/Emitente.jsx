import React from 'react';
import { Secao, Campo, Selecao } from './campos.jsx';
import { Endereco } from './Endereco.jsx';
import { CRT } from '../opcoes.js';
import { digitos } from '../modelo.js';

const formatarDoc = (d) =>
  d.length === 11 ? d.replace(/(\d{3})(\d{3})(\d{3})(\d{2})/, '$1.$2.$3-$4')
    : d.length === 14 ? d.replace(/(\d{2})(\d{3})(\d{3})(\d{4})(\d{2})/, '$1.$2.$3/$4-$5') : d;

export function Emitente({ nota, set, certificado, onSalvarPadrao, salvo }) {
  const e = nota.emit;
  const s = (campo) => (v) => set(['emit', campo], v);
  const doc = digitos(e.CNPJ || e.CPF);
  return (
    <Secao
      id="emitente" numero="2" titulo="Emitente" descricao="Quem emite a nota. O documento vem do certificado digital."
      acoes={<button type="button" className="btn-texto" onClick={onSalvarPadrao}>{salvo ? '✓ Salvo como padrão' : 'Salvar como padrão'}</button>}
    >
      <div className="grade">
        <div className="campo l-p">
          <span className="rotulo">{doc.length === 14 ? 'CNPJ' : 'CPF'}</span>
          <div className="valor-fixo" title="Definido pelo certificado digital">🔒 {formatarDoc(doc) || '—'}</div>
          <span className="dica">{certificado ? `Certificado ${certificado.tipo}` : 'Carregando certificado…'}</span>
        </div>
        <Campo rotulo="Nome / Razão social" valor={e.xNome} onChange={(v) => s('xNome')(v.toUpperCase())} largura="g" obrigatorio erro={!e.xNome.trim() && 'Informe o nome'} />
        <Campo rotulo="Inscrição estadual" valor={e.IE} onChange={s('IE')} largura="p" inputMode="numeric" obrigatorio erro={!digitos(e.IE) && 'Informe a IE'} />
        <Selecao rotulo="Regime tributário" valor={e.CRT} onChange={s('CRT')} opcoes={CRT} largura="m" />
      </div>
      <h3 className="subtitulo">Endereço</h3>
      <Endereco valor={e.ender} onChange={(v) => set(['emit', 'ender'], v)} cepObrigatorio />
    </Secao>
  );
}
