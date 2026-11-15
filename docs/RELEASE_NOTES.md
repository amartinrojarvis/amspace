AMSpace 0.9.24 — Arrastre de pestañas fiable y batería E2E.

- El inicio, movimiento y fin del arrastre sobreviven a la reentrada de AppKit durante consultas AX. Se conserva el control original y el punto final; los movimientos se fusionan en una cola fija, sin acumular trabajo.
- Arrastrar una pestaña hasta «Hacer flotante», en la pantalla de su pila, la separa con el tamaño y centrado existentes. Volver a la barra permite reordenar; salir, cancelar, pausar o desactivar la zona no confirma una acción antigua.
- Caducidad, desbordamiento y controles retirados cancelan el gesto de forma segura. Reconfigurar Mosaico también retira las intenciones y overlays pendientes.
- Batería E2E reproducible con ventanas desechables: ocho plantillas, selección/reordenación/separación, pestañas activas e inactivas, flotante y retorno, animaciones, cancelación, cierre y veto nativo, ajustes e idiomas. Los escenarios invasivos de Spaces son optativos y requieren cerrar la app normal; la batería nunca lo hace por sí misma.
- Verificado sobre el bundle universal: 279 pruebas de regresión, 58 escenarios nativos de arrastre/retorno más cierre, controles, Ajustes en español/inglés y ciclo de vida de atajos. Dominio persistente de preferencias idéntico antes y después.
- Sin dependencias nuevas, colas dinámicas ni temporizadores de entrega en reposo. Preferencias e identidades persistidas conservadas.

English

- Tab drag begin, motion and end survive AppKit reentry during AX queries. The original control and release point are retained; motion is coalesced in a fixed queue rather than accumulating work.
- Dragging a tab onto “Make floating” on its group's display detaches it using the existing floating size and centering. Returning to the strip reorders; leaving, cancelling, pausing or disabling the zone does not commit a stale action.
- Expiry, overflow and retired controls cancel safely. Reconfiguring Tiling also clears pending gesture intentions and overlays.
- Reproducible E2E suite using disposable windows: all eight layouts, selection/reordering/detachment, active and inactive tabs, float and return, animations, cancellation, close and native veto, settings and languages. Invasive Space scenarios are opt-in and require the normal app to be closed; the suite never closes it automatically.
- Verified against the universal bundle: 279 regression tests, 58 native drag/return scenarios plus close, controls, Spanish/English settings and shortcut lifecycle. The persistent preferences domain is identical before and after.
- No new dependencies, dynamic queues or idle delivery timers. Existing preferences and persisted identities are preserved.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
