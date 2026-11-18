AMSpace 0.9.26 — Inicio, foco y arrastre de ventanas.

- Conceder Accesibilidad reactiva las funciones habilitadas y programa Mosaico sin esperar a mover, abrir o cerrar una ventana. Se conservan la pausa y las preferencias de activación.
- El arrastre de ventanas conserva el destinatario del evento nativo incluso si AppKit entrega su copia cuando la ventana ya se ha movido. No se confunde con otra ventana situada en el punto antiguo.
- El foco bajo el cursor acepta ventanas normales corroboradas por WindowServer aunque su aplicación omita el subrol AX estándar. Consulta el foco AX real, no una selección anterior, y conserva los reintentos ante respuestas transitorias.
- Desactivar el foco cancela su activación pendiente. La suspensión no pierde su notificación durante reentrada AX y cancela cambios de escritorio y traslados pendientes; al despertar se reconcilia el estado nativo.
- La navegación desde el indicador y el selector de plantillas respetan los traslados en curso. Los cambios de Mosaico rechazados durante un traslado restauran el valor visible del control. Las animaciones comprueban la presentación antes de cada escritura AX.
- Los destinos incompatibles explican durante el arrastre su límite de pestañas o tamaño mínimo, sin resaltarse como válidos.
- «Colocar en…», en el menú de cada pestaña, permite elegir cualquier posición independiente final. Las posiciones incompatibles están deshabilitadas con su motivo; un menú obsoleto no aplica otro destino.
- El asa visible y los cursores de mano abierta/cerrada hacen reconocible el arrastre sin invadir el botón de cierre.
- Mantener ⌥ durante el arrastre de una pestaña fuerza colocación independiente incluso en el tercio superior. Soltar ⌥ restaura la agrupación; decide el modificador del evento final. La reordenación y «Hacer flotante» mantienen sus reglas.

English

- Granting Accessibility reactivates enabled features and schedules Tiling without moving, opening or closing another window. Pause and activation preferences are preserved.
- Window drags retain the native event recipient even when AppKit delivers its copy after the window has moved. Another window at the old point is not mistaken for the source.
- Pointer focus accepts normal windows corroborated by WindowServer even when an application omits the standard AX subrole. It checks actual AX focus rather than a previous selection and retains retries after transient replies.
- Disabling pointer focus cancels pending activation. Sleep notifications survive AX reentry and cancel pending desktop switches and transfers; wake reconciles native state.
- Indicator navigation and the layout picker respect transfers in progress. Tiling edits rejected during a transfer restore the control's visible value. Animations check presentation before each AX write.
- Incompatible drop destinations explain their tab limit or minimum size during the drag, without highlighting them as valid.
- “Place in…” in each tab's menu chooses any final independent position. Incompatible positions are disabled with their reason; a stale menu does not apply another destination.
- A visible grip and open/closed hand cursors make dragging discoverable without encroaching on the close button.
- Holding ⌥ while dragging a tab forces independent placement even in the top third. Releasing ⌥ restores grouping; the final event's modifier decides. Reordering and “Make floating” retain their rules.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
