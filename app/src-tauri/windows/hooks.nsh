; SPDX-License-Identifier: GPL-3.0-or-later
; Greffon de l'installateur NSIS (Lot 11, ADR-0015) : enregistre MegaUltraPDF comme
; application capable d'ouvrir les PDF (Capabilities + RegisteredApplications). C'est ce qui
; permet a Parametres > Applications par defaut d'afficher la fiche de l'application, et au
; bouton des preferences d'y mener directement (ms-settings:defaultapps?registeredAppUser=).
; SHCTX : la ruche de l'installation (utilisateur courant par defaut).

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\MegaUltraPDF\Capabilities" "ApplicationName" "MegaUltraPDF"
  WriteRegStr SHCTX "Software\MegaUltraPDF\Capabilities" "ApplicationDescription" "Free PDF reader and editor"
  WriteRegStr SHCTX "Software\MegaUltraPDF\Capabilities\FileAssociations" ".pdf" "MegaUltraPDF.Document"
  WriteRegStr SHCTX "Software\RegisteredApplications" "MegaUltraPDF" "Software\MegaUltraPDF\Capabilities"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue SHCTX "Software\RegisteredApplications" "MegaUltraPDF"
  DeleteRegKey SHCTX "Software\MegaUltraPDF\Capabilities"
  DeleteRegKey /ifempty SHCTX "Software\MegaUltraPDF"
!macroend
