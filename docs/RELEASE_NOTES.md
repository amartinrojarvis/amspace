AMSpace 0.9.21 — cierre fiable de pestañas y transiciones más robustas.

- Corrige pestañas fantasma tras cerrar ventanas cuyo ID y geometría macOS seguía conservando. La retirada exige confirmación nativa y AX; minimizar, ocultar o vetar el cierre no elimina miembros.
- Los clics de selección y cierre sobreviven a reentradas de Accesibilidad. Las acciones caducadas, pausadas o pertenecientes a controles retirados no se ejecutan después.
- Conserva las notificaciones de «Mostrar escritorio» durante reentrada y descarta transiciones obsoletas.
- Evita una división por cero al saturar determinados layouts con muchas ventanas. Sin nuevos temporizadores permanentes ni dependencias.
- Los enlaces de instalación de las guías apuntan siempre a la última versión publicada.

English

- Fixes ghost tabs after closing windows whose IDs and bounds remain retained by macOS. Removal requires native and Accessibility confirmation; minimizing, hiding or vetoing a close preserves group members.
- Tab selection and close clicks survive Accessibility reentry. Expired, paused or detached-control actions cannot execute later.
- Preserves Show Desktop notifications during reentry and discards obsolete transitions.
- Prevents division by zero when many windows exhaust a layout's available space. No new permanent timers or dependencies.
- Installation guides now link to the latest published version.

Verificación / Verification: 274 pruebas correctas / tests passed, ocho ignoradas / eight ignored; cierre AX y controles AppKit con ventanas desechables / AX closing and AppKit controls using disposable windows; compilación universal / universal build. No acredita todas las aplicaciones, carga prolongada ni ejecución física en Intel / Does not establish coverage of every app, prolonged workloads or execution on Intel hardware.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
