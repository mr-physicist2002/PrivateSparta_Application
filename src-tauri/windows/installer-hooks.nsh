; Give Windows a new icon cache key on every release. The executable path is
; stable across upgrades, so Explorer can otherwise keep showing the old icon.
!macro NSIS_HOOK_POSTINSTALL
  Delete "$INSTDIR\PrivateSparta-icon-*.ico"
  CopyFiles /SILENT "$INSTDIR\PrivateSparta.ico" "$INSTDIR\PrivateSparta-icon-${VERSION}.ico"

  IfFileExists "$DESKTOP\${PRODUCTNAME}.lnk" 0 desktop_done
    CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\PrivateSparta-icon-${VERSION}.ico" 0
    !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
  desktop_done:

  IfFileExists "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" 0 start_menu_root
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\PrivateSparta-icon-${VERSION}.ico" 0
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"

  start_menu_root:
  IfFileExists "$SMPROGRAMS\${PRODUCTNAME}.lnk" 0 shortcuts_done
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$INSTDIR\PrivateSparta-icon-${VERSION}.ico" 0
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"

  shortcuts_done:
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\PrivateSparta-icon-*.ico"
!macroend
