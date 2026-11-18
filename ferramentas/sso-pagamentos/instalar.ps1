param(
  # Teste neste PC: sem tarefa do Windows, sem túnel e sem Asaas (simulador de Pix). As entradas vêm
  # de variáveis de ambiente (TESTE_BANCO_PAGAMENTOS, TESTE_BANCO_SSO, TESTE_ADMIN_EMAIL).
  [switch]$Teste,
  [string]$Raiz = (Join-Path $env:ProgramData 'SistemasVaz'),
  [string]$Dominio = 'sistemasvaz.com.br'
)
# Instala (ou atualiza) o SaintSSO (login único e pacotes) e o GatewayRR (Pix pelo Asaas) neste
# servidor Windows, sem Docker: programas como tarefas do Windows e publicação pelo túnel da
# Cloudflare que o Campo Certo já usa. Segue o DEPLOY-INFRA.md do SaintSSO, etapas 2 a 5.1.
#
# Primeira vez: pede as conexões do Neon e a chave do Asaas (digitadas sem aparecer) e gera todos os
# outros segredos. Depois: só atualiza os programas, mantendo chaves e configuração.
$ErrorActionPreference = 'Stop'
$aqui = $PSScriptRoot
$portaSso = 8090
$portaGw = 8091
$pastaSso = Join-Path $Raiz 'sso'
$pastaGw = Join-Path $Raiz 'pagamentos'
$pastaConfig = Join-Path $Raiz 'config'      # segredos: só SYSTEM e administradores
$cfgGw = Join-Path $pastaConfig 'pagamentos.env'
$cfgGwDono = Join-Path $pastaConfig 'pagamentos-dono.env'
$cfgSso = Join-Path $pastaConfig 'sso.env'
$tarefaGw = 'SistemasVazPagamentos'
$tarefaSso = 'SistemasVazSSO'
$issuer = if ($Teste) { "http://localhost:$portaSso" } else { "https://sso.$Dominio" }
$ajudante = Join-Path $aqui 'ajudante-sso.exe'

if (-not $Teste) { Start-Transcript -LiteralPath (Join-Path $env:TEMP 'sistemasvaz-sso-instalacao.log') -Force | Out-Null }
function Passo($t) { Write-Host $t -ForegroundColor Cyan }

function LerEnv($arquivo) {
  $h = [ordered]@{}
  if (Test-Path -LiteralPath $arquivo) {
    foreach ($l in Get-Content -LiteralPath $arquivo) { if ($l -match '^\s*([A-Z][A-Z0-9_]*)\s*=(.*)$') { $h[$Matches[1]] = $Matches[2].Trim() } }
  }
  $h
}
function SalvarEnv($arquivo, $h) {
  $linhas = foreach ($k in $h.Keys) { "$k=$($h[$k])" }
  Set-Content -LiteralPath $arquivo -Value $linhas -Encoding ascii
}

# Roda o ajudante com variáveis só para ele (segredos nunca na linha de comando).
function Ajudante([string[]]$argumentos, $variaveis = @{}) {
  $antes = @{}
  foreach ($k in $variaveis.Keys) { $antes[$k] = [Environment]::GetEnvironmentVariable($k, 'Process'); [Environment]::SetEnvironmentVariable($k, $variaveis[$k], 'Process') }
  $ErrorActionPreference = 'Continue'  # stderr do programa não é erro; vale o código de saída
  $erros = [IO.Path]::GetTempFileName()
  try {
    $saida = & $ajudante @argumentos 2> $erros
    if ($LASTEXITCODE -ne 0) { throw "ajudante $($argumentos[0]) falhou: $((Get-Content -LiteralPath $erros) -join ' ')" }
    return ($saida | Out-String).Trim()
  } finally {
    Remove-Item -LiteralPath $erros -Force -ErrorAction SilentlyContinue
    foreach ($k in $variaveis.Keys) { [Environment]::SetEnvironmentVariable($k, $antes[$k], 'Process') }
  }
}

