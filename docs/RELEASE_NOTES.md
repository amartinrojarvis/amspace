AMSpace 0.8.2 — el indicador y los atajos siguen la pantalla del puntero.

- Con Spaces independientes por pantalla, ⌘1–9 y los botones del indicador usan ahora la numeración del monitor que contiene el puntero. Antes se tomaba siempre la del monitor principal, porque `SLSGetActiveSpace` solo nombra esa pantalla: en un monitor secundario (por ejemplo, en vertical) los atajos no coincidían con sus escritorios.
- Orden de selección: pantalla del puntero, después la que posee el escritorio activo de WindowServer y, si ninguna resuelve, la primera entrada. La numeración sigue el orden de Mission Control de ese monitor; las ventanas a pantalla completa no cuentan como escritorios normales.
- El detalle del indicador nombra la pantalla (`Escritorio 2 de 3 · pantalla 2`) en lugar de la etiqueta fija `pantalla activa`. Las miniaturas, el mosaico por `(DisplayID, SpaceID)` y los traslados conservan su comportamiento.

188 pruebas internas correctas, con una nueva para la selección de pantalla del indicador. Pendiente la comprobación física en el equipo con dos monitores (uno vertical). Paquete universal para Apple Silicon e Intel. Firma ad hoc, sin notarización de Apple. macOS 13 o posterior; las funciones privadas dependen de su disponibilidad real.
