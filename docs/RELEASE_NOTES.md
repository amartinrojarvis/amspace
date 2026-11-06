AMSpace 0.1.1 — correcciones de foco y Mission Control.

- Corregido el modo del run loop del temporizador: el seguimiento del cursor ahora se ejecuta en el ciclo normal de macOS.
- Los ajustes abiertos en segundo plano ya no bloquean el foco automático.
- La app de destino se activa antes de elevar y enfocar su ventana.
- El borde usa el comportamiento transitorio de AppKit para ocultarse en Mission Control y reaparecer al volver al escritorio.
- Prueba de integración del temporizador: reproduce el fallo anterior, comprueba su ejecución real y su desactivación.

El foco se activa al detener el cursor unos 240–350 ms sobre una ventana; se pausa con ⌥, al arrastrar y al trabajar en los ajustes de AMSpace.

Paquete universal Apple Silicon + Intel, firmado ad hoc. Tras sustituir una compilación, macOS puede requerir eliminar AMSpace de Accesibilidad y añadir de nuevo la copia de Aplicaciones.
