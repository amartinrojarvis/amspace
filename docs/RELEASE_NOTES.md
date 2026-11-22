AMSpace 0.9.35 — Guardado de escenas de trabajo.

- Guardar y revisar escenas ya no dependen de que el mosaico termine de actualizar su estado interno, si macOS confirma que las ventanas siguen en su escritorio. Aplicar conserva esa protección antes de mover ventanas.
- El panel confirma el guardado sin iniciar una revisión automática y distingue los bloqueos por permisos, gestos o cambios de escritorio.

English

- Saving and reviewing scenes no longer wait for the tiling state to settle when macOS confirms the windows are still on their desktop. Applying still waits before moving windows.
- The panel confirms the save without starting a review and distinguishes permission, gesture, and desktop-transition blockers.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
