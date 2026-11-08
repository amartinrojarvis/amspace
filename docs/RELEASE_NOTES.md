AMSpace 0.6.15 — consultas más ligeras y menos asignaciones.

- Consulta por Accesibilidad la ventana enfocada únicamente cuando sirve para insertar nuevas ventanas en una distribución personalizada. Los escritorios sin cambios y las plantillas que no usan ese dato evitan esa consulta.
- Comprueba las reglas de aplicaciones flotantes mediante una búsqueda directa, sin reconstruir ni ordenar la lista completa. Conserva las preferencias y su edición.
- Reutiliza las cadenas constantes de los atributos habituales de Accesibilidad, manteniendo la ruta normal para atributos desconocidos.

134 pruebas automáticas correctas, incluidas equivalencias de inserción/cierre en las ocho plantillas, reglas flotantes y las regresiones previas de pantallas, escritorios, menús y diálogos. Los benchmarks y su alcance se detallan en docs/VALIDATION.md.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