# Gateway: comandos com o papel dono do banco (migrate, credential).
function Gateway([string[]]$argumentos, $urlDono) {
  # Só a resposta (stdout); os avisos de log (stderr) só aparecem se der erro.
  $ErrorActionPreference = 'Continue'
  $antes = $env:DATABASE_MIGRATION_URL
  $env:DATABASE_MIGRATION_URL = $urlDono
  $erros = [IO.Path]::GetTempFileName()
  try {
    $saida = & (Join-Path $pastaGw 'gateway-api.exe') @argumentos 2> $erros
    if ($LASTEXITCODE -ne 0) { throw "gateway-api $($argumentos[0]) falhou: $((Get-Content -LiteralPath $erros) -join ' ')" }
    return ($saida | Out-String).Trim()
  } finally {
    $env:DATABASE_MIGRATION_URL = $antes
    Remove-Item -LiteralPath $erros -Force -ErrorAction SilentlyContinue
  }
}

# Conexão do Neon: a direta (sem -pooler), sempre com sslmode=verify-full, sem channel_binding.
function UrlBanco([string]$url) {
  $url = $url.Trim().Trim('"', "'")
  if ($url -notmatch '^postgres(ql)?://') { throw 'A conexão do banco precisa começar com postgresql:// (copie a "connection string" do Neon).' }
  $url = $url -replace '-pooler\.', '.'
  $partes = $url -split '\?', 2
  $params = @()
  if ($partes.Count -gt 1) { $params = $partes[1] -split '&' | Where-Object { $_ -and $_ -notmatch '^(sslmode|channel_binding)=' } }
  $params = @('sslmode=verify-full') + $params
  "$($partes[0])?$($params -join '&')"
}

function TrocarUsuario([string]$url, [string]$usuario, [string]$senha) { $url -replace '^(postgres(?:ql)?://)[^@/]+@', "`$1${usuario}:${senha}@" }

function Pedir([string]$texto, [string]$variavelTeste, [switch]$Secreto) {
  if ($Teste) {
    $v = [Environment]::GetEnvironmentVariable($variavelTeste)
    if (-not $v) { throw "Teste: defina $variavelTeste" }
    return $v
  }
  if ($Secreto) {
    $s = Read-Host -Prompt $texto -AsSecureString
    $p = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($s)
    try { return [Runtime.InteropServices.Marshal]::PtrToStringBSTR($p) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($p) }
  }
  Read-Host -Prompt $texto
}

