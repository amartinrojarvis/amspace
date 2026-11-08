AMSpace 0.6.16 — estabilidad al entrar en otro escritorio.

- Espera a disponer de las ventanas del escritorio de destino y de geometría estable antes de reanudar el mosaico. Una llegada incompleta ya no se trata como el cierre de una ventana.
- Conserva los huecos, proporciones y distribución existentes durante la transición, incluso si macOS tarda más que el plazo anterior.
- Descarta movimientos y verificaciones pendientes anteriores al cambio para no reutilizar posiciones transitorias.
- Distingue ventanas que aún están llegando de ventanas ocultas, minimizadas y pestañas nativas inactivas. Recupera referencias de Accesibilidad caducadas por identidad de ventana.

144 pruebas automáticas correctas. Regresiones de las ocho plantillas en ambas orientaciones, con y sin rejilla, cambios rápidos de escritorio y varias pantallas. Los diagnósticos nativos y los límites de la comprobación se detallan en docs/VALIDATION.md.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
