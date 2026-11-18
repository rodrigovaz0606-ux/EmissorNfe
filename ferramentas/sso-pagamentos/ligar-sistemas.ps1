param(
  # Teste neste PC: sem reiniciar tarefas; senha do admin vem de TESTE_ADMIN_SENHA.
  [switch]$Teste,
  [string]$Raiz = (Join-Path $env:ProgramData 'SistemasVaz'),
  [string]$UrlEmissor = 'https://sistemasvaz.com.br',
  [string]$UrlCampoCerto = 'https://campocertoprodutorrural.com.br',
  [string]$ConfigEmissor = (Join-Path $env:ProgramData 'EmissorNfe\.env'),
  [string]$ConfigCampoCerto = (Join-Path $env:ProgramData 'CampoCerto\config.env'),
  [string]$PortaCampoCerto = '80'
)
# Liga o Emissor NF-e e o Campo Certo ao login único (DEPLOY-INFRA.md, etapa 5.3/5.4) e à
# integração das notas (Emissor -> Campo Certo). Rode depois de criar a conta do administrador em
# https://sso.<domínio>/register. Pode rodar de novo: os segredos são trocados e regravados.
#
# Os client_secret e o segredo da integração vão direto para a configuração de cada sistema
# (pastas que só o sistema e os administradores leem); nada aparece na tela.
$ErrorActionPreference = 'Stop'
$aqui = $PSScriptRoot
$ajudante = Join-Path $aqui 'ajudante-sso.exe'
$pastaConfig = Join-Path $Raiz 'config'
$cfgSso = Join-Path $pastaConfig 'sso.env'

function LerEnv($arquivo) {
  $h = [ordered]@{}
  if (Test-Path -LiteralPath $arquivo) {
    foreach ($l in Get-Content -LiteralPath $arquivo) { if ($l -match '^\s*([A-Z][A-Z0-9_]*)\s*=(.*)$') { $h[$Matches[1]] = $Matches[2].Trim() } }
  }
  $h
}

# Troca ou acrescenta NOME=valor, mantendo o resto do arquivo (comentários inclusive).
function AtualizarEnv($arquivo, $valores) {
  $linhas = @()
  if (Test-Path -LiteralPath $arquivo) { $linhas = @(Get-Content -LiteralPath $arquivo) }
  foreach ($k in $valores.Keys) {
    $achou = $false
    for ($i = 0; $i -lt $linhas.Count; $i++) {
      if ($linhas[$i] -match "^\s*$k\s*=") { $linhas[$i] = "$k=$($valores[$k])"; $achou = $true }
    }
    if (-not $achou) { $linhas += "$k=$($valores[$k])" }
  }
  Set-Content -LiteralPath $arquivo -Value $linhas -Encoding ascii
  if (-not $Teste) {
    & icacls $arquivo /inheritance:r /grant:r '*S-1-5-18:F' '*S-1-5-32-544:F' | Out-Null
  }
}

function Ajudante([string[]]$argumentos, $variaveis = @{}) {
  $ErrorActionPreference = 'Continue'
  $antes = @{}
  foreach ($k in $variaveis.Keys) { $antes[$k] = [Environment]::GetEnvironmentVariable($k, 'Process'); [Environment]::SetEnvironmentVariable($k, $variaveis[$k], 'Process') }
  $erros = [IO.Path]::GetTempFileName()
  try {
    $saida = & $ajudante @argumentos 2> $erros
    if ($LASTEXITCODE -ne 0) { throw "ajudante $($argumentos[0]) falhou: $((Get-Content -LiteralPath $erros) -join ' ')" }
    return ($saida | Out-String).Trim()
  } finally {
    foreach ($k in $variaveis.Keys) { [Environment]::SetEnvironmentVariable($k, $antes[$k], 'Process') }
    Remove-Item -LiteralPath $erros -Force -ErrorAction SilentlyContinue
  }
}

