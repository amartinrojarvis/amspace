AMSpace 0.9.31 — Borde, pestañas e indicador más ligeros.

- El borde evita consultas al sistema para ventanas que no pueden taparlo. El mosaico reutiliza los botones de pestaña cuando no cambian y evita copiar la distribución. El indicador deja de preparar el texto de Ajustes antes de abrir esta vista.
- Las lecturas de posición y tamaño de las ventanas se agrupan para reducir las llamadas de Accesibilidad. Se conserva la frecuencia de actualización y el gesto de retorno de flotantes al mosaico.

English

- The active border skips system queries for windows that cannot cover it. Tiling reuses unchanged tab controls without copying the layout. The menu bar indicator skips preparing Settings text until the view is opened.
- Window position and size reads are batched to reduce Accessibility calls. Update frequency and the gesture for returning floating windows to tiling are unchanged.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
