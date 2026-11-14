AMSpace 0.9.19 — zona superior para alternar flotante y mosaico.

- Arrastra una ventana al recuadro del centro superior de la pantalla: muestra «Hacer flotante» o «Volver al mosaico», se resalta al entrar y confirma únicamente al soltar dentro. Salir cancela esa acción.
- Sustituye la agitación por un destino explícito, con el mismo fondo oscuro translúcido, bordes redondeados y resaltado de las zonas de apilar/intercambiar.
- Conserva los tamaños flotantes cómodos y las animaciones existentes. Si la ventana no cabe de vuelta en el mosaico, permanece flotante y muestra un aviso.
- Desactivable en Ajustes → Mosaico → Zona superior: flotante / mosaico. Respeta la desactivación guardada del gesto anterior.
- Reutiliza panel y controles, sin nuevos temporizadores en reposo. Fuera del recuadro se conservan los destinos ordinarios de arrastre.

Verificación y límites: pruebas del modelo, compilación universal y controles AppKit propios. Pendiente validar el arrastre físico completo con ventanas de otras aplicaciones, varias pantallas reales y ejecución en hardware Intel.

Universal Apple Silicon/Intel, macOS 13 o posterior. Firma ad hoc, sin Developer ID ni notarización. Comprobaciones: https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
