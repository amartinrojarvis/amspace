AMSpace 0.9.18 — agitar para alternar flotante y mosaico.

- Agita brevemente una ventana mientras la arrastras: un borde blanco confirma el gesto y, al soltar, alterna entre flotante y mosaico. Se puede desactivar en Ajustes → Mosaico.
- Reutiliza el tamaño flotante cómodo y las animaciones existentes. Si la ventana no cabe de vuelta en el mosaico, conserva su estado flotante y muestra un aviso.
- Los atajos numéricos de navegación controlan los escritorios de la pantalla principal aunque el foco esté en otra pantalla. El indicador, los clics y los envíos conservan sus contextos respectivos.
- Corregidas entregas de eventos perdidas durante reentrada, una liberación de objeto CF nulo y el arranque innecesario del temporizador al cambiar ajustes con mosaico desactivado.
- Menos cálculo redundante al incorporar ventanas a plantillas no personalizadas. El detector de agitación no asigna memoria ni añade un temporizador en reposo.
- La biblioteca de plantillas mantiene tarjetas legibles mediante desplazamiento nativo, incluso con 64 plantillas guardadas.

Verificado con 258 pruebas, compilación universal y controles AppKit propios, incluidas 12.000 solicitudes reentrantes. Pendiente comprobar el gesto físico completo con ventanas de otras aplicaciones y hardware Intel; las pruebas del modelo no sustituyen esa validación.

Universal Apple Silicon/Intel, macOS 13 o posterior. Firma ad hoc, sin Developer ID ni notarización. Comprobaciones: https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
