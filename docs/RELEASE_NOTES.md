AMSpace 0.9.10 — pestañas integradas y alineación configurable.

- Pestañas alineadas a la izquierda por defecto, con 8 puntos de margen. Puedes centrarlas desde Ajustes → Mosaico → Alineación de pestañas.
- Fondo oscuro translúcido limitado a la tira, borde redondeado tenue, sombra nativa y mayor contraste para la pestaña activa.
- La × permanece visible en la activa y aparece al pasar el cursor por las demás, sin desplazar títulos ni cambiar el área de cierre.
- Se mantienen la barra exterior de 22 puntos, la separación por arrastre, el cierre normal de ventanas y el máximo de pestañas configurable.
- Seguimiento del cursor mediante eventos nativos, sin nuevos sondeos, temporizadores ni animaciones continuas. Se reutilizan las vistas y se eliminan vectores temporales de anchos.

Verificación: 224 pruebas correctas, controles AppKit, ajustes claros/oscuros y compilación universal. Una muestra aislada de 20,5 s en reposo registró 0,0 % de CPU a la resolución de muestreo; no acredita coste cero ni mide GPU/WindowServer o múltiples grupos. Sigue pendiente el cierre AX y los avisos de guardar de aplicaciones ajenas.

Paquete universal Apple Silicon/Intel; compilar Intel no acredita ejecución física en Intel. Firma ad hoc, sin Developer ID ni notarización. macOS 13 o posterior.
