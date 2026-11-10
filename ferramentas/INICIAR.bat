@echo off
rem As mensagens do programa saem em UTF-8 (acentos).
chcp 65001 >nul
cd /d "%~dp0"
title Emissor NF-e
echo.
echo   Emissor NF-e - homologacao
echo   Deixe esta janela aberta enquanto usa o emissor. Para encerrar, feche a janela.
echo.
rem Abre o navegador depois de 2 segundos, quando o servidor ja subiu.
start "" /min cmd /c "timeout /t 2 /nobreak >nul & start http://localhost:3335"
emissor-nfe.exe servidor
echo.
echo   O emissor parou. Veja a mensagem acima.
pause
