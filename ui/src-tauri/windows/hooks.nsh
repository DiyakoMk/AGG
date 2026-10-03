!macro NSIS_HOOK_POSTINSTALL
  StrCpy $1 ""
  IfFileExists "$INSTDIR\agg-svc.exe" 0 try_resources
    StrCpy $1 "$INSTDIR\agg-svc.exe"
    Goto do_svc_install
  try_resources:
  IfFileExists "$INSTDIR\resources\agg-svc.exe" 0 skip_svc_install
    StrCpy $1 "$INSTDIR\resources\agg-svc.exe"
  do_svc_install:
    nsExec::ExecToLog '"$1" install'
    Pop $0
    IntCmp $0 0 skip_svc_install
      MessageBox MB_OK "AGGService failed to install (error $0). Run agg-svc.exe install as Administrator."
  skip_svc_install:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  IfFileExists "$INSTDIR\agg-svc.exe" 0 try_resources_rm
    nsExec::ExecToLog '"$INSTDIR\agg-svc.exe" uninstall'
    Pop $0
    Goto skip_svc_remove
  try_resources_rm:
  IfFileExists "$INSTDIR\resources\agg-svc.exe" 0 skip_svc_remove
    nsExec::ExecToLog '"$INSTDIR\resources\agg-svc.exe" uninstall'
    Pop $0
  skip_svc_remove:
!macroend
