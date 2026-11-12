AMSpace 0.9.11 — estabilidad y menor trabajo innecesario al gestionar ventanas.

- Cambiar o restaurar una distribución cancela también las colocaciones pendientes de pestañas ocultas, evitando que reciban geometría anterior.
- Las plantillas personales se previsualizan con los grupos, mínimos y rejilla actuales. El selector cuenta huecos, no ventanas individuales.
- Restaurar ventanas al desactivar Mosaico comprueba de nuevo su identidad, escritorio y pantalla.
- Un redimensionado con coordenadas no válidas se rechaza completo, sin modificar otras divisiones.
- Menos enumeraciones innecesarias en reposo. Durante arrastres sin cambios de grupo o tamaño se reutilizan títulos, iconos y controles; mover el indicador no reconstruye su contenido.
- Se conservan el cierre individual, la separación por arrastre y la presentación exterior de pestañas. Sin nuevos temporizadores ni sondeo adicional.

Verificación: 227 pruebas correctas, controles AppKit reales y compilación universal. Una muestra aislada del diagnóstico en reposo registró 0,0 % de CPU durante 20,46 s; no acredita coste cero ni mide todas las funciones, GPU/WindowServer o arrastre continuo. Las pruebas AX con aplicaciones ajenas siguen pendientes del permiso de Accesibilidad.

Paquete universal Apple Silicon/Intel; compilar Intel no acredita ejecución física en Intel o múltiples monitores. Firma ad hoc, sin Developer ID ni notarización. macOS 13 o posterior.
