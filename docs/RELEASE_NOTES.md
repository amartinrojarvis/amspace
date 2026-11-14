AMSpace 0.9.20 — español/inglés y suelta fiable en la zona flotante.

- Interfaz completa en español e inglés, incluidas ayuda, feedback e instrucciones del instalador. Sigue el primer idioma compatible de macOS; usa inglés si no hay ninguno.
- Selector en Ajustes → General → Idioma. Guarda la elección para la próxima apertura, sin cambiar el idioma del sistema ni los nombres, plantillas o preferencias personales.
- Corrige dos carreras que podían perder la suelta en «Hacer flotante» o «Volver al mosaico»: el temporizador ya no consume el gesto antes del evento y las reentradas de AppKit conservan los eventos pendientes.
- La confirmación usa la posición real al soltar. Los eventos caducados o desbordados cancelan sin ejecutar una acción antigua. Sin nuevos temporizadores permanentes ni dependencias.

English

- Full Spanish/English interface, including help, feedback and installer instructions. Follows the first supported macOS language, with English as the fallback.
- Choose the language in Settings → General → Language; reopen the app to apply it. System language, personal names, templates and existing preferences remain unchanged.
- Fixes two races that could lose releases over “Make floating” or “Return to tiling”. Confirmation uses the actual release position; stale or overflowing events cancel safely, without a new idle timer.

Verificación / Verification: 273 pruebas correctas / tests passed, ocho ignoradas / eight ignored; controles AppKit en ambos idiomas / AppKit controls in both languages; compilación universal / universal build. No se ha comprobado el arrastre físico completo con ventanas ajenas ni la ejecución en hardware Intel / Full physical dragging of other apps’ windows and execution on Intel hardware have not been verified.

Universal Apple Silicon/Intel, macOS 13+. Firma ad hoc, sin Developer ID ni notarización / Ad hoc signed, no Developer ID or notarization.
https://github.com/amartinrojarvis/amspace/blob/main/docs/VALIDATION.md
