; Instalador do FeatherPDF para Windows (NSIS). Gerado por pacote/gerar.sh:
;   makensis -DVERSAO=0.1.0 -DEXE=caminho/featherpdf.exe -DSAIDA=caminho/instalador.exe pacote/featherpdf.nsi
; Instala para todos os usuários em "Arquivos de Programas\FeatherWorks\FeatherPDF" (pede
; administrador uma vez: só o administrador altera os .exe; a pasta FeatherWorks é da suíte), cria atalhos, entra no "Abrir com" do .pdf
; (sem tomar o lugar do leitor padrão) e registra o desinstalador em "Aplicativos instalados".

Unicode true
!include "MUI2.nsh"

!ifndef VERSAO
  !define VERSAO "0.0.0"
!endif

Name "FeatherPDF ${VERSAO}"
OutFile "${SAIDA}"
InstallDir "$PROGRAMFILES64\FeatherWorks\FeatherPDF"
InstallDirRegKey HKLM "Software\FeatherWorks\FeatherPDF" "Pasta"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define MUI_ICON "icones\featherpdf.ico"
!define MUI_UNICON "icones\featherpdf.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\featherpdf.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Abrir o FeatherPDF"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "PortugueseBR"

!include "FileFunc.nsh"
!define DESINSTALAR "Software\Microsoft\Windows\CurrentVersion\Uninstall\FeatherPDF"

; Atalhos de todos os usuários e o registro de 64 bits (o programa é de 64 bits).
Function .onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Function un.onInit
  SetShellVarContext all
  SetRegView 64
FunctionEnd

Section "FeatherPDF"
  ; As versões antigas instalavam só para o usuário (%LOCALAPPDATA%\Programs): tira essa cópia.
  ; _?= faz o desinstalador antigo rodar no lugar e esperar; o próprio .exe e a pasta saem aqui.
  ReadRegStr $2 HKCU "Software\FeatherWorks\FeatherPDF" "Pasta"
  StrCmp $2 "" antiga_fora
  IfFileExists "$2\desinstalar.exe" 0 antiga_fora
    ExecWait '"$2\desinstalar.exe" /S _?=$2'
    Delete "$2\desinstalar.exe"
    RMDir "$2"
  antiga_fora:

  SetOutPath "$INSTDIR"
  ; Com o programa aberto, o .exe em uso não se sobrescreve, mas se renomeia; o .old sai
  ; na próxima instalação ou na desinstalação.
  Delete "$INSTDIR\featherpdf.old"
  Rename "$INSTDIR\featherpdf.exe" "$INSTDIR\featherpdf.old"
  File "/oname=featherpdf.exe" "${EXE}"
  File "icones\featherpdf.ico"
  File "/oname=LICENSE.txt" "..\LICENSE"
  WriteUninstaller "$INSTDIR\desinstalar.exe"

  CreateShortCut "$SMPROGRAMS\FeatherPDF.lnk" "$INSTDIR\featherpdf.exe" "" "$INSTDIR\featherpdf.ico"
  CreateShortCut "$DESKTOP\FeatherPDF.lnk" "$INSTDIR\featherpdf.exe" "" "$INSTDIR\featherpdf.ico"

  ; .pdf é formato de terceiros: só o "Abrir com", sem tomar o lugar do programa padrão.
  WriteRegStr HKLM "Software\Classes\FeatherPDF.Documento" "" "Documento PDF"
  WriteRegStr HKLM "Software\Classes\FeatherPDF.Documento\DefaultIcon" "" "$INSTDIR\featherpdf.ico"
  WriteRegStr HKLM "Software\Classes\FeatherPDF.Documento\shell\open\command" "" '"$INSTDIR\featherpdf.exe" "%1"'
  WriteRegStr HKLM "Software\Classes\.pdf\OpenWithProgids" "FeatherPDF.Documento" ""
  ; Aparece em Aplicativos padrão (o botão "Tornar padrão" abre essa tela já nele).
  WriteRegStr HKLM "Software\FeatherWorks\FeatherPDF\Capabilities" "ApplicationName" "FeatherPDF"
  WriteRegStr HKLM "Software\FeatherWorks\FeatherPDF\Capabilities" "ApplicationDescription" "Editor de PDF da suíte FeatherWorks"
  WriteRegStr HKLM "Software\FeatherWorks\FeatherPDF\Capabilities\FileAssociations" ".pdf" "FeatherPDF.Documento"
  WriteRegStr HKLM "Software\RegisteredApplications" "FeatherPDF" "Software\FeatherWorks\FeatherPDF\Capabilities"

  WriteRegStr HKLM "Software\FeatherWorks\FeatherPDF" "Pasta" "$INSTDIR"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayName" "FeatherPDF"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayVersion" "${VERSAO}"
  WriteRegStr HKLM "${DESINSTALAR}" "Publisher" "FeatherWorks"
  ; Tamanho em "Programas e Recursos" (KB, do que foi instalado).
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${DESINSTALAR}" "EstimatedSize" "$0"
  WriteRegStr HKLM "${DESINSTALAR}" "DisplayIcon" "$INSTDIR\featherpdf.ico"
  WriteRegStr HKLM "${DESINSTALAR}" "URLInfoAbout" "https://github.com/feather-works/featherpdf"
  WriteRegStr HKLM "${DESINSTALAR}" "UninstallString" '"$INSTDIR\desinstalar.exe"'
  WriteRegStr HKLM "${DESINSTALAR}" "InstallLocation" "$INSTDIR"
  WriteRegDWORD HKLM "${DESINSTALAR}" "NoModify" 1
  WriteRegDWORD HKLM "${DESINSTALAR}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\FeatherPDF.lnk"
  Delete "$DESKTOP\FeatherPDF.lnk"
  Delete "$INSTDIR\featherpdf.exe"
  Delete "$INSTDIR\featherpdf.old"
  Delete "$INSTDIR\featherpdf.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\desinstalar.exe"
  RMDir "$INSTDIR"
  ; A pasta da suíte só sai se ficou vazia (sem outro programa nem o Launcher).
  RMDir "$INSTDIR\.."
  DeleteRegKey HKLM "Software\Classes\FeatherPDF.Documento"
  DeleteRegValue HKLM "Software\Classes\.pdf\OpenWithProgids" "FeatherPDF.Documento"
  DeleteRegValue HKLM "Software\RegisteredApplications" "FeatherPDF"
  DeleteRegKey HKLM "Software\FeatherWorks\FeatherPDF"
  DeleteRegKey /ifempty HKLM "Software\FeatherWorks"
  DeleteRegKey HKLM "${DESINSTALAR}"
SectionEnd
