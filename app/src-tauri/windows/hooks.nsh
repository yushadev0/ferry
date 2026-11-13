; Extra installer steps for Ferry (see bundle.windows.nsis.installerHooks).

; Ferry registers ferry-host with the browsers each time it starts (and, once
; published, the store extension). Remove those entries with the app.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\app.yusa.ferry"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\app.yusa.ferry"
  ; Without its entry the browser removes the store extension Ferry offered.
  DeleteRegKey HKCU "Software\Google\Chrome\Extensions\ofkibiokjdlboldiiplkllehifkjbihd"
!macroend