function Parar($tarefa, $exe) {
  if (-not $Teste) { Stop-ScheduledTask -TaskName $tarefa -ErrorAction SilentlyContinue }
  Get-CimInstance Win32_Process -Filter "Name='$exe'" | Where-Object { $_.ExecutablePath -like "$Raiz*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

function Iniciar($tarefa, $exe, $config, $descricao) {
  $iniciador = Join-Path $Raiz 'iniciar-servico.ps1'
  $argumentos = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$iniciador`" -Exe `"$exe`" -Config `"$config`""
  if ($Teste) {
    $log = Join-Path $Raiz "$tarefa.log"
    Start-Process powershell.exe -ArgumentList $argumentos -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError "$log.erro" | Out-Null
    return
  }
  # Registro (log) de cada serviço em <Raiz>\logs, para investigar erros (sem chaves nem senhas).
  $pastaLogs = Join-Path $Raiz 'logs'
  New-Item -ItemType Directory -Force -Path $pastaLogs | Out-Null
  $argumentos += " -Log `"$(Join-Path $pastaLogs "$tarefa.log")`""
  if (Get-ScheduledTask -TaskName $tarefa -ErrorAction SilentlyContinue) { Unregister-ScheduledTask -TaskName $tarefa -Confirm:$false }
  $acao = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $argumentos -WorkingDirectory (Split-Path -Parent $exe)
  $conta = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
  $ajustes = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  Register-ScheduledTask -TaskName $tarefa -Description $descricao -Action $acao -Trigger (New-ScheduledTaskTrigger -AtStartup) -Principal $conta -Settings $ajustes | Out-Null
  Start-ScheduledTask -TaskName $tarefa
}

function Esperar($url, $nome) {
  for ($i = 0; $i -lt 60; $i++) {
    try { $r = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 3; if ($r.StatusCode -lt 300) { return } } catch {}
    Start-Sleep -Seconds 1
  }
  throw "$nome não respondeu em $url. Veja o Agendador de Tarefas e o log em $Raiz."
}

try {
  foreach ($f in @('saintsso.exe', 'gateway-api.exe', 'ajudante-sso.exe', 'iniciar-servico.ps1', 'public\index.html')) {
    if (-not (Test-Path -LiteralPath (Join-Path $aqui $f))) { throw "Pasta incompleta: falta $f. Extraia o ZIP inteiro e rode de novo." }
  }
  $configTunel = Join-Path $env:ProgramData 'CampoCerto\cloudflared\config.yml'
  if (-not $Teste -and -not (Test-Path -LiteralPath $configTunel)) { throw "Não achei o túnel do Campo Certo ($configTunel)." }
  $primeira = -not (Test-Path -LiteralPath $cfgSso)

  Passo '1/6 Copiando os programas...'
  Parar $tarefaSso 'saintsso.exe'
  Parar $tarefaGw 'gateway-api.exe'
  Start-Sleep -Seconds 1
  foreach ($p in @($Raiz, $pastaSso, $pastaGw, $pastaConfig, (Join-Path $pastaSso 'dados'))) { New-Item -ItemType Directory -Force -Path $p | Out-Null }
  if (-not $Teste) {
    # Só o sistema (os serviços) e os administradores leem os segredos e a chave dos tokens.
    foreach ($p in @($pastaConfig, (Join-Path $pastaSso 'dados'))) {
      & icacls $p /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
      if ($LASTEXITCODE -ne 0) { throw "Não consegui proteger $p (icacls)." }
    }
  }
  Copy-Item -LiteralPath (Join-Path $aqui 'saintsso.exe') -Destination $pastaSso -Force
  if (Test-Path -LiteralPath (Join-Path $pastaSso 'public')) { Remove-Item -LiteralPath (Join-Path $pastaSso 'public') -Recurse -Force }
  Copy-Item -LiteralPath (Join-Path $aqui 'public') -Destination (Join-Path $pastaSso 'public') -Recurse
  Copy-Item -LiteralPath (Join-Path $aqui 'gateway-api.exe') -Destination $pastaGw -Force
  Copy-Item -LiteralPath (Join-Path $aqui 'iniciar-servico.ps1') -Destination $Raiz -Force

  if ($primeira) {
    Passo '2/6 Dados de acesso (o que você digitar não aparece na tela)...'
    $donoGw = UrlBanco (Pedir 'Conexão do banco dos PAGAMENTOS (Neon, projeto 1, postgresql://...)' 'TESTE_BANCO_PAGAMENTOS' -Secreto)
    $bancoSso = UrlBanco (Pedir 'Conexão do banco do LOGIN ÚNICO (Neon, projeto 2, postgresql://...)' 'TESTE_BANCO_SSO' -Secreto)
    $adminEmail = (Pedir 'E-mail do administrador do login único' 'TESTE_ADMIN_EMAIL').Trim().ToLower()
    if ($Teste) { $asaas = '' } else { $asaas = (Pedir 'Chave de API do Asaas (SANDBOX, começa com $aact_hmlg_)' '' -Secreto).Trim() }
    if (-not $Teste -and -not $asaas.StartsWith('$aact_')) { throw 'A chave do Asaas deve começar com $aact_.' }
    if ($adminEmail -notmatch '^[^@\s]+@[^@\s]+\.[^@\s]+$') { throw 'E-mail do administrador inválido.' }
    SalvarEnv $cfgGwDono ([ordered]@{ DATABASE_MIGRATION_URL = $donoGw })

    Passo '3/6 Pagamentos (GatewayRR): banco, usuário do serviço e chaves...'
    Gateway @('migrate') $donoGw | Out-Null
    $senhaServico = Ajudante @('segredo')
    $sql = "DO `$`$ BEGIN IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'gateway_service') THEN CREATE ROLE gateway_service LOGIN PASSWORD '$senhaServico'; ELSE ALTER ROLE gateway_service LOGIN PASSWORD '$senhaServico'; END IF; END `$`$; GRANT gateway_app TO gateway_service;"
    Ajudante @('sql') @{ SQL_URL = $donoGw; SQL_COMANDO = $sql } | Out-Null
    $gw = [ordered]@{
      GATEWAY_BIND_ADDR = "127.0.0.1:$portaGw"
      RUST_LOG = 'info'
      DATABASE_URL = (TrocarUsuario $donoGw 'gateway_service' $senhaServico)
      GATEWAY_TRUSTED_PROXIES = '127.0.0.1'
      GATEWAY_EVENT_SIGNING_KEY = (Ajudante @('segredo'))
    }
    if ($Teste) {
      $gw.PIX_PROVIDER = 'simulator'; $gw.PIX_WEBHOOK_SECRET = (Ajudante @('segredo'))
    } else {
      $gw.PIX_PROVIDER = 'asaas'; $gw.ASAAS_ENVIRONMENT = 'sandbox'; $gw.ASAAS_API_KEY = $asaas; $gw.ASAAS_WEBHOOK_TOKEN = (Ajudante @('segredo'))
    }
    SalvarEnv $cfgGw $gw
  } else {
    Passo '2/6 Configuração existente mantida (só atualização).'
    Passo '3/6 Pagamentos (GatewayRR): atualizando o banco...'
    $donoGw = (LerEnv $cfgGwDono).DATABASE_MIGRATION_URL
    Gateway @('migrate') $donoGw | Out-Null
  }

  Passo '4/6 Ligando os pagamentos...'
  Iniciar $tarefaGw (Join-Path $pastaGw 'gateway-api.exe') $cfgGw 'GatewayRR: Pix dos pacotes (Sistemas Vaz)'
  Esperar "http://127.0.0.1:$portaGw/health/ready" 'GatewayRR'

  if ($primeira) {
    Passo '5/6 Login único (SaintSSO): chave no gateway e configuração...'
    $eventos = Invoke-RestMethod -Uri "http://127.0.0.1:$portaGw/v1/events/signing-key" -TimeoutSec 10
    $par = (Ajudante @('chave-ed25519')) -split ' '
    $tenant = [guid]::NewGuid().ToString()
    $escopos = 'payments:read+payments:write+events:manage'
    $resposta = Gateway @('credential', 'add', $tenant, $par[0], $escopos, 'saintsso') $donoGw
    if ($resposta -notmatch '([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})') { throw "Resposta inesperada do gateway ao registrar a chave: $resposta" }
    $keyId = $Matches[1]
    $sso = [ordered]@{
      SAINTSSO_ISSUER = $issuer
      SAINTSSO_BIND = "127.0.0.1:$portaSso"
      DATABASE_URL = $bancoSso
      SAINTSSO_DB_MAX_CONNECTIONS = '10'
      SAINTSSO_ADMIN_EMAILS = $adminEmail
      SAINTSSO_SIGNING_KEY_PATH = (Join-Path $pastaSso 'dados\signing-key.pem')
      SAINTSSO_GENERATE_SIGNING_KEY = 'true'
      SAINTSSO_STATIC_DIR = (Join-Path $pastaSso 'public')
      SAINTSSO_TRUST_PROXY = $(if ($Teste) { 'false' } else { 'true' })
      SAINTSSO_SESSION_TTL_HOURS = '12'
      RUST_LOG = 'info'
      # Fala com o gateway dentro do servidor, sem sair pela internet.
      GATEWAYRR_URL = "http://127.0.0.1:$portaGw"
      GATEWAYRR_KEY_ID = $keyId
      GATEWAYRR_PRIVATE_KEY_SEED = $par[1]
      GATEWAYRR_EVENT_KEY_ID = $eventos.key_id
      GATEWAYRR_EVENT_PUBLIC_KEY = $eventos.public_key
    }
    SalvarEnv $cfgSso $sso
  } else {
    Passo '5/6 Login único (SaintSSO): configuração mantida.'
  }
  Iniciar $tarefaSso (Join-Path $pastaSso 'saintsso.exe') $cfgSso 'SaintSSO: login único e pacotes (Sistemas Vaz)'
  Esperar "http://127.0.0.1:$portaSso/health/ready" 'SaintSSO'

  if ($primeira -and -not $Teste) {
    # O gateway avisa o SSO de cada Pix pago (sem isso, o SSO só confirma com a tela do Pix aberta).
    $s = LerEnv $cfgSso
    try {
      Ajudante @('webhook') @{ GATEWAYRR_URL = "http://127.0.0.1:$portaGw"; GATEWAYRR_KEY_ID = $s.GATEWAYRR_KEY_ID; GATEWAYRR_PRIVATE_KEY_SEED = $s.GATEWAYRR_PRIVATE_KEY_SEED; SAINTSSO_ISSUER = $issuer } | Out-Null
      Write-Host '   Aviso de pagamento (webhook) registrado no gateway.'
    } catch { Write-Host "   ATENÇÃO: o webhook não foi registrado ($($_.Exception.Message)). Rode este instalador de novo depois." -ForegroundColor Yellow }
  }

  if ($Teste) {
    Passo '6/6 Teste: túnel não mexido.'
  } else {
    Passo "6/6 Publicando sso.$Dominio e pagamentos.$Dominio no túnel..."
    $texto = Get-Content -LiteralPath $configTunel -Raw
    $novas = ''
    foreach ($par in @(@("sso.$Dominio", $portaSso), @("pagamentos.$Dominio", $portaGw))) {
      if ($texto -notmatch [regex]::Escape("hostname: $($par[0])")) { $novas += "  - hostname: $($par[0])`r`n    service: http://127.0.0.1:$($par[1])`r`n" }
    }
    if ($novas) {
      $marca = '  - service: http_status:404'
      if (-not $texto.Contains($marca)) { throw "Formato inesperado em $configTunel." }
      $copia = "$configTunel.antes-sso-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
      Copy-Item -LiteralPath $configTunel -Destination $copia
      Set-Content -LiteralPath $configTunel -Value $texto.Replace($marca, $novas + $marca) -Encoding ascii -NoNewline
      & (Join-Path (Split-Path -Parent $configTunel) 'cloudflared.exe') tunnel --config $configTunel ingress validate
      if ($LASTEXITCODE -ne 0) { Copy-Item -LiteralPath $copia -Destination $configTunel -Force; throw 'A configuração nova do túnel não passou na validação; voltei a anterior.' }
      Stop-ScheduledTask -TaskName 'CampoCertoCloudflare' -ErrorAction SilentlyContinue
      Get-Process cloudflared -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 2
      Start-ScheduledTask -TaskName 'CampoCertoCloudflare'
    }
  }

  Write-Host ''
  Write-Host "PRONTO: login único em $issuer e pagamentos em http://127.0.0.1:$portaGw (interno)." -ForegroundColor Green
  if ($primeira -and -not $Teste) {
    $g = LerEnv $cfgGw
    Write-Host ''
    Write-Host 'FALTAM 3 PASSOS:' -ForegroundColor Yellow
    Write-Host "1) Cloudflare > sistemasvaz.com.br > DNS: crie os registros 'sso' e 'pagamentos' do tipo Tunnel (campo-certo-servidor), como os do sistemasvaz."
    Write-Host '2) Asaas (sandbox) > Integrações > Webhooks:'
    Write-Host "   URL: https://pagamentos.$Dominio/v1/webhooks/pix"
    Write-Host '   Token de autenticação: já está COPIADO (Ctrl+V no campo do token). Eventos: cobranças/pagamentos.'
    Set-Clipboard -Value $g.ASAAS_WEBHOOK_TOKEN
    Write-Host "3) Abra https://sso.$Dominio/register e crie a conta com o e-mail $((LerEnv $cfgSso).SAINTSSO_ADMIN_EMAILS)."
    Write-Host '   Depois rode LIGAR SISTEMAS.cmd para ligar o Emissor e o Campo Certo ao login único.'
  }
} catch {
  Write-Host "ERRO: $($_.Exception.Message)" -ForegroundColor Red
  if (-not $Teste) { Write-Host "Detalhes em $env:TEMP\sistemasvaz-sso-instalacao.log" }
  exit 1
} finally {
  if (-not $Teste) { Stop-Transcript | Out-Null }
}
