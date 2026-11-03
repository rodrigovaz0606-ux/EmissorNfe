# Emissor NF-e

NF-e modelo 55 emitida direto na SEFAZ, com Node. Usa a mesma stack do CampoCerto/NfeConsulta para poder ser incorporado depois.

**Só opera em homologação.** O ambiente de produção está bloqueado em `server/fiscal/config.js`.

## Interface web

```bash
npm run dev
```

Abra http://localhost:5175. A API fica em http://localhost:3335 e só aceita conexões da própria máquina, porque é ela que assina as notas com o certificado.

- Formulário com identificação, emitente, destinatário, produtos (com ICMS/PIS/COFINS), pagamento e informações complementares
- Totais e impostos calculados enquanto você digita, com as mesmas funções que geram o XML
- Lista de pendências e avisos (por exemplo, CFOP incompatível com o destino da operação)
- Busca de endereço pelo CEP (ViaCEP), que também preenche o código IBGE do município
- **Validar**: gera, assina e confere o XML no XSD, sem enviar. **Emitir**: envia para a SEFAZ em homologação
- O rascunho e o emitente padrão ficam salvos no navegador (localStorage)

## Configuração

1. Copie `.env.example` para `.env` (se ainda não existir).
2. Em `CERT_PFX_PATH`, informe o caminho do seu certificado A1 (`.pfx`). Em `CERT_PFX_SENHA`, a senha.
3. Rode:
   ```bash
   npm install
   npm run cert     # abre o .pfx e mostra titular/validade (não usa a rede)
   npm run status   # consulta NFeStatusServico4 na SEFAZ-GO (homologação)
   npm run gerar    # gera, assina e valida o XML do cenário de exemplo (venda de soja) em saida/
   npm run validar -- saida/<arquivo>.xml   # valida no XSD
   npm run enviar   # gera, assina, valida e envia para autorização (homologação)
   npm test
   ```

Resposta esperada do `status`: `cStat 107: Servico em Operacao`.

## Testes

```bash
npm test       # unidade + integração da API (banco em memória, SEFAZ falsa): rápido, não usa rede
npm run e2e    # interface no Microsoft Edge com a SEFAZ SIMULADA (data/e2e.sqlite)
```

O `e2e` abre o Edge instalado em modo headless e clica na interface: preenche a nota, busca o CEP, valida, emite, faz carta de correção e cancela. Nada é enviado à SEFAZ.

## Modo SEFAZ simulada

Para treinar o uso ou demonstrar a interface sem enviar nada:

```bash
# PowerShell
$env:NFE_SEFAZ_SIMULADA="1"; $env:NFE_DB="data/simulado.sqlite"; npm run dev
```

Uma faixa amarela fica visível o tempo todo. As "autorizações" são fictícias e ficam num banco separado.

## Comandos de linha

| Comando | O que faz |
|---|---|
| `npm run status` | Status da SEFAZ (homologação) |
| `npm run consultar -- <chave>` | Situação de uma NF-e (somente leitura) |
| `npm run gerar` | Gera, assina e valida o XML de exemplo em `saida/` |
| `npm run enviar` | Envia o exemplo para autorização (homologação) |

## Roteiro

- [x] Fase 1: leitura do certificado A1, cliente SOAP com TLS mútuo, status do serviço
- [x] Fase 2: chave de acesso, XML da NF-e 4.00, validação XSD, assinatura XMLDSig
- [x] Fase 3: autorização (NFeAutorizacao4), `nfeProc`, SQLite, numeração
- [x] Fase 4: cancelamento, CC-e, inutilização, consulta, DANFE em PDF, telas em React
- [ ] Autorização real (cStat 100): precisa de um e-CNPJ com IE em GO (a SEFAZ-GO rejeita emitente CPF com 407)
- [ ] Atualizar schemas para o PL_010 (reforma tributária: IBS/CBS)
