AMSpace 0.8.5 — un único ajuste por ventana al volver a un escritorio.

- Al regresar de otro escritorio, una ventana podía mostrar dos ajustes seguidos. Tres causas, las tres corregidas:
  - Durante el deslizamiento nativo la lista del compositor llega a veces incompleta en el primer escaneo. Si la membresía es un prefijo exacto del orden guardado, ese orden se reutiliza y la distribución no cambia (introducido en 0.8.3).
  - El destino ya comprometido no se consideraba "pendiente" mientras el Space asentaba: la verificación podía descartarse antes del siguiente escaneo y la misma geometría volvía a escribirse (introducido en 0.8.4).
  - La propia verificación reescribía el rect ya comprometido cuando el compositor todavía informaba del marco anterior al deslizamiento. Ahora, mientras el Space asienta, un destino ya aplicado en esa verificación no se reescribe ni consume su reintento; la verificación continúa en el primer escaneo posterior a la ventana acotada, donde una deriva real se repara igual.
- Resultado: cada ventana conserva su hueco con una sola escritura; las ocho plantillas se comportan igual, con rejilla activada o no. Una ventana realmente cerrada sigue colapsando su nodo con normalidad.
- El indicador y los atajos ⌘1–9 siguen la numeración del monitor que contiene el puntero (desde 0.8.2).

193 pruebas internas correctas, con tres regresiones para el asentamiento (membresía parcial en las ocho plantillas, ausencia de segunda escritura y la tabla de la verificación durante el asentamiento). Pendiente la comprobación física en el equipo con dos monitores (uno vertical). Paquete universal para Apple Silicon e Intel. Firma ad hoc, sin notarización de Apple. macOS 13 o posterior; las funciones privadas dependen de su disponibilidad real.
