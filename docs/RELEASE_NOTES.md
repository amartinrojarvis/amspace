AMSpace 0.9.7 — candidata, todavía no publicada.

- Corrige dos errores de propiedad de memoria en títulos e iconos de las tiras: el texto AX se copia antes de liberar su objeto y el icono prestado se retiene correctamente.
- Corrige el tipo Objective-C del puente `CGColor`, que provocaba un panic al dibujar la tira en compilaciones de depuración.
- Las pestañas ya son botones: seleccionan, enfocan y elevan su ventana tras comprobar su identidad y escritorio. Las tiras largas se desplazan horizontalmente sin desbordar el hueco.
- Los miembros del grupo reciben su geometría aunque la ventana activa ya esté colocada. Las selecciones externas se reflejan en el modelo sin crear otro hueco.
- Corrige el mantenimiento de grupos al separar, cerrar, intercambiar y sustituir pestañas nativas, preservando las divisiones ajustadas.
- La preparación de actualizaciones usa exclusivamente el Llavero, sin exportar la clave privada a archivos.

Publicación retenida hasta completar la prueba de interacción nativa con Accesibilidad. El render y las regresiones del modelo no demuestran la selección AX ni el arrastre físico.

Las agrupaciones son estado de sesión. El paquete es universal Apple Silicon/Intel; compilar Intel no acredita ejecución física en Intel. Firma ad hoc, sin Developer ID ni notarización. macOS 13 o posterior. Las funciones privadas dependen de su disponibilidad real. La evidencia ejecutada y los límites físicos están en docs/VALIDATION.md del repositorio.
