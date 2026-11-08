AMSpace 0.6.12 — arrastre y redimensionamiento más estables.

- Evita que el mosaico compita con el redimensionamiento nativo de macOS. Durante el gesto muestra los huecos previstos; al soltar aplica y verifica la distribución.
- Conserva las proporciones ajustadas y los tamaños de los huecos al intercambiar ventanas. Las variantes con principal centrada mantienen su simetría.
- Identifica la ventana que realmente se está moviendo o redimensionando aunque el foco siga en una vecina.
- Captura el inicio y el final del gesto y vuelve a calcular el destino al soltar, evitando conservar una posición anterior de la previsualización en arrastres rápidos.
- Al trasladar una ventana entre pantallas respeta la plantilla de destino, incluida Principal vertical, y comprueba los tamaños mínimos conocidos antes de confirmar el traslado.
- Correcciones comunes a todas las aplicaciones y a las ocho plantillas, sin excepciones por app.

125 pruebas automáticas correctas, incluidas matrices con ambas orientaciones, rejilla opcional, intercambios, tamaños ajustados y llegadas entre pantallas. El alcance de la validación se detalla en docs/VALIDATION.md.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
