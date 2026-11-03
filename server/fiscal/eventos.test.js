import { test } from 'node:test';
import assert from 'node:assert/strict';
import { prepararEvento, montarEvento, lerRetEnvEvento, montarProcEvento, COND_USO_CCE } from './eventos.js';
import { prepararInutilizacao, montarInutNFe, lerRetInutNFe, montarProcInut } from './inutilizacao.js';
import { montarConsSitNFe, lerRetConsSitNFe } from './consulta.js';
import { validarXsd } from './nfe/validarXsd.js';
import { verificarAssinatura } from './nfe/assinar.js';
import { certificadoDeTeste } from './certificadoTeste.js';

const config = { cUF: 52, tpAmb: 2 };
const CHAVE = '52261112345678000195550010000000011482017365'; // fictícia
const cert = certificadoDeTeste();
const DH = '2026-11-03T10:00:00-03:00';

test('cancelamento: monta, assina e passa no XSD oficial', async () => {
  const { evento, envEvento, schema } = prepararEvento('cancelamento', {
    chave: CHAVE, documento: '12345678000195', nProt: '152260000000001', xJust: 'Erro na digitacao do valor', dhEvento: DH,
  }, config, cert);
  assert.match(evento, new RegExp(`<infEvento Id="ID110111${CHAVE}01">`));
  assert.equal(verificarAssinatura(evento).valida, true);
  assert.equal(verificarAssinatura(envEvento).valida, true); // continua válida dentro do lote
  const { valido, erros } = await validarXsd(envEvento, schema);
  assert.ok(valido, erros.join('\n'));
});

test('CC-e: monta, assina e passa no XSD oficial com o texto de condição de uso', async () => {
  const { envEvento, schema } = prepararEvento('cce', {
    chave: CHAVE, documento: '52998224725', nSeqEvento: 2, xCorrecao: 'Corrigir o endereco do destinatario para Rua 10', dhEvento: DH,
  }, config, cert);
  assert.match(envEvento, /<CPF>52998224725<\/CPF>/);
  assert.match(envEvento, new RegExp(`Id="ID110110${CHAVE}02"`));
  assert.ok(envEvento.includes(`<xCondUso>${COND_USO_CCE}</xCondUso>`));
  const { valido, erros } = await validarXsd(envEvento, schema);
  assert.ok(valido, erros.join('\n'));
});

test('eventos: validações de entrada', () => {
  const base = { chave: CHAVE, documento: '12345678000195' };
  assert.throws(() => montarEvento('cancelamento', { ...base, nProt: '1', xJust: 'justificativa longa ok' }, config), /protocolo/);
  assert.throws(() => montarEvento('cancelamento', { ...base, nProt: '152260000000001', xJust: 'curta' }, config), /15 a 255/);
  assert.throws(() => montarEvento('cce', { ...base, xCorrecao: 'curta' }, config), /15 a 1000/);
  assert.throws(() => montarEvento('cce', { ...base, chave: '123', xCorrecao: 'texto suficiente aqui' }, config), /Chave/);
  assert.throws(() => montarEvento('cce', { ...base, nSeqEvento: 21, xCorrecao: 'texto suficiente aqui' }, config), /nSeqEvento/);
  assert.throws(() => montarEvento('cce', { ...base, xCorrecao: 'texto suficiente aqui' }, { cUF: 52, tpAmb: 1 }), /homologação/);
});

test('lê retorno de evento e monta procEventoNFe', () => {
  const resp =
    '<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope"><soap:Body><nfeResultMsg>' +
    '<retEnvEvento versao="1.00" xmlns="http://www.portalfiscal.inf.br/nfe"><idLote>1</idLote><tpAmb>2</tpAmb><verAplic>GO</verAplic>' +
    '<cOrgao>52</cOrgao><cStat>128</cStat><xMotivo>Lote de Evento Processado</xMotivo>' +
    '<retEvento versao="1.00"><infEvento><tpAmb>2</tpAmb><verAplic>GO</verAplic><cOrgao>52</cOrgao><cStat>135</cStat>' +
    `<xMotivo>Evento registrado e vinculado a NF-e</xMotivo><chNFe>${CHAVE}</chNFe><tpEvento>110111</tpEvento>` +
    '<xEvento>Cancelamento</xEvento><nSeqEvento>1</nSeqEvento><dhRegEvento>2026-11-03T10:00:01-03:00</dhRegEvento>' +
    '<nProt>152260000000099</nProt></infEvento></retEvento></retEnvEvento></nfeResultMsg></soap:Body></soap:Envelope>';
  const r = lerRetEnvEvento(resp);
  assert.equal(r.lote.cStat, '128');
  assert.equal(r.evento.cStat, '135');
  assert.equal(r.evento.registrado, true);
  assert.equal(r.evento.nProt, '152260000000099');
  const proc = montarProcEvento('<evento versao="1.00" xmlns="http://www.portalfiscal.inf.br/nfe"><infEvento/></evento>', r.xmlRetEvento);
  assert.ok(proc.includes('<procEventoNFe versao="1.00" xmlns="http://www.portalfiscal.inf.br/nfe"><evento versao="1.00"><infEvento/></evento><retEvento versao="1.00">'));
});

test('inutilização: monta, assina e passa no XSD; exige CNPJ', async () => {
  const d = { cnpj: '12.345.678/0001-95', serie: 1, nNFIni: 10, nNFFin: 12, xJust: 'Falha no sistema pulou a numeracao', ano: 2026 };
  const xml = prepararInutilizacao(d, config, cert);
  assert.ok(xml.includes('<infInut Id="ID52261234567800019555001000000010000000012">'));
  assert.equal(verificarAssinatura(xml).valida, true);
  const { valido, erros } = await validarXsd(xml, 'inutilizacao');
  assert.ok(valido, erros.join('\n'));
  assert.throws(() => montarInutNFe({ ...d, cnpj: '52998224725' }, config), /CNPJ/);
  assert.throws(() => montarInutNFe({ ...d, nNFFin: 5 }, config), /Faixa/);

  const ret = lerRetInutNFe(
    '<x><retInutNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><infInut><tpAmb>2</tpAmb><cStat>102</cStat>' +
    '<xMotivo>Inutilizacao de numero homologado</xMotivo><nProt>152260000000123</nProt></infInut></retInutNFe></x>',
  );
  assert.equal(ret.homologada, true);
  const proc = montarProcInut(xml, ret.xmlRet);
  assert.ok(proc.includes('<ProcInutNFe versao="4.00"'));
  assert.ok(proc.endsWith('</retInutNFe></ProcInutNFe>'));
});

test('consulta de situação: monta e lê', () => {
  assert.equal(
    montarConsSitNFe(CHAVE, 2),
    `<consSitNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><xServ>CONSULTAR</xServ><chNFe>${CHAVE}</chNFe></consSitNFe>`,
  );
  const r = lerRetConsSitNFe(
    '<x><retConsSitNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><cStat>217</cStat>' +
    `<xMotivo>Rejeicao: NF-e nao consta na base de dados da SEFAZ</xMotivo><cUF>52</cUF><chNFe>${CHAVE}</chNFe></retConsSitNFe></x>`,
  );
  assert.equal(r.cStat, '217');
  assert.equal(r.protocolo, null);
});
