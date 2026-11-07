AMSpace 0.6.3 — estabilidad de ventanas, borde y animaciones.

- Evita reintentos de colocación innecesarios al activar aplicaciones y distingue mejor los clics de los arrastres reales.
- Conserva el mosaico durante los cambios de escritorio y cancela escrituras de geometría anteriores mientras termina la transición nativa.
- Refuerza los traslados entre pantallas horizontales y verticales, la estabilidad en bordes compartidos y la restauración de ventanas flotantes en el monitor de destino.
- Abrir el selector de plantillas actualiza la previsualización sin colocar ventanas; confirmar cancela los destinos pendientes del reparto anterior.
- Reduce las rutas que pueden provocar parpadeo del borde: evita repintados y reordenaciones redundantes e ignora eventos del propio panel y ventanas auxiliares no estándar.
- Mejora la regularidad de las animaciones con inicio sincronizado, recuperación de escrituras intermedias fallidas y menos trabajo de Accesibilidad durante los fotogramas.
- Tolera mejor lecturas temporales incompletas de ventanas y cambios de configuración de pantallas.

Validación: 81 pruebas correctas en desarrollo y modo optimizado, incluida una matriz de 1.152 escenarios y 27.648 pasos del modelo. Pendiente de comprobación física del parpadeo específico de Mullvad y de monitores con escalas y frecuencias distintas; no se han medido FPS en esos equipos.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
