import { test } from 'node:test';
import assert from 'node:assert/strict';
import { calcularDV, montarChave, validarChave, decomporChave, gerarCNF } from './chaveAcesso.js';

// Vetores calculados de forma independente (pesos 4329876543… da esquerda p/ direita).
// Documentos fictícios.
test('DV confere com cálculo independente', () => {
  assert.equal(calcularDV('5226111234567800019555001000000001148201736'), 5);
  assert.equal(calcularDV('5226110001234567890955001000000123190817263'), 2);
});

test('monta chave com CNPJ', () => {
  const { chave, cDV } = montarChave({
    cUF: 52, dhEmi: '2026-11-03T10:00:00-03:00', documento: '12.345.678/0001-95',
    serie: 1, nNF: 1, cNF: '48201736',
  });
  assert.equal(chave, '52261112345678000195550010000000011482017365');
  assert.equal(cDV, 5);
  assert.equal(chave.length, 44);
});

test('CPF do emitente é completado com zeros até 14 posições', () => {
  const { chave } = montarChave({
    cUF: 52, dhEmi: '2026-11-03T10:00:00-03:00', documento: '123.456.789-09',
    serie: 1, nNF: 123, cNF: '90817263',
  });
  assert.equal(chave, '52261100012345678909550010000001231908172632');
  assert.equal(decomporChave(chave).documento, '00012345678909');
});

test('AAMM vem do texto do dhEmi, sem conversão de fuso', () => {
  // 00:30 de 1º/11 em Brasília = 31/10 em fusos mais a oeste; a chave deve usar 2611.
  const { chave } = montarChave({
    cUF: 52, dhEmi: '2026-11-01T00:30:00-03:00', documento: '12345678000195', serie: 1, nNF: 1, cNF: '48201736',
  });
  assert.equal(chave.slice(2, 6), '2611');
});

test('validarChave detecta dígito errado', () => {
  assert.equal(validarChave('52261112345678000195550010000000011482017365'), true);
  assert.equal(validarChave('52261112345678000195550010000000011482017364'), false);
  assert.equal(validarChave('123'), false);
});

test('cNF sorteado tem 8 dígitos e nunca é igual ao nNF', () => {
  for (let i = 0; i < 2000; i++) {
    const c = gerarCNF(12345);
    assert.match(c, /^\d{8}$/);
    assert.notEqual(c, '00012345');
    assert.notEqual(c, '12345678');
  }
});

test('rejeita entradas inválidas', () => {
  const base = { cUF: 52, dhEmi: '2026-11-03', documento: '12345678000195', serie: 1, nNF: 1 };
  assert.throws(() => montarChave({ ...base, documento: '123' }), /CPF ou CNPJ/);
  assert.throws(() => montarChave({ ...base, nNF: 0 }), /nNF/);
  assert.throws(() => montarChave({ ...base, serie: 1000 }), /serie/);
  assert.throws(() => montarChave({ ...base, dhEmi: 'ontem' }), /dhEmi/);
});
