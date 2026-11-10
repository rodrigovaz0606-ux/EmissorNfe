"""Compara o validador XSD do emissor com a libxml2 (via lxml) no schema oficial da NF-e.

Uso: python ferramentas/comparar_xsd.py <pasta-de-trabalho>
1. Gera amostras assinadas pelo emissor (certificado fictício dos testes).
2. Cria variações adulteradas de cada amostra (elementos removidos, duplicados, valores inválidos,
   ordem trocada).
3. Valida tudo nos dois validadores e lista onde os veredictos divergem.
Só a validação de estrutura é comparada: a assinatura não é conferida pela libxml2.
"""
import copy
import os
import subprocess
import sys
from pathlib import Path

from lxml import etree

RAIZ = Path(__file__).resolve().parent.parent
SCHEMA = RAIZ / 'schemas' / 'PL_010g_v1.03' / 'NFe' / 'nfe_v4.00.xsd'
NS = '{http://www.portalfiscal.inf.br/nfe}'


def rodar_teste(pasta, fase):
    env = dict(os.environ, COMPARAR_XSD_DIR=str(pasta), COMPARAR_XSD_FASE=fase)
    subprocess.run(['cargo', 'test', '-q', '--manifest-path', str(RAIZ / 'backend' / 'Cargo.toml'), '--test', 'nfe',
                    'comparacao_xsd_com_libxml2'], env=env, check=True, capture_output=True)


def variacoes(arvore):
    """Gera (nome, árvore) adulteradas a partir de uma nota válida."""
    raiz = arvore.getroot()
    inf = raiz.find(f'{NS}infNFe')
    elementos = [e for e in inf.iter() if isinstance(e.tag, str) and e is not inf]
    valores_ruins = ['', 'X' * 70, '-1', '1.2.3', 'ABC', '99999999999999999999', ' 1', '0']
    for i, e in enumerate(elementos):
        nome = etree.QName(e).localname
        # remover
        a = copy.deepcopy(arvore)
        alvo = [x for x in a.getroot().find(f'{NS}infNFe').iter() if isinstance(x.tag, str)][i + 1]
        alvo.getparent().remove(alvo)
        yield f'rem_{i}_{nome}', a
        # duplicar
        a = copy.deepcopy(arvore)
        alvo = [x for x in a.getroot().find(f'{NS}infNFe').iter() if isinstance(x.tag, str)][i + 1]
        alvo.addnext(copy.deepcopy(alvo))
        yield f'dup_{i}_{nome}', a
        # valores inválidos nas folhas
        if len(e) == 0:
            for j, v in enumerate(valores_ruins):
                a = copy.deepcopy(arvore)
                alvo = [x for x in a.getroot().find(f'{NS}infNFe').iter() if isinstance(x.tag, str)][i + 1]
                alvo.text = v
                yield f'val_{i}_{nome}_{j}', a
    # grupo agropecuario fora do lugar (antes de infAdic)
    agro = inf.find(f'{NS}agropecuario')
    if agro is not None:
        a = copy.deepcopy(arvore)
        inf2 = a.getroot().find(f'{NS}infNFe')
        g = inf2.find(f'{NS}agropecuario')
        inf2.remove(g)
        inf2.find(f'{NS}infAdic').addprevious(g)
        yield 'agro_fora_de_ordem', a


def main():
    pasta = Path(sys.argv[1]).resolve()
    pasta.mkdir(parents=True, exist_ok=True)
    for f in pasta.glob('*.xml'):
        f.unlink()
    rodar_teste(pasta, 'gerar')
    bases = sorted(pasta.glob('base_*.xml'))
    for b in bases:
        arvore = etree.parse(str(b))
        for nome, a in variacoes(arvore):
            a.write(str(pasta / f'{b.stem}__{nome}.xml'), xml_declaration=False, encoding='utf-8')
    rodar_teste(pasta, 'validar')

    schema = etree.XMLSchema(etree.parse(str(SCHEMA)))
    emissor = dict(l.split('\t') for l in (pasta / 'emissor.tsv').read_text(encoding='utf-8').splitlines())
    divergentes, total, validos = [], 0, 0
    for arq in sorted(pasta.glob('*.xml')):
        total += 1
        lib = schema.validate(etree.parse(str(arq)))
        nosso = emissor[arq.name] == 'true'
        validos += lib
        if lib != nosso:
            erro = '' if lib else str(schema.error_log.last_error)[:160]
            divergentes.append(f'{arq.name}: libxml2={lib} emissor={nosso} {erro}')
    print(f'arquivos: {total} | válidos pela libxml2: {validos} | divergências: {len(divergentes)}')
    for d in divergentes:
        print('  ' + d)


if __name__ == '__main__':
    main()
