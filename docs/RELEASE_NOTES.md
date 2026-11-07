AMSpace 0.6.8 — estabilidad al cambiar de escritorio con varias pantallas.

- Conserva la distribución y el estado de colocación de las pantallas cuyo escritorio no cambia.
- Limita a la pantalla afectada los reajustes por cambios de área útil del Dock o la barra de menús.
- Usa la geometría de Accesibilidad para decidir y verificar colocaciones, evitando reajustes por posiciones transitorias del compositor durante la animación de escritorio.
- Comprueba de nuevo los escritorios visibles antes de reanudar colocaciones pendientes.

Validación: 92 pruebas automáticas correctas, incluida una regresión con pantalla apaisada y vertical en escritorios independientes. Pendiente de validación física del incidente en el Mac con dos pantallas.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel; ejecución local comprobada en Apple Silicon en versiones anteriores.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
