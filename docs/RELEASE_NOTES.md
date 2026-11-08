AMSpace 0.6.13 — intercambios entre filas y previsualización opcional.

- Corrige el bloqueo al intercambiar ventanas con una fila central más pequeña en División en 3. Los huecos conservan sus tamaños cuando las ventanas caben; si un mínimo exige más espacio, se ajustan las divisiones compartidas en lugar de rechazar un intercambio viable.
- Aplica el mismo criterio a las ocho plantillas, manteniendo su estructura, la rejilla y el centrado de las principales. Los destinos que realmente no caben siguen rechazándose sin modificar la distribución.
- Corrige errores de redondeo al ajustar mínimos en Espiral y vuelve a ajustar los laterales tras centrar una plantilla con ventanas de mínimos distintos.
- Añade «Previsualizar al redimensionar» en Ajustes → Mosaico, desactivado por defecto. Sin activarlo no se muestra la plantilla azul sobre las ventanas al cambiar su tamaño. El indicador de destino al mover ventanas se conserva.
- Con la previsualización desactivada, evita recalcular la distribución durante cada muestra del redimensionamiento nativo.

127 pruebas automáticas correctas. Incluyen las seis direcciones de intercambio de tres filas, las seis ordenaciones y sus transiciones, y 6.672 parejas de intercambio en las ocho plantillas, ambas orientaciones y varias rejillas con mínimos distintos por ventana.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
