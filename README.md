# Emissor NF-e

NF-e modelo 55 emitida direto na SEFAZ. **Backend em Rust** (`backend/`), interface em React (`src/`).

**Só opera em homologação.** O ambiente de produção está bloqueado em `backend/src/fiscal/config.rs`.

## Requisitos

- Rust (stable) com o toolchain MSVC no Windows
- Node 18+ (só para a interface e os testes no navegador)
- Certificado A1 (`.pfx`)

## Interface web

```bash
npm run dev
```

Abra http://localhost:5175. A API (Rust) fica em http://localhost:3335 e só aceita conexões da própria máquina, porque é ela que assina as notas com o certificado. A primeira execução compila o backend e leva alguns minutos; as seguintes são rápidas.

- Formulário com identificação, emitente, destinatário, produtos (com ICMS/PIS/COFINS), pagamento e informações complementares
- Totais e impostos calculados enquanto você digita, com o mesmo arredondamento que gera o XML
- Lista de pendências e avisos (por exemplo, CFOP incompatível com o destino da operação)
- Busca de endereço pelo CEP (ViaCEP), que também preenche o código IBGE do município
- **Validar**: gera, assina e confere o XML no XSD, sem enviar. **Emitir**: envia para a SEFAZ em homologação
- **Produtos**: cadastro (código, descrição, NCM, CFOP, unidade, preço e tributação) salvo no SQLite; na nota, escolha o produto em "Produto cadastrado" para preencher o item, ou salve um item digitado com "Salvar no cadastro de produtos"
- **Tema claro e escuro** (botão no topo e na tela de login). Sem escolha salva, segue o sistema. As animações são desligadas para quem ativa "reduzir movimento" no sistema
- **Estoque** (opcional por produto): saldo, estoque mínimo e histórico. Entrada, saída e ajuste de contagem são manuais; nota autorizada baixa (saída) ou aumenta (entrada) o saldo, e o cancelamento devolve. Falta de saldo só gera aviso, não impede a emissão. Item com unidade diferente da do cadastro não mexe no saldo e é avisado
- **Clientes**: cadastro de destinatários; na nota, escolha em "Cliente cadastrado" ou salve o destinatário digitado
- **Tipo de nota**: modelos de venda, remessa, devolução e entrada que preenchem natureza, CFOP e pagamento (confira com o contador se servem para a sua operação). Devolução e complementar pedem a chave da NF-e referenciada
- **Painel**: total autorizado no mês, notas por situação, valor dos últimos 6 meses e certificados que vencem em até 30 dias
- **Duplicar nota**: abre uma nota nova com os dados de uma antiga e o próximo número livre
- **Filtros** em Notas emitidas (período, situação, número, destinatário, CPF/CNPJ ou chave)
- **Exportar para o contador**: ZIP com os XMLs do período (notas autorizadas, canceladas e denegadas, eventos e inutilizações) e um `resumo.csv` que abre no Excel
- Notas emitidas: DANFE em PDF, XML, carta de correção, cancelamento, consulta e inutilização
- O rascunho e o emitente padrão ficam salvos no navegador (localStorage)

Para usar sem o Vite (interface já compilada servida pelo próprio backend): `npm run build` e depois `npm start`, em http://localhost:3335.

## Login, produtores e administrador

- **Primeiro acesso:** com o banco sem usuários, a tela inicial pede para criar o **administrador**. O administrador cria os outros usuários na aba *Administração* (criar, redefinir senha, desativar e dar acesso de administrador).
- **Produtores:** cada usuário cadastra os produtores dele enviando o certificado A1 (.pfx e senha) na aba *Produtores*. O produtor é identificado pelo CPF/CNPJ do certificado. Ali também dá para **trocar** de produtor, **renovar** o certificado (precisa ser do mesmo documento) e **remover** (o certificado é apagado e as notas ficam no histórico). O administrador vê os produtores de todos os usuários.
- Notas, numeração, inutilizações, produtos e o emitente padrão ficam **separados por produtor**.
- **Segurança:** as senhas são guardadas como hash Argon2id. A sessão fica num cookie `HttpOnly` + `SameSite=Strict` e expira após 8 horas sem uso. Depois de 5 senhas erradas, o login fica bloqueado por 5 minutos. Pedidos de alteração vindos de outros sites são recusados. O .pfx e a senha de cada produtor são guardados **cifrados (AES-256-GCM)** com a chave-mestra, que fica em `NFE_CHAVE_MESTRA` ou no arquivo `chave-mestra.key`, ao lado do banco e criado na primeira execução. **Faça backup da chave junto com o banco:** sem ela, os certificados guardados não abrem.
- Bancos de antes do login são migrados automaticamente: ao cadastrar o produtor com o mesmo documento do emitente, as notas antigas passam a ser dele.

## Configuração

