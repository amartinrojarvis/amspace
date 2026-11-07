AMSpace 0.6.6 — tamaños mínimos de Brave y Spark.

- Detecta los tamaños mínimos que las aplicaciones imponen al rechazar un ajuste del mosaico.
- Redistribuye las ventanas en columnas o filas cuando esos límites provocarían solapamientos, conservando la plantilla elegida cuando cabe.
- Amplía el diagnóstico de ventanas para comprobar su disponibilidad y geometría de Accesibilidad.

Validación: 90 pruebas automáticas correctas y prueba local con Brave, Spark y Ghostty. Reproducido el mínimo de 700 píxeles de alto de Spark y verificada la redistribución sin solapamientos, incluida la recuperación del espacio al cerrar la ventana de prueba. Si los tamaños mínimos de todas las ventanas superan el espacio disponible, sigue siendo necesario reducir el número de ventanas o usar flotantes.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel; ejecución local comprobada en Apple Silicon.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
