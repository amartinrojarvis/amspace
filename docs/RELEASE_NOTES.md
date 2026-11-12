AMSpace 0.9.8 — separar pestañas y barra exterior.

- Arrastra cualquier pestaña y suelta para convertirla en una ventana independiente dentro del mosaico. La previsualización muestra el destino; cancelar conserva la agrupación.
- Barra fina de 22 puntos, centrada por encima de la ventana, con fondo oscuro translúcido y separación de 4 puntos. Reserva su espacio sin tapar controles de las aplicaciones.
- Máximo de pestañas por grupo configurable en Ajustes → Mosaico: 5 por defecto, entre 2 y 20. Reducirlo no deshace los grupos existentes.
- Los movimientos menores de 6 puntos siguen siendo clics. Cancelar un arrastre no genera una selección accidental ni bloquea los clics posteriores.
- Cambiar de plantilla conserva los grupos y su pestaña activa. Al quedar una sola ventana se recupera el espacio de la barra.

Verificación: 222 pruebas correctas, controles AppKit y ajustes comprobados, compilación universal. Sigue pendiente la prueba completa sobre ventanas de otras aplicaciones con Accesibilidad autorizada; no se acredita arrastre físico ni varios monitores reales.

Las agrupaciones son estado de sesión. Paquete universal Apple Silicon/Intel; compilar Intel no acredita ejecución física en Intel. Firma ad hoc, sin Developer ID ni notarización. macOS 13 o posterior. Las funciones privadas dependen de su disponibilidad real.
