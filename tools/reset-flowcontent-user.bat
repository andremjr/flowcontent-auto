@echo off
setlocal EnableExtensions

echo ============================================
echo FlowContent Auto - limpeza local do usuario
echo ============================================
echo.
echo Este script remove apenas a instalacao local e os dados do usuario.
echo Ele nao mexe no codigo-fonte do projeto.
echo.

set "APP_INSTALL=%LOCALAPPDATA%\FlowContent Auto"
set "APP_DATA_ROAMING=%APPDATA%\com.flowcontent.auto"
set "APP_DATA_LOCAL=%LOCALAPPDATA%\com.flowcontent.auto"
set "APP_SHORTCUT=%APPDATA%\Microsoft\Windows\Start Menu\Programs\FlowContent Auto.lnk"
set "DESKTOP_SHORTCUT=%USERPROFILE%\Desktop\FlowContent Auto.lnk"
set "PUBLIC_DESKTOP_SHORTCUT=%PUBLIC%\Desktop\FlowContent Auto.lnk"
set "UNINSTALL_KEY=HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\FlowContent Auto"

echo Fechando o app, se estiver aberto...
taskkill /IM flowcontent-auto.exe /F >nul 2>&1
taskkill /IM FlowContent.Auto.exe /F >nul 2>&1

echo.
echo Removendo instalacao anterior...
if exist "%APP_INSTALL%" (
  rmdir /S /Q "%APP_INSTALL%"
  if exist "%APP_INSTALL%" (
    echo [ERRO] Nao foi possivel remover: "%APP_INSTALL%"
  ) else (
    echo [OK] Instalacao removida: "%APP_INSTALL%"
  )
) else (
  echo [OK] Instalacao nao encontrada: "%APP_INSTALL%"
)

echo.
echo Removendo dados do usuario...
if exist "%APP_DATA_ROAMING%" (
  rmdir /S /Q "%APP_DATA_ROAMING%"
  if exist "%APP_DATA_ROAMING%" (
    echo [ERRO] Nao foi possivel remover: "%APP_DATA_ROAMING%"
  ) else (
    echo [OK] Dados removidos: "%APP_DATA_ROAMING%"
  )
) else (
  echo [OK] Dados nao encontrados: "%APP_DATA_ROAMING%"
)

if exist "%APP_DATA_LOCAL%" (
  rmdir /S /Q "%APP_DATA_LOCAL%"
  if exist "%APP_DATA_LOCAL%" (
    echo [ERRO] Nao foi possivel remover: "%APP_DATA_LOCAL%"
  ) else (
    echo [OK] Dados removidos: "%APP_DATA_LOCAL%"
  )
) else (
  echo [OK] Dados nao encontrados: "%APP_DATA_LOCAL%"
)

echo.
echo Removendo atalhos...
if exist "%APP_SHORTCUT%" (
  del /F /Q "%APP_SHORTCUT%" >nul 2>&1
)
if exist "%DESKTOP_SHORTCUT%" (
  del /F /Q "%DESKTOP_SHORTCUT%" >nul 2>&1
)
if exist "%PUBLIC_DESKTOP_SHORTCUT%" (
  del /F /Q "%PUBLIC_DESKTOP_SHORTCUT%" >nul 2>&1
)
echo [OK] Atalhos antigos removidos, se existiam.

echo.
echo Limpando registro de desinstalacao do usuario...
reg delete "%UNINSTALL_KEY%" /f >nul 2>&1
echo [OK] Chave antiga removida, se existia.

echo.
echo Verificacao final...
if exist "%APP_INSTALL%" echo [PENDENTE] Ainda existe: "%APP_INSTALL%"
if exist "%APP_DATA_ROAMING%" echo [PENDENTE] Ainda existe: "%APP_DATA_ROAMING%"
if exist "%APP_DATA_LOCAL%" echo [PENDENTE] Ainda existe: "%APP_DATA_LOCAL%"
if not exist "%APP_INSTALL%" if not exist "%APP_DATA_ROAMING%" if not exist "%APP_DATA_LOCAL%" (
  echo [OK] Limpeza concluida.
)

echo.
echo Proximo passo:
echo 1. Reiniciar o Windows, se quiser garantir que nada ficou em memoria.
echo 2. Baixar novamente o instalador mais recente.
echo 3. Instalar e abrir o app.
echo.
pause
