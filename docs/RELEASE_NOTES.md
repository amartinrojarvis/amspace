AMSpace 0.6.7 — recuperación de ventanas en el mosaico.

- Renueva las referencias de Accesibilidad cuando dejan de ofrecer geometría y antes de reintentar un ajuste rechazado.
- Distingue los fallos de escritura de los límites reales de tamaño: un ajuste rechazado ya no bloquea permanentemente la colocación de la ventana ni genera un falso tamaño mínimo.
- Conserva la redistribución según los tamaños mínimos de Brave y Spark incorporada en 0.6.6.

Validación: 91 pruebas automáticas correctas. Prueba local con Ghostty y Spark: Ghostty recupera todo el espacio al minimizar Spark y ambas vuelven al mosaico al restaurarla, sin solapamientos. Esta revisión refuerza la recuperación tras fallos de Accesibilidad; no se ha identificado de forma concluyente el estado interno que causó el atasco anterior.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel; ejecución local comprobada en Apple Silicon.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
