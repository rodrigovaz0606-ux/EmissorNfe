param(
  # Clones dos repositórios do SaintSSO e do GatewayRR (github.com/limiticel).
  [Parameter(Mandatory)] [string]$SaintSSO,
  [Parameter(Mandatory)] [string]$GatewayRR,
  [string]$Destino = ''
)
# Monta pacote\SSO-Pagamentos(.zip): programas do SaintSSO e do GatewayRR compilados para Windows,
# a tela do SaintSSO, o ajudante e os instaladores. Runtime do C embutido (não precisa instalar nada).
$ErrorActionPreference = 'Stop'
if (-not $Destino) { $Destino = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'pacote' }
$alvo = Join-Path $Destino 'SSO-Pagamentos'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$ErrorActionPreference = 'Continue'  # cargo/npm escrevem progresso no stderr; vale o código de saída
try {
  Write-Host '1/4 SaintSSO (programa e tela)...'
  Push-Location (Join-Path $SaintSSO 'backend'); cargo build --release; if ($LASTEXITCODE) { throw 'falha ao compilar o SaintSSO' }; Pop-Location
  Push-Location (Join-Path $SaintSSO 'frontend'); npm ci --no-audit --no-fund; npm run build; if ($LASTEXITCODE) { throw 'falha ao compilar a tela do SaintSSO' }; Pop-Location
  Write-Host '2/4 GatewayRR...'
  Push-Location $GatewayRR; cargo build --release -p gateway-api; if ($LASTEXITCODE) { throw 'falha ao compilar o GatewayRR' }; Pop-Location
  Write-Host '3/4 Ajudante...'
  Push-Location (Join-Path $PSScriptRoot 'ajudante'); cargo build --release; if ($LASTEXITCODE) { throw 'falha ao compilar o ajudante' }; Pop-Location
} finally { Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue; $ErrorActionPreference = 'Stop' }

Write-Host '4/4 Montando...'
if (Test-Path $alvo) { Remove-Item -Recurse -Force -Confirm:$false $alvo }
New-Item -ItemType Directory -Force $alvo | Out-Null
Copy-Item (Join-Path $SaintSSO 'backend\target\release\saintsso.exe') $alvo
Copy-Item (Join-Path $GatewayRR 'target\release\gateway-api.exe') $alvo
Copy-Item (Join-Path $PSScriptRoot 'ajudante\target\release\ajudante-sso.exe') $alvo
Copy-Item -Recurse (Join-Path $SaintSSO 'frontend\dist') (Join-Path $alvo 'public')
foreach ($f in 'instalar.ps1', 'ligar-sistemas.ps1', 'iniciar-servico.ps1') {
  # PowerShell 5.1 só lê acentos com BOM; Windows quer CRLF.
  $t = [IO.File]::ReadAllText((Join-Path $PSScriptRoot $f)) -replace "`r?`n", "`r`n"
  [IO.File]::WriteAllText((Join-Path $alvo $f), $t, (New-Object Text.UTF8Encoding $true))
}
Copy-Item (Join-Path $PSScriptRoot 'INSTALAR SSO E PAGAMENTOS.cmd'), (Join-Path $PSScriptRoot 'LIGAR SISTEMAS.cmd'), (Join-Path $PSScriptRoot 'LEIA-ME.txt') $alvo
$zip = "$alvo.zip"
if (Test-Path $zip) { Remove-Item -Force -Confirm:$false $zip }
Compress-Archive -Path $alvo -DestinationPath $zip
Write-Host "Pronto: $zip"
