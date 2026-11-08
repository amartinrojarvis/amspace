AMSpace 0.6.10 — menos consumo y protección del foco en contraseñas.

- Evita el bucle de repintado de los botones de la barra de menús mediante dibujo directo de sus títulos, conservando los botones nativos y sus acciones de accesibilidad.
- Reduce asignaciones de memoria al consultar ventanas y escritorios: reutiliza claves constantes, recorre listas nativas sin copiarlas y resuelve una sola vez la función de identidad AX.
- Elimina comprobaciones duplicadas de Accesibilidad y consultas de pantalla innecesarias durante el reposo del tiling, manteniendo sus intervalos de respuesta.
- Pausa el foco bajo el cursor durante la entrada segura de macOS y respeta campos de contraseña, diálogos modales y hojas de la aplicación activa. También protege las activaciones diferidas y sus reintentos.
- Reduce el tamaño del paquete aproximadamente un 7 % mediante compresión sin pérdidas del icono, conservando sus píxeles y todas sus resoluciones.

Validación: 97 pruebas automáticas correctas, formato, Clippy y compilación universal verificados. En una reproducción aislada de la barra de menús, el consumo pasó del 42 % al 0,04 % de un núcleo; el procesamiento sintético de metadatos mejoró un 73–78 %. Estas cifras no representan el consumo total de la app. Pendientes la prueba de uso de un diálogo real de autenticación y la validación visual y de acciones AX en la app completa.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
