AMSpace 0.9.16 — corrección de la reaparición de pestañas y optimización del arranque.

- Corregida una ruta que volvía a ocultar las barras de pestañas durante los cambios de escritorio solicitados desde la barra de menús.
- Al volver de Mission Control, los paneles confirmados pueden reaparecer sin esperar a la reconciliación AX. Se conservan las protecciones de geometría, pausa, pertenencia y cambios de pantalla.
- El descubrimiento inicial reutiliza una lista AX de ventanas por aplicación y pasada, evitando enumeraciones repetidas y multiplicar los fallos de consulta.
- Corregida la cadencia inicial del mosaico; el puente de traslado no se resuelve si no hay atajos de envío que lo necesiten.
- Se mantienen la reordenación por arrastre, separación, cierre y preferencias existentes. Sin nuevos temporizadores ni sondeo permanente.

Verificado con pruebas del modelo y controles AppKit, incluida la restauración de un panel oculto antes de reconciliar. No se ha medido el arranque completo en la sesión del usuario ni verificado una transición física entre escritorios.

Universal Apple Silicon/Intel, macOS 13 o posterior. Firma ad hoc, sin Developer ID ni notarización. Comprobaciones: https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
