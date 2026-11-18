AMSpace 0.9.29 — Conserva la ventana al arrastrarla de vuelta al mosaico.

- Si macOS entrega tarde el clic con el identificador de la ventana pero sin el del proceso, AMSpace recupera el propietario de esa ventana en lugar de buscar otra bajo el punto inicial. Esto corrige un caso en el que no aparecía la zona para volver al mosaico.

English

- When macOS delivers a late click with a window ID but no process ID, AMSpace looks up that window’s owner instead of looking for another window at the original click position. This fixes one case where the return-to-tiling drop zone did not appear.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