1. Copie `.env.example` para `.env` (se ainda não existir).
2. `CERT_PFX_PATH` e `CERT_PFX_SENHA` (certificado A1 e senha) só são usados pelos **comandos de linha** (`status`, `cert`, `gerar`, `enviar`, `consultar`). Na interface web, os certificados vêm do cadastro de produtores. Não precisa de aspas, mesmo com espaços no caminho.
3. Rode:
   ```bash
   npm install
   npm run cert     # abre o .pfx e mostra titular/validade (não usa a rede)
   npm run status   # consulta NFeStatusServico4 na SEFAZ-GO (homologação)
   npm test
   ```

Resposta esperada do `status`: `cStat 107: Serviço em operação`.

## Testes

```bash
npm test       # Rust (unidade + integração da API com banco em memória e SEFAZ falsa) + cálculos da tela
npm run e2e    # interface no Microsoft Edge com a SEFAZ SIMULADA (data/e2e.sqlite)
```

O `e2e` compila o backend em modo release, abre o Edge instalado em modo headless e clica na interface: preenche a nota, busca o CEP, valida, emite, faz carta de correção e cancela. Nada é enviado à SEFAZ.

Os certificados em `backend/tests/fixtures/` são **fictícios** (autoassinados, senha `teste`): `teste.pfx` (e-CNPJ 12.345.678/0001-95) e `teste-cpf.pfx` (e-CPF 529.982.247-25). Servem só para os testes.

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
| `npm run cert` | Dados do certificado (sem rede) |
| `npm run status` | Status da SEFAZ (homologação) |
| `npm run consultar -- <chave>` | Situação de uma NF-e (somente leitura) |
| `npm run gerar` | Gera, assina e valida o XML de exemplo em `saida/` |
| `npm run validar -- <arquivo.xml>` | Valida um XML de NF-e no XSD |
| `npm run enviar` | Envia o exemplo para autorização (homologação) |

Os comandos chamam o binário `emissor-nfe` (`cargo run -- <comando>`); ele também pode ser usado direto depois de compilado.

## Como o backend está organizado

| Módulo | Responsabilidade |
|---|---|
| `fiscal/nfe.rs` | XML da NF-e 4.00 (leiaute, totais em centavos, ICMS/PIS/COFINS) |
| `fiscal/chave.rs` | Chave de acesso (44 dígitos, DV módulo 11) |
| `fiscal/assinatura.rs`, `fiscal/xml.rs` | XMLDSig (RSA-SHA1) com canonicalização C14N própria |
| `fiscal/xsd.rs` | Validador XSD próprio para o subconjunto usado pelos schemas da NF-e |
| `fiscal/pkcs12.rs`, `fiscal/certificado.rs` | Leitura do `.pfx` (aceita BER, 3DES, RC2-40 e AES, comuns na ICP-Brasil) |
| `fiscal/soap.rs`, `fiscal/servicos.rs` | SOAP 1.2 com TLS mútuo (Schannel no Windows) e os serviços da SEFAZ |
| `danfe/` | DANFE em PDF (Code-128, várias folhas, marcas d'água) |
| `db.rs`, `emissao.rs`, `servidor.rs` | SQLite (com migrações), regras de emissão/eventos e API HTTP (axum) |
| `auth.rs`, `contas.rs`, `cofre.rs` | Login (Argon2id, sessões, bloqueio), usuários, produtores e cofre AES-256-GCM dos certificados |
| `produtos.rs`, `clientes.rs` | Cadastros de produtos e clientes (validação; código ou documento único por produtor) |
| `estoque.rs` | Saldo por movimentos: manuais, notas autorizadas e estorno no cancelamento |
| `relatorios.rs` | Painel e exportação em ZIP para o contador |

O validador XSD foi comparado com a libxml2 em 1.283 documentos adulterados (elementos removidos, duplicados e valores inválidos). Os veredictos coincidiram em todos, exceto 3 casos de base64 inválido no certificado, em que o validador próprio é mais rígido. As assinaturas também foram conferidas por outra implementação (xml-crypto).

## Roteiro

- [x] Fase 1: leitura do certificado A1, cliente SOAP com TLS mútuo, status do serviço
- [x] Fase 2: chave de acesso, XML da NF-e 4.00, validação XSD, assinatura XMLDSig
- [x] Fase 3: autorização (NFeAutorizacao4), `nfeProc`, SQLite, numeração
- [x] Fase 4: cancelamento, CC-e, inutilização, consulta, DANFE em PDF, telas em React
- [x] Backend reescrito em Rust (mesma API; bancos SQLite existentes continuam valendo)
- [x] Cadastro de produtos
- [x] Login, usuários, administrador e vários produtores (certificados cifrados)
- [x] Clientes, duplicar nota, filtros, exportação para o contador, painel e tipos de nota
- [x] Controle de estoque por produto
- [ ] Autorização real (cStat 100): precisa de um e-CNPJ com IE em GO (a SEFAZ-GO rejeita emitente CPF com 407)
- [ ] Atualizar schemas para o PL_010 (reforma tributária: IBS/CBS)
- [ ] Cadeia ICP-Brasil em `NFE_CA_BUNDLE` para verificar o certificado do servidor da SEFAZ