$arquivoSegredos = $null
try {
  if (-not (Test-Path -LiteralPath $cfgSso)) { throw 'O login único ainda não foi instalado neste servidor (rode antes o INSTALAR SSO E PAGAMENTOS.cmd).' }
  if (-not (Test-Path -LiteralPath $ConfigEmissor)) { throw "Não achei a configuração do Emissor ($ConfigEmissor). Instale o Emissor antes." }
  if (-not $Teste -and -not (Test-Path -LiteralPath (Split-Path -Parent $ConfigCampoCerto))) { throw 'Não achei o Campo Certo neste servidor.' }
  $sso = LerEnv $cfgSso
  $issuer = $sso.SAINTSSO_ISSUER
  $admin = ($sso.SAINTSSO_ADMIN_EMAILS -split ',')[0].Trim()

  Write-Host "Administrador do login único: $admin" -ForegroundColor Cyan
  if ($Teste) { $senha = $env:TESTE_ADMIN_SENHA } else {
    $s = Read-Host -Prompt 'Senha desse administrador (não aparece na tela)' -AsSecureString
    $p = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($s)
    try { $senha = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($p) } finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($p) }
  }

  Write-Host '1/3 Cadastrando o Emissor e o Campo Certo no login único...' -ForegroundColor Cyan
  $clientes = @(
    [ordered]@{ client_id = 'emissor-nfe-web'; name = 'Emissor NF-e'; redirect_uris = @("$UrlEmissor/callback"); post_logout_redirect_uris = @("$UrlEmissor/"); product = 'emissor_nfe'; first_party = $true; confidential = $true },
    [ordered]@{ client_id = 'campo-certo-web'; name = 'Campo Certo'; redirect_uris = @("$UrlCampoCerto/callback"); post_logout_redirect_uris = @("$UrlCampoCerto/"); product = 'campo_certo'; first_party = $true; confidential = $true }
  )
  $entrada = Join-Path $pastaConfig 'clientes.json'
  $arquivoSegredos = Join-Path $pastaConfig 'segredos-clientes.json'
  # ConvertTo-Json do PowerShell 5.1 vira lista de 1 item em objeto; o -InputObject mantém a lista.
  [IO.File]::WriteAllText($entrada, (ConvertTo-Json -InputObject $clientes -Depth 5), (New-Object Text.UTF8Encoding $false))
  Ajudante @('clientes', $entrada, $arquivoSegredos) @{ SAINTSSO_URL = 'http://127.0.0.1:' + ($sso.SAINTSSO_BIND -split ':')[-1]; SAINTSSO_ADMIN_EMAIL = $admin; SAINTSSO_ADMIN_PASSWORD = $senha } | Write-Host
  $segredos = Get-Content -LiteralPath $arquivoSegredos -Raw | ConvertFrom-Json

  Write-Host '2/3 Gravando a configuração do Emissor e do Campo Certo...' -ForegroundColor Cyan
  $integracao = Ajudante @('segredo')
  AtualizarEnv $ConfigEmissor ([ordered]@{
    SSO_ENABLED = 'true'; SSO_ISSUER = $issuer; SSO_CLIENT_ID = 'emissor-nfe-web'; SSO_CLIENT_SECRET = $segredos.'emissor-nfe-web'
    CAMPO_CERTO_URL = "http://127.0.0.1:$PortaCampoCerto"; INTEGRACAO_EMISSOR_SEGREDO = $integracao
  })
  AtualizarEnv $ConfigCampoCerto ([ordered]@{
    SSO_ENABLED = 'true'; SSO_ISSUER = $issuer; SSO_CLIENT_ID = 'campo-certo-web'; SSO_CLIENT_SECRET = $segredos.'campo-certo-web'
    INTEGRACAO_EMISSOR_SEGREDO = $integracao
  })

  if ($Teste) {
    Write-Host '3/3 Teste: sistemas não reiniciados.' -ForegroundColor Cyan
  } else {
    Write-Host '3/3 Reiniciando o Emissor e o Campo Certo...' -ForegroundColor Cyan
    foreach ($t in @('EmissorNfeServidor', 'CampoCertoServidor')) {
      Stop-ScheduledTask -TaskName $t -ErrorAction SilentlyContinue
    }
    Get-CimInstance Win32_Process -Filter "Name='emissor-nfe.exe'" | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Get-CimInstance Win32_Process -Filter "Name='node.exe'" | Where-Object { $_.ExecutablePath -like "$env:ProgramData\CampoCerto*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 3
    foreach ($t in @('EmissorNfeServidor', 'CampoCertoServidor')) { Start-ScheduledTask -TaskName $t }
  }
  Write-Host ''
  Write-Host 'PRONTO: o Emissor e o Campo Certo agora têm "Entrar com SaintSSO", e as notas do Emissor vão para o Campo Certo.' -ForegroundColor Green
} catch {
  Write-Host "ERRO: $($_.Exception.Message)" -ForegroundColor Red
  exit 1
} finally {
  if ($arquivoSegredos) { Remove-Item -LiteralPath $arquivoSegredos -Force -ErrorAction SilentlyContinue }
}
