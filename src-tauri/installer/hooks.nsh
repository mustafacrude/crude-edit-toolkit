; Extra install and uninstall steps for the Windows installer.
;
; "Send to Crude's Toolkit" in Explorer's right-click menu, for the file types the app
; handles and for folders. It's written under HKCU (this user only, no admin) and
; removed again on uninstall. On Windows 11 it sits under "Show more options".
; Nothing here starts the app at sign-in.

!macro CrudeVerb EXT
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\CrudeToolkit" "" "Send to Crude's Toolkit"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\CrudeToolkit" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\CrudeToolkit\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro CrudeVerbRemove EXT
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\CrudeToolkit"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Video
  !insertmacro CrudeVerb "mkv"
  !insertmacro CrudeVerb "mp4"
  !insertmacro CrudeVerb "mov"
  !insertmacro CrudeVerb "webm"
  !insertmacro CrudeVerb "m4v"
  !insertmacro CrudeVerb "avi"
  !insertmacro CrudeVerb "flv"
  ; Images
  !insertmacro CrudeVerb "png"
  !insertmacro CrudeVerb "jpg"
  !insertmacro CrudeVerb "jpeg"
  !insertmacro CrudeVerb "jfif"
  !insertmacro CrudeVerb "webp"
  !insertmacro CrudeVerb "avif"
  !insertmacro CrudeVerb "heic"
  !insertmacro CrudeVerb "heif"
  !insertmacro CrudeVerb "svg"
  !insertmacro CrudeVerb "jxl"
  !insertmacro CrudeVerb "tif"
  !insertmacro CrudeVerb "tiff"
  !insertmacro CrudeVerb "bmp"
  ; Audio
  !insertmacro CrudeVerb "wav"
  !insertmacro CrudeVerb "mp3"
  !insertmacro CrudeVerb "m4a"
  !insertmacro CrudeVerb "aac"
  !insertmacro CrudeVerb "flac"
  !insertmacro CrudeVerb "opus"
  !insertmacro CrudeVerb "ogg"
  ; Folders
  WriteRegStr HKCU "Software\Classes\Directory\shell\CrudeToolkit" "" "Send to Crude's Toolkit"
  WriteRegStr HKCU "Software\Classes\Directory\shell\CrudeToolkit" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe"
  WriteRegStr HKCU "Software\Classes\Directory\shell\CrudeToolkit\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CrudeVerbRemove "mkv"
  !insertmacro CrudeVerbRemove "mp4"
  !insertmacro CrudeVerbRemove "mov"
  !insertmacro CrudeVerbRemove "webm"
  !insertmacro CrudeVerbRemove "m4v"
  !insertmacro CrudeVerbRemove "avi"
  !insertmacro CrudeVerbRemove "flv"
  !insertmacro CrudeVerbRemove "png"
  !insertmacro CrudeVerbRemove "jpg"
  !insertmacro CrudeVerbRemove "jpeg"
  !insertmacro CrudeVerbRemove "jfif"
  !insertmacro CrudeVerbRemove "webp"
  !insertmacro CrudeVerbRemove "avif"
  !insertmacro CrudeVerbRemove "heic"
  !insertmacro CrudeVerbRemove "heif"
  !insertmacro CrudeVerbRemove "svg"
  !insertmacro CrudeVerbRemove "jxl"
  !insertmacro CrudeVerbRemove "tif"
  !insertmacro CrudeVerbRemove "tiff"
  !insertmacro CrudeVerbRemove "bmp"
  !insertmacro CrudeVerbRemove "wav"
  !insertmacro CrudeVerbRemove "mp3"
  !insertmacro CrudeVerbRemove "m4a"
  !insertmacro CrudeVerbRemove "aac"
  !insertmacro CrudeVerbRemove "flac"
  !insertmacro CrudeVerbRemove "opus"
  !insertmacro CrudeVerbRemove "ogg"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\CrudeToolkit"
!macroend
