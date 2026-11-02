; Instalador do FeatherPDF para Windows (NSIS). Gerado por pacote/gerar.sh:
;   makensis -DVERSAO=0.1.0 -DEXE=caminho/featherpdf-pelicano.exe -DSAIDA=caminho/instalador.exe pacote/featherpdf.nsi
; Instala por usuário (sem pedir administrador), cria atalhos, entra no "Abrir com" do .pdf
; (sem tomar o lugar do leitor padrão) e registra o desinstalador em "Aplicativos instalados".

Unicode true
!include "MUI2.nsh"

!ifndef VERSAO
  !define VERSAO "0.0.0"
!endif

Name "FeatherPDF ${VERSAO}"
OutFile "${SAIDA}"
InstallDir "$LOCALAPPDATA\Programs\FeatherPDF"
InstallDirRegKey HKCU "Software\FeatherWorks\FeatherPDF" "Pasta"
RequestExecutionLevel user
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

Section "FeatherPDF"
  SetOutPath "$INSTDIR"
  File "/oname=featherpdf.exe" "${EXE}"
  File "icones\featherpdf.ico"
  File "/oname=LICENSE.txt" "..\LICENSE"
  WriteUninstaller "$INSTDIR\desinstalar.exe"

  CreateShortCut "$SMPROGRAMS\FeatherPDF.lnk" "$INSTDIR\featherpdf.exe" "" "$INSTDIR\featherpdf.ico"
  CreateShortCut "$DESKTOP\FeatherPDF.lnk" "$INSTDIR\featherpdf.exe" "" "$INSTDIR\featherpdf.ico"

  ; .pdf é formato de terceiros: só o "Abrir com", sem tomar o lugar do programa padrão.
  WriteRegStr HKCU "Software\Classes\FeatherPDF.Documento" "" "Documento PDF"
  WriteRegStr HKCU "Software\Classes\FeatherPDF.Documento\DefaultIcon" "" "$INSTDIR\featherpdf.ico"
  WriteRegStr HKCU "Software\Classes\FeatherPDF.Documento\shell\open\command" "" '"$INSTDIR\featherpdf.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\.pdf\OpenWithProgids" "FeatherPDF.Documento" ""

  WriteRegStr HKCU "Software\FeatherWorks\FeatherPDF" "Pasta" "$INSTDIR"
  WriteRegStr HKCU "${DESINSTALAR}" "DisplayName" "FeatherPDF"
  WriteRegStr HKCU "${DESINSTALAR}" "DisplayVersion" "${VERSAO}"
  WriteRegStr HKCU "${DESINSTALAR}" "Publisher" "FeatherWorks"
  ; Tamanho em "Programas e Recursos" (KB, do que foi instalado).
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${DESINSTALAR}" "EstimatedSize" "$0"
  WriteRegStr HKCU "${DESINSTALAR}" "DisplayIcon" "$INSTDIR\featherpdf.ico"
  WriteRegStr HKCU "${DESINSTALAR}" "URLInfoAbout" "https://github.com/feather-works/featherpdf"
  WriteRegStr HKCU "${DESINSTALAR}" "UninstallString" '"$INSTDIR\desinstalar.exe"'
  WriteRegDWORD HKCU "${DESINSTALAR}" "NoModify" 1
  WriteRegDWORD HKCU "${DESINSTALAR}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\FeatherPDF.lnk"
  Delete "$DESKTOP\FeatherPDF.lnk"
  Delete "$INSTDIR\featherpdf.exe"
  Delete "$INSTDIR\featherpdf.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\desinstalar.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Classes\FeatherPDF.Documento"
  DeleteRegValue HKCU "Software\Classes\.pdf\OpenWithProgids" "FeatherPDF.Documento"
  DeleteRegKey HKCU "Software\FeatherWorks\FeatherPDF"
  DeleteRegKey HKCU "${DESINSTALAR}"
SectionEnd
