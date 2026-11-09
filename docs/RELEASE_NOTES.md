AMSpace 0.8.4 — un único ajuste por ventana al volver a un escritorio.

- Al regresar de otro escritorio, una ventana podía mostrar dos ajustes seguidos. Dos causas, ambas corregidas:
  - Durante el deslizamiento nativo la lista del compositor llega a veces incompleta en el primer escaneo. Si la membresía es un prefijo exacto del orden guardado, ese orden se reutiliza y la distribución no cambia (introducido en 0.8.3).
  - El destino ya comprometido no se consideraba "pendiente" mientras el Space asentaba: la verificación podía descartarse antes del siguiente escaneo y la misma geometría volvía a escribirse. Ahora, durante la ventana acotada de asentamiento, el destino ya aplicado cuenta como pendiente y no se reescribe; la reparación por deriva real sigue activa en cuanto expira esa ventana.
- Resultado: cada ventana conserva su hueco con una sola escritura; las ocho plantillas se comportan igual, con rejilla activada o no. Una ventana realmente cerrada sigue colapsando su nodo con normalidad.
- El indicador y los atajos ⌘1–9 siguen la numeración del monitor que contiene el puntero (desde 0.8.2).

192 pruebas internas correctas, con dos regresiones nuevas para el asentamiento (membresía parcial en las ocho plantillas y ausencia de segunda escritura). Pendiente la comprobación física en el equipo con dos monitores (uno vertical). Paquete universal para Apple Silicon e Intel. Firma ad hoc, sin notarización de Apple. macOS 13 o posterior; las funciones privadas dependen de su disponibilidad real.
