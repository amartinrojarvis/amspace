AMSpace 0.6.1 — colocación y animación del selector corregidas.

- Evita dejar ventanas a medio camino cuando falla una escritura intermedia de la animación: conserva y verifica el destino final.
- Los clics en el selector y los ajustes ya no cancelan las transiciones. Un movimiento o redimensionado real mantiene la prioridad del ratón.
- Coordina temporalmente la animación de Accesibilidad de cada aplicación para evitar que compita con la de AMSpace; restaura su estado al terminar o cancelar.
- Comparte esa coordinación entre las ventanas de una misma aplicación.
- Usa geometría final entera por bordes compartidos y reduce consultas AX durante cada fotograma.

Validación: 59 pruebas automatizadas. Prueba real con cuatro ventanas de Finder y una de Ghostty: las distribuciones de tres columnas y cuadrícula alcanzan las posiciones y tamaños previstos. Duración programada de 120 ms; no se han medido FPS ni consumo. Sigue pendiente la validación física con dos monitores de distinta orientación.

Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
