AMSpace 0.9.34 — Ventanas flotantes al usar Mostrar escritorio.

- Se corrige la detección de «Mostrar escritorio» si un monitor secundario de distinta altura aparece antes del principal en la lista del sistema.
- Las ventanas nuevas detectadas durante el regreso al mosaico conservan su estado flotante aunque macOS tarde en mostrarlas. No se mueven ni se redimensionan al incorporarlas.

English

- Show Desktop is detected correctly when a secondary display of a different height sorts before the main display.
- New windows detected during the return to tiling stay floating even if macOS takes longer to show them. They are not moved or resized on admission.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
