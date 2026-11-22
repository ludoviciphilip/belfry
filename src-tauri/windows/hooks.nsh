; Tauri NSIS hooks: the installer runs elevated (perMachine), so it can
; stop, register and start the belfryd service itself.

!macro NSIS_HOOK_PREINSTALL
  ; Upgrading: stop the old service so its files can be replaced.
  nsExec::Exec 'sc.exe stop belfryd'
  Sleep 2000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog '"$INSTDIR\belfryd.exe" install'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\belfryd.exe" uninstall'
  Sleep 1000
!macroend
