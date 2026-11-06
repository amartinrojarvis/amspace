AMSpace 0.6.0 — selector de distribuciones y estabilidad entre monitores.

- Nuevo selector semitransparente mediante ⌃⌥Espacio, configurable en Tiling → Plantillas y atajo…
- Previsualización en el monitor del cursor, selección con ratón, números o flechas, Intro para aplicar y Escape para cancelar.
- Seis plantillas: Dwindle, Espiral, Cuadrícula, Principal y divisiones de 3 o 4. Estas últimas muestran columnas en pantallas horizontales y filas en verticales.
- Preferencias de plantilla independientes por monitor y escritorio, también cuando comparten un Space.
- Corrige la interpretación de cambios automáticos de tamaño al trasladar ventanas a pantallas verticales.
- Estabiliza la pertenencia al monitor de destino ante posiciones atrasadas de WindowServer y evita animar el traslado entre pantallas.
- Verifica la geometría después de asentarse y permite un único reintento antes de registrar una restricción de la aplicación.

Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.

Validación: 58 pruebas automatizadas y revisión interactiva del selector. Pendiente la validación física de monitores horizontal y vertical y del atajo global con teclado físico. Finder mostró algunas discrepancias entre la geometría solicitada y la real; no se consideran resueltos todos los casos de colocación.

La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
