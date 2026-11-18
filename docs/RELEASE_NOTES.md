AMSpace 0.9.27 — Borde de ventana y atajos más ligeros.

- El borde activo usa la posición y el recorte de una misma lectura de WindowServer. Se oculta si la ventana desaparece, queda totalmente cubierta o no se puede confirmar su visibilidad.
- El recorte del borde reutiliza el mismo vector de regiones en vez de reconstruirlo para cada ventana superpuesta.
- Al configurar atajos normales, se consulta una vez la lista de reservas del sistema y se mantiene el registro exclusivo de cada combinación.

English

- The active border gets its position and clipping from the same WindowServer snapshot. It hides when the window goes away, is fully covered, or cannot be confirmed visible.
- Border clipping reuses a single region buffer instead of rebuilding it for every occluding window.
- Configuring standard shortcuts fetches the system reservation list once and keeps each shortcut registration exclusive.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
Pruebas realizadas y límites físicos / Executed checks and physical limits:
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
