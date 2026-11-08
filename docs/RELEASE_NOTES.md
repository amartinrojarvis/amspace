AMSpace 0.6.11 — estabilidad del monitor secundario al cambiar de escritorio.

- Usa la geometría AX también para decidir a qué monitor pertenece una ventana. Los desplazamientos temporales de WindowServer durante el cambio de Space ya no retiran una ventana de su mosaico por quedar momentáneamente fuera del área del monitor.
- Conserva temporalmente el hueco de una ventana gestionada cuando su lectura de geometría es incierta, sin recolocarla durante esa incertidumbre. Los cambios nativos de Space confirmados y las transferencias reales entre monitores siguen prevaleciendo.
- Reutiliza la misma lectura de geometría para pertenencia y colocación, sin añadir sondeo ni ampliar las pausas.
- Incluye las mejoras de consumo, tamaño y protección del foco de la 0.6.10.

Validación: 99 pruebas automáticas correctas, formato, Clippy y compilación universal verificados. Regresión con monitor principal horizontal y secundario vertical, dos ventanas en la plantilla de tres filas; también se comprueban Spaces compartidos y traslados reales entre monitores. La reproducción es automatizada en el modelo; pendiente de confirmación física en el Mac afectado.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
