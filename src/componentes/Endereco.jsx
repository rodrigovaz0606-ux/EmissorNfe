import React from 'react';
import { Campo, Selecao } from './campos.jsx';
import { UFS } from '../opcoes.js';
import { buscarCep } from '../api.js';
import { digitos } from '../modelo.js';

export function Endereco({ valor, onChange, cepObrigatorio }) {
  const [buscando, setBuscando] = React.useState(false);
  const [erroCep, setErroCep] = React.useState('');
  const set = (campo) => (v) => onChange({ ...valor, [campo]: v });

  async function consultarCep() {
    setErroCep('');
    setBuscando(true);
    try {
      const r = await buscarCep(valor.CEP);
      onChange({ ...valor, ...Object.fromEntries(Object.entries(r).filter(([, v]) => v)) });
    } catch (e) {
      setErroCep(e.message);
    } finally {
      setBuscando(false);
    }
  }

  return (
    <div className="grade">
      <div className="cep-grupo l-p">
        <Campo
          rotulo="CEP" valor={valor.CEP} onChange={set('CEP')} largura="cheia" inputMode="numeric" placeholder="00000-000"
          obrigatorio={cepObrigatorio} erro={cepObrigatorio && digitos(valor.CEP).length !== 8 && 'Informe o CEP'}
          onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), consultarCep())}
        />
        <button type="button" className="btn-mini" onClick={consultarCep} disabled={buscando || digitos(valor.CEP).length !== 8}>
          {buscando ? '…' : 'Buscar'}
        </button>
        {erroCep && <span className="msg-erro">{erroCep}</span>}
      </div>
      <Campo rotulo="Logradouro" valor={valor.xLgr} onChange={set('xLgr')} largura="g" obrigatorio erro={!valor.xLgr.trim() && 'Informe o logradouro'} />
      <Campo rotulo="Número" valor={valor.nro} onChange={set('nro')} largura="pp" placeholder="SN" />
      <Campo rotulo="Complemento" valor={valor.xCpl} onChange={set('xCpl')} largura="m" />
      <Campo rotulo="Bairro" valor={valor.xBairro} onChange={set('xBairro')} largura="m" obrigatorio erro={!valor.xBairro.trim() && 'Informe o bairro'} />
      <Campo rotulo="Município" valor={valor.xMun} onChange={set('xMun')} largura="m" obrigatorio erro={!valor.xMun.trim() && 'Informe o município'} />
      <Campo
        rotulo="Cód. IBGE" valor={valor.cMun} onChange={set('cMun')} largura="p" inputMode="numeric" obrigatorio
        erro={!/^\d{7}$/.test(digitos(valor.cMun)) && '7 dígitos'} dica="Preenchido pelo CEP"
      />
      <Selecao rotulo="UF" valor={valor.UF} onChange={set('UF')} opcoes={UFS} largura="pp" />
      <Campo rotulo="Telefone" valor={valor.fone} onChange={set('fone')} largura="p" inputMode="tel" />
    </div>
  );
}
