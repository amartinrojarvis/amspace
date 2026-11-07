AMSpace 0.6.5 — corrección del borde y del foco.

- Evita reintentos de colocación innecesarios al activar aplicaciones y distingue mejor los clics de los arrastres reales.
- Conserva el mosaico durante los cambios de escritorio y cancela escrituras de geometría anteriores mientras termina la transición nativa.
- Refuerza los traslados entre pantallas horizontales y verticales, la estabilidad en bordes compartidos y la restauración de ventanas flotantes en el monitor de destino.
- Abrir el selector de plantillas actualiza la previsualización sin colocar ventanas; confirmar cancela los destinos pendientes del reparto anterior.
- El borde pasa a ser un panel pasivo con recorte por oclusión: no ordena su panel respecto a ventanas ajenas ni necesita elevarlas para dibujarse.
- Conserva ventanas reales de navegador con roles de Accesibilidad desconocidos; excluye únicamente roles auxiliares reconocidos.
- Omite activaciones solo cuando coincide la ventana concreta previamente activada, sin confiar únicamente en `AXFocusedWindow`. Los cambios entre ventanas de la misma app reciben una activación específica de ventana, incluso si AX informa mal del foco anterior. Cancelar una entrega no reactiva la ventana anterior; cambiar de aplicación, mover el cursor o cambiar la superficie bajo él cancela la entrega pendiente.
- Corrige la selección con flotantes superpuestas: contrasta el ID señalado por Accesibilidad con la geometría visible, descarta ventanas de otros Spaces y vuelve a validar el objetivo antes de enfocarlo. Tolera lecturas temporales fallidas con un máximo de tres intentos por posición del cursor.
- Suaviza las transiciones de distribución ampliando su duración de 120 a 160 ms, manteniendo 60 Hz durante el movimiento y el ritmo reducido en reposo.
- Mejora la regularidad de las animaciones con inicio sincronizado, recuperación de escrituras intermedias fallidas y menos trabajo de Accesibilidad durante los fotogramas.
- Tolera mejor lecturas temporales incompletas de ventanas y cambios de configuración de pantallas.

Validación: 89 pruebas automáticas correctas y un diagnóstico nativo de solo lectura, incluidas máscaras con ventanas superpuestas y una matriz de 1.152 escenarios y 27.648 pasos del modelo. El usuario confirmó la corrección del foco y del destello del borde en Finder con una flotante sobre tiles; las combinaciones físicas de monitores siguen pendientes. No se garantiza la ausencia universal de parpadeo ni se presenta el consumo de CPU elevado observado como resuelto.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
