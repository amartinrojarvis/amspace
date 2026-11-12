AMSpace 0.9.9 — cerrar ventanas desde las pestañas.

- Cada pestaña incorpora una × para solicitar el cierre de su ventana mediante el botón nativo de la aplicación.
- El cierre no fuerza la salida ni descarta documentos. La pestaña permanece hasta confirmar que la ventana se ha cerrado; los avisos de guardar quedan bajo control de la aplicación.
- Una pestaña oculta se selecciona antes de solicitar el cierre para mostrar cualquier confirmación. Los diálogos protegidos conservan el foco.
- Cerrar, seleccionar y arrastrar tienen áreas independientes. Soltar fuera de la × cancela sin ejecutar otra acción.
- Se mantienen la barra exterior, la separación por arrastre y el máximo de pestañas configurable de 0.9.8.

Verificación: 224 pruebas correctas, controles AppKit y compilación universal. El cierre AX real y los diálogos de guardar de otras aplicaciones siguen pendientes de prueba física con Accesibilidad autorizada.

Las agrupaciones son estado de sesión. Paquete universal Apple Silicon/Intel; compilar Intel no acredita ejecución física en Intel. Firma ad hoc, sin Developer ID ni notarización. macOS 13 o posterior. Las ideas de reordenación, menú contextual, atajos y pestañas fijadas no forman parte de esta versión.
