param(
  [Parameter(Mandatory)] [string]$Exe,
  [Parameter(Mandatory)] [string]$Config,
  # Arquivo de registro (log). Vazio = saída normal (modo de teste).
  [string]$Log = '',
  [string[]]$Argumentos = @()
)
# Inicia o SaintSSO ou o GatewayRR com as variáveis do arquivo de configuração (NOME=valor por
# linha). Os programas não leem .env sozinhos; o arquivo fica numa pasta que só o sistema e os
# administradores leem. Usado pela tarefa do Windows (e pelo modo de teste do instalador).
# Os registros nunca contêm chaves nem senhas (os dois programas não as escrevem).
$ErrorActionPreference = 'Stop'
foreach ($linha in Get-Content -LiteralPath $Config) {
  if ($linha -match '^\s*([A-Z][A-Z0-9_]*)\s*=(.*)$') { Set-Item -Path "Env:$($Matches[1])" -Value $Matches[2].Trim() }
}
Set-Location -LiteralPath (Split-Path -Parent $Exe)
if (-not $Log) {
  & $Exe @Argumentos
  exit $LASTEXITCODE
}
# Guarda até ~10 MB por arquivo, com uma cópia anterior (.1).
if ((Test-Path -LiteralPath $Log) -and (Get-Item -LiteralPath $Log).Length -gt 10MB) {
  Move-Item -LiteralPath $Log -Destination "$Log.1" -Force
}
$ErrorActionPreference = 'Continue'  # os programas escrevem o registro no stderr
& $Exe @Argumentos 2>&1 | ForEach-Object { "$_" } | Out-File -LiteralPath $Log -Append -Encoding utf8
exit $LASTEXITCODE
