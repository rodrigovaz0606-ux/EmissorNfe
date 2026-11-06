import React from 'react';
import { Secao, Campo, Selecao, Alternador } from './campos.jsx';
import { Endereco } from './Endereco.jsx';
import { IND_IE_DEST } from '../opcoes.js';
import { cpfValido, cnpjValido, digitos, destDoCliente } from '../modelo.js';

// Aplica a máscara por grupos: CPF 000.000.000-00, CNPJ 00.000.000/0000-00.
export function mascara(v, tipo) {
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

// Campos do destinatário: usados na nota e no cadastro de clientes.
// `s(campo, valor)` altera um campo.
export function CamposDestinatario({ d, s, homologacao }) {
  const pf = d.tipo === 'PF';
  const docValor = mascara(pf ? d.CPF : d.CNPJ, d.tipo);
  const docOk = pf ? cpfValido(d.CPF) : cnpjValido(d.CNPJ);
  return (
    <>
      <div className="grade">
        <div className="campo l-p">
          <span className="rotulo">Pessoa</span>
          <Alternador valor={d.tipo} onChange={(v) => s('tipo', v)} opcoes={[['PF', 'Física'], ['PJ', 'Jurídica']]} />
        </div>
        <Campo
          rotulo={pf ? 'CPF' : 'CNPJ'} valor={docValor} largura="p" inputMode="numeric" obrigatorio
          onChange={(v) => s(pf ? 'CPF' : 'CNPJ', mascara(v, d.tipo))}
          erro={!docOk && `${pf ? 'CPF' : 'CNPJ'} inválido`}
          dica={docOk ? '✓ Documento válido' : undefined}
        />
        <Campo
          rotulo="Nome / Razão social" valor={d.xNome} onChange={(v) => s('xNome', v.toUpperCase())} largura="g" obrigatorio maxLength={60}
          erro={!d.xNome.trim() && 'Informe o nome'}
          dica={homologacao ? 'Em homologação a SEFAZ exige um texto padrão no lugar do nome; ele é trocado automaticamente.' : undefined}
        />
        <Selecao rotulo="Indicador de IE" valor={d.indIEDest} onChange={(v) => s('indIEDest', v)} opcoes={IND_IE_DEST} largura="m" />
        {d.indIEDest === '1' && (
          <Campo rotulo="Inscrição estadual" valor={d.IE} onChange={(v) => s('IE', v)} largura="p" inputMode="numeric" obrigatorio erro={!digitos(d.IE) && 'Informe a IE'} />
        )}
        <Campo rotulo="E-mail" valor={d.email} onChange={(v) => s('email', v)} largura="m" type="email" dica="Opcional" />
      </div>
      <h3 className="subtitulo">Endereço</h3>
      <Endereco valor={d.ender} onChange={(v) => s('ender', v)} />
    </>
  );
}

function EscolherCliente({ clientes, onEscolher }) {
  const id = React.useId();
  if (!clientes) return null;
  return (
    <div className="campo escolher-produto">
      <label className="rotulo" htmlFor={id}>Cliente cadastrado</label>
      <select
        id={id} value="" disabled={clientes.length === 0}
        onChange={(e) => { const c = clientes.find((x) => String(x.id) === e.target.value); if (c) onEscolher(c); }}
      >
        <option value="">{clientes.length === 0 ? 'Nenhum cliente cadastrado' : 'Escolher da lista…'}</option>
        {clientes.map((c) => <option key={c.id} value={c.id}>{c.xNome} ({mascara(c.CPF || c.CNPJ, c.tipo)})</option>)}
      </select>
    </div>
  );
}

export function Destinatario({ nota, set, homologacao, clientes, onSalvarCliente }) {
  const d = nota.dest;
  const [aviso, setAviso] = React.useState(null);
  const [salvando, setSalvando] = React.useState(false);

  async function salvar() {
    setSalvando(true);
    setAviso(null);
    try {
      setAviso({ ok: true, texto: await onSalvarCliente(d) });
    } catch (e) {
      setAviso({ ok: false, texto: e.message });
    } finally {
      setSalvando(false);
    }
  }

  return (
    <Secao
      id="destinatario" numero="3" titulo="Destinatário" descricao="Quem recebe a mercadoria"
      acoes={<EscolherCliente clientes={clientes} onEscolher={(c) => { setAviso(null); set(['dest'], destDoCliente(c)); }} />}
    >
      <CamposDestinatario d={d} s={(campo, v) => set(['dest', campo], v)} homologacao={homologacao} />
      {aviso && <div className={`aviso-local ${aviso.ok ? 'ok' : 'falha'}`}>{aviso.texto}</div>}
      <div className="item-rodape">
        <button type="button" className="btn-texto" onClick={salvar} disabled={salvando}>
          {salvando ? 'Salvando…' : 'Salvar no cadastro de clientes'}
        </button>
      </div>
    </Secao>
  );
}
