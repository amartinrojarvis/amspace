AMSpace 0.9.50 — Mosaico más robusto y transiciones más fluidas.

- Una ventana que no cabe ya no impide colocar las demás. Si el mosaico de destino está lleno, puedes enviarla allí como flotante sin reorganizarlo.
- Las animaciones de movimiento y tamaño mantienen su cadencia mientras se verifica otra ventana. El borde evita repintados innecesarios y el motor abandona las comprobaciones que no se estabilizan.

English

- A window that does not fit no longer blocks other tiles. Sending a window to a full destination leaves it floating without rearranging that layout.
- Move and resize animations keep their cadence while another window is being verified. The border avoids unnecessary redraws, and unsettled checks no longer keep the engine running at high frequency indefinitely.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
