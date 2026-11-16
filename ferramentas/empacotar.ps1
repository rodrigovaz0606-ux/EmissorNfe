# Gera pacote\EmissorNfe (e pacote\EmissorNfe.zip): pasta portatil para rodar em outro PC Windows
# sem instalar Rust nem Node. Leva apenas o programa, a interface compilada e os schemas.
# NAO leva .env, banco (data\), chave-mestra nem certificados.
$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent $PSScriptRoot
Set-Location $raiz

Write-Host '1/4 Compilando a interface...'
npx vite build
if ($LASTEXITCODE -ne 0) { throw 'Falha ao compilar a interface.' }

Write-Host '2/4 Compilando o backend (release, runtime do C embutido: nao precisa instalar nada no outro PC)...'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$env:CARGO_TARGET_DIR = Join-Path $raiz 'backend\target\portatil'
cargo build --release --manifest-path backend/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'Falha ao compilar o backend.' }
Remove-Item Env:RUSTFLAGS, Env:CARGO_TARGET_DIR

Write-Host '3/4 Montando a pasta...'
$destino = Join-Path $raiz 'pacote\EmissorNfe'
if (Test-Path $destino) { Remove-Item -Recurse -Force -Confirm:$false $destino }
New-Item -ItemType Directory -Force $destino | Out-Null
Copy-Item (Join-Path $raiz 'backend\target\portatil\release\emissor-nfe.exe') $destino
Copy-Item -Recurse (Join-Path $raiz 'dist') $destino
Copy-Item -Recurse (Join-Path $raiz 'schemas') $destino
Copy-Item (Join-Path $PSScriptRoot 'INICIAR.bat') $destino
Copy-Item (Join-Path $PSScriptRoot 'LEIA-ME.txt') $destino
# Instalação como servidor (serviço + domínio pelo túnel da Cloudflare).
Copy-Item (Join-Path $PSScriptRoot 'servidor\*') $destino
# Configuracao minima, em PRODUCAO (notas com valor fiscal). Sem caminho nem senha de certificado
# (eles entram pela tela de Produtores).
Set-Content -Encoding ascii (Join-Path $destino '.env') @(
  'NFE_UF=GO',
  'NFE_AMBIENTE=producao',
  '# Mensalidade por Pix (GatewayRR): desligada. Ver README, secao Mensalidade.',
  'API_PAGAMENTO=false',
  'URL_API_PAGAMENTO='
)

Write-Host '4/4 Compactando...'
$zip = Join-Path $raiz 'pacote\EmissorNfe.zip'
if (Test-Path $zip) { Remove-Item -Force -Confirm:$false $zip }
Compress-Archive -Path $destino -DestinationPath $zip
Write-Host "Pronto: $zip"
