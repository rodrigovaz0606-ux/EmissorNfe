# Emissor NF-e

NF-e modelo 55 emitida direto na SEFAZ, com Node. Usa a mesma stack do CampoCerto/NfeConsulta para poder ser incorporado depois.

**Só opera em homologação.** O ambiente de produção está bloqueado em `server/fiscal/config.js`.

## Configuração

1. Copie `.env.example` para `.env` (se ainda não existir).
2. Em `CERT_PFX_PATH`, informe o caminho do seu certificado A1 (`.pfx`). Em `CERT_PFX_SENHA`, a senha.
3. Rode:
   ```bash
   npm install
   npm run cert     # abre o .pfx e mostra titular/validade (não usa a rede)
   npm run status   # consulta NFeStatusServico4 na SEFAZ-GO (homologação)
   npm test
   ```

Resposta esperada do `status`: `cStat 107: Servico em Operacao`.

## Roteiro

- [x] Fase 1: leitura do certificado A1, cliente SOAP com TLS mútuo, status do serviço
- [ ] Fase 2: XML da NF-e 4.00, chave de acesso, validação XSD, assinatura XMLDSig
- [ ] Fase 3: autorização (NFeAutorizacao4), `nfeProc`, SQLite
- [ ] Fase 4: cancelamento, CC-e, inutilização, DANFE, telas em React
