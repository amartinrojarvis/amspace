AMSpace 0.6.14 — previsualización estable y arrastre más ligero.

- Corrige el parpadeo del recuadro azul al mover ventanas con la previsualización de redimensionamiento desactivada. Un único recorrido controla su visibilidad; el redimensionamiento ya no oculta el indicador del arrastre.
- Evita animaciones implícitas y operaciones repetidas de mostrar/ocultar los paneles. Una breve tolerancia al pasar por separaciones evita destellos; al soltar se valida siempre el destino actual.
- Reutiliza los huecos y las propuestas mientras el destino no cambia, en lugar de recalcular varias veces la misma distribución. La caché es limitada y se descarta cuando cambia el contexto o termina el gesto.
- Evita consultar por Accesibilidad la ventana enfocada al iniciar el arrastre. Tras identificar el gesto, deja de consultar su geometría nativa en cada muestra del movimiento.
- Conserva la previsualización de redimensionamiento opcional y apagada por defecto, las proporciones ajustadas y las verificaciones al soltar.

131 pruebas automáticas correctas. Se comprueba la invalidación de la caché por cambios de plantilla, orden, tamaños, rejilla, mínimos, pantalla y escritorio. Incluye las matrices de intercambios y geometría de las versiones anteriores.
Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.
La firma ad hoc puede requerir renovar Accesibilidad tras actualizar.
