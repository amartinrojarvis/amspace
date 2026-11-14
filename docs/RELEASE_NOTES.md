AMSpace 0.9.17 — flotantes persistentes y salida del mosaico más cómoda.

- Las ventanas abiertas durante «Mostrar escritorio» permanecen flotantes al volver al mosaico, incluidas las publicadas durante la reentrada.
- Las ventanas que ya eran flotantes conservan su estado; la herencia de pestañas nativas no las reinserta en un hueco tiled.
- Hacer flotante mediante el atajo o el menú centra la ventana en el área útil de su pantalla y adapta el tamaño para mejorar la lectura y dejar margen visible. Las flotantes automáticas no se recolocan.
- Se reutilizan las transiciones y la preferencia de animación existentes, sin nuevos temporizadores ni sondeo.

Verificado con 243 pruebas y paneles AppKit propios en las pantallas horizontal y vertical. Pendiente comprobar el gesto físico completo de «Mostrar escritorio» y las restricciones de tamaño de otras aplicaciones. La captura adicional de llegadas tardías es acotada: no se extiende a una reanudación manual posterior.

Universal Apple Silicon/Intel, macOS 13 o posterior. Firma ad hoc, sin Developer ID ni notarización. Comprobaciones: https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
