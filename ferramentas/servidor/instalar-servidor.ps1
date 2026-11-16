$ErrorActionPreference = 'Stop'
# Instala o Emissor NF-e neste servidor como serviço (liga junto com o Windows) e publica em
# https://sistemasvaz.com.br pelo túnel da Cloudflare que o Campo Certo já usa aqui.
# Pode rodar de novo para atualizar: mantém o banco, a chave e o .env que já estiverem instalados.
Start-Transcript -LiteralPath (Join-Path $env:TEMP 'emissornfe-instalacao.log') -Force | Out-Null
try {
  $aqui = $PSScriptRoot
  $dominio = 'sistemasvaz.com.br'
  $porta = 3335
  $destino = Join-Path $env:ProgramData 'EmissorNfe'
  $dados = Join-Path $destino 'data'
  $tarefa = 'EmissorNfeServidor'
  $pastaTunel = Join-Path $env:ProgramData 'CampoCerto\cloudflared'
  $configTunel = Join-Path $pastaTunel 'config.yml'
  $tarefaTunel = 'CampoCertoCloudflare'
  $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'

  foreach ($item in @('emissor-nfe.exe', 'dist\index.html', 'schemas')) {
    if (-not (Test-Path -LiteralPath (Join-Path $aqui $item))) { throw "Pasta incompleta: falta $item. Extraia o ZIP inteiro e rode de novo." }
  }
  if (-not (Test-Path -LiteralPath $configTunel)) {
    throw "Não achei o túnel do Campo Certo ($configTunel). Este instalador usa o túnel que já existe neste servidor."
  }

  Write-Host '1/5 Parando a versão anterior (se houver)...' -ForegroundColor Cyan
  Stop-ScheduledTask -TaskName $tarefa -ErrorAction SilentlyContinue
  Get-CimInstance Win32_Process -Filter "Name='emissor-nfe.exe'" | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
  if (Get-NetTCPConnection -LocalPort $porta -State Listen -ErrorAction SilentlyContinue) {
    throw "A porta $porta continua ocupada. Feche a janela preta do INICIAR.bat (se estiver aberta) e rode de novo."
  }

  Write-Host '2/5 Copiando o programa...' -ForegroundColor Cyan
  New-Item -ItemType Directory -Force -Path $destino | Out-Null
  foreach ($pasta in @('dist', 'schemas')) {
    $alvo = Join-Path $destino $pasta
    if (Test-Path -LiteralPath $alvo) { Remove-Item -LiteralPath $alvo -Recurse -Force }
    Copy-Item -LiteralPath (Join-Path $aqui $pasta) -Destination $alvo -Recurse
  }
  Copy-Item -LiteralPath (Join-Path $aqui 'emissor-nfe.exe') -Destination $destino -Force
  $envDestino = Join-Path $destino '.env'
  # Na primeira instalação usa o .env do pacote; depois mantém o que já estiver no servidor.
  if (-not (Test-Path -LiteralPath $envDestino)) {
    if (Test-Path -LiteralPath (Join-Path $aqui '.env')) { Copy-Item -LiteralPath (Join-Path $aqui '.env') -Destination $envDestino }
    else { Set-Content -LiteralPath $envDestino -Encoding ascii -Value @('NFE_UF=GO', 'NFE_AMBIENTE=producao', 'API_PAGAMENTO=false') }
  }

  Write-Host '3/5 Banco de dados e chave dos certificados...' -ForegroundColor Cyan
  # Se este pacote foi extraído por cima da pasta usada antes (com INICIAR.bat), traz os dados
  # de lá: notas, produtores e a chave-mestra. Move (não copia) para não ficar uma segunda cópia.
  $dadosAntigos = Join-Path $aqui 'data'
  $temAntigos = Test-Path -LiteralPath (Join-Path $dadosAntigos 'chave-mestra.key')
  $temInstalados = Test-Path -LiteralPath (Join-Path $dados 'chave-mestra.key')
  if ($temAntigos -and -not $temInstalados) {
    if (Test-Path -LiteralPath $dados) { Rename-Item -LiteralPath $dados -NewName "data-vazia-$stamp" }
    Move-Item -LiteralPath $dadosAntigos -Destination $dados
    Write-Host "   Dados trazidos de $dadosAntigos"
  } elseif ($temAntigos -and $temInstalados) {
    Write-Host "   Já existe um banco instalado; o da pasta $dadosAntigos NÃO foi usado." -ForegroundColor Yellow
  }
  New-Item -ItemType Directory -Force -Path $dados | Out-Null
  # Só o sistema (o serviço) e os administradores leem a pasta: ela guarda os certificados cifrados e a chave.
  & icacls $dados /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
  if ($LASTEXITCODE -ne 0) { throw 'Não consegui proteger a pasta de dados (icacls).' }

  Write-Host '4/5 Registrando o serviço...' -ForegroundColor Cyan
  if (Get-ScheduledTask -TaskName $tarefa -ErrorAction SilentlyContinue) { Unregister-ScheduledTask -TaskName $tarefa -Confirm:$false }
  $acao = New-ScheduledTaskAction -Execute (Join-Path $destino 'emissor-nfe.exe') -Argument 'servidor' -WorkingDirectory $destino
  $gatilho = New-ScheduledTaskTrigger -AtStartup
  $conta = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
  $ajustes = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
  Register-ScheduledTask -TaskName $tarefa -Description 'Emissor NF-e (sistemasvaz.com.br)' -Action $acao -Trigger $gatilho -Principal $conta -Settings $ajustes | Out-Null
  Start-ScheduledTask -TaskName $tarefa
  $estado = $null
  for ($i = 0; $i -lt 30 -and -not $estado; $i++) {
    Start-Sleep -Seconds 1
    try { $estado = Invoke-RestMethod -Uri "http://127.0.0.1:$porta/api/auth/estado" -TimeoutSec 2 } catch {}
  }
  if (-not $estado) { throw 'O emissor não respondeu. Veja o Agendador de Tarefas (EmissorNfeServidor).' }
  Write-Host "   Emissor no ar em http://localhost:$porta ($($estado.ambiente))"

  Write-Host "5/5 Ligando $dominio no túnel..." -ForegroundColor Cyan
  $texto = Get-Content -LiteralPath $configTunel -Raw
  if ($texto -notmatch [regex]::Escape("hostname: $dominio")) {
    $marca = '  - service: http_status:404'
    if (-not $texto.Contains($marca)) { throw "Formato inesperado em $configTunel (falta a linha final 'service: http_status:404')." }
    Copy-Item -LiteralPath $configTunel -Destination "$configTunel.antes-emissor-$stamp"
    $novas = "  - hostname: $dominio`r`n    service: http://127.0.0.1:$porta`r`n  - hostname: www.$dominio`r`n    service: http://127.0.0.1:$porta`r`n"
    $texto = $texto.Replace($marca, $novas + $marca)
    Set-Content -LiteralPath $configTunel -Value $texto -Encoding ascii -NoNewline
    $cloudflared = Join-Path $pastaTunel 'cloudflared.exe'
    & $cloudflared tunnel --config $configTunel ingress validate
    if ($LASTEXITCODE -ne 0) {
      Copy-Item -LiteralPath "$configTunel.antes-emissor-$stamp" -Destination $configTunel -Force
      throw 'A configuração nova do túnel não passou na validação; voltei a anterior (o Campo Certo segue igual).'
    }
  } else {
    Write-Host '   O domínio já estava no túnel.'
  }
  Stop-ScheduledTask -TaskName $tarefaTunel -ErrorAction SilentlyContinue
  Get-Process cloudflared -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
  Start-ScheduledTask -TaskName $tarefaTunel
  $ligado = $false
  for ($i = 0; $i -lt 20 -and -not $ligado; $i++) {
    Start-Sleep -Seconds 1
    $ligado = [bool](Get-Process cloudflared -ErrorAction SilentlyContinue)
  }
  if (-not $ligado) { throw "O túnel não voltou. Veja o Agendador de Tarefas ($tarefaTunel)." }

  Write-Host ''
  Write-Host "PRONTO: Emissor NF-e instalado. Endereço: https://$dominio" -ForegroundColor Green
  if (-not $estado.configurado) {
    Write-Host 'FALTA 1 PASSO: crie o usuário administrador AQUI NESTE SERVIDOR (pela internet isso é bloqueado).' -ForegroundColor Yellow
    Write-Host "Abrindo http://localhost:$porta ..." -ForegroundColor Yellow
    Start-Process "http://localhost:$porta"
  }
  Write-Host 'O Campo Certo continua no mesmo túnel, sem mudança.'
  Write-Host 'Apague o ZIP e a pasta extraída depois de instalar.' -ForegroundColor Yellow
} catch {
  Write-Host "ERRO: $($_.Exception.Message)" -ForegroundColor Red
  Write-Host "Detalhes em $env:TEMP\emissornfe-instalacao.log"
} finally {
  Stop-Transcript | Out-Null
}
