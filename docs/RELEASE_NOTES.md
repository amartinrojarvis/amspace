AMSpace 0.9.15 — pestañas persistentes y reordenación por arrastre.

- Las barras conservan sus paneles y controles en el escritorio del grupo, en lugar de retirarlos al cambiar de Space. macOS gestiona su visibilidad con pertenencia nativa comprobada.
- Arrastra dentro de la barra para reordenar. Una marca indica la posición de inserción; la ventana activa, sus tamaños y la distribución se mantienen.
- Arrastra fuera de la barra para separar la ventana. Si no cabe un hueco independiente, puedes seguir reordenando las pestañas.
- El gesto respeta el desplazamiento horizontal. Cancelar, cambiar de contexto o perder la entrega final no modifica el grupo.
- Sin nuevos temporizadores ni sondeo permanente. Pausar Mosaico, Mostrar escritorio y Mission Control mantienen la ocultación de las barras.

Se conservan las preferencias y el canal firmado de actualizaciones. Si macOS no permite confirmar la pertenencia del panel, se conserva la ocultación segura durante la transición.

Universal Apple Silicon/Intel, macOS 13 o posterior. Firma ad hoc, sin Developer ID ni notarización. Pruebas del modelo y controles AppKit; no acreditan la animación física entre escritorios ni un arrastre completo con otras apps. Comprobaciones: https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
