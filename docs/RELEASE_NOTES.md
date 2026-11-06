AMSpace 0.5.0 — mosaico inteligente de ventanas.

- Nuevo tiling opcional con cuatro plantillas: Dwindle, Espiral, Cuadrícula y Principal con secundarias. Cada monitor y escritorio mantiene su propia distribución durante la sesión.
- Colocación inteligente con ratón y ventana fantasma: los bordes de pantalla cambian la orientación, los bordes de otra ventana dividen su tile y el centro intercambia posiciones conservando tamaños.
- Redimensionado compartido: las ventanas vecinas se adaptan y las proporciones se conservan al intercambiar o cerrar ventanas.
- Ventanas flotantes con atajo configurable y lista de aplicaciones que se abren flotantes. Mover una flotante no la devuelve al mosaico.
- Rejilla virtual configurable, con guardado y restauración de proporciones para el mismo número de tiles.
- Adaptación a monitores verticales, coordenadas globales y distribuciones independientes incluso cuando comparten un Space de macOS.
- Animaciones de 120 ms con aceleración y deceleración suaves, temporizador adaptativo y menos consultas AX al cambiar de escritorio.
- Integración con el borde de ventana y el foco bajo el cursor existentes.

Requiere macOS 13 o posterior. Paquete universal para Apple Silicon e Intel.

Validación: 43 pruebas automatizadas y comprobación interactiva del usuario. Pendiente la validación física con varios monitores y la ejecución en hardware Intel. El guardado actual conserva geometría; todavía no restaura aplicaciones/documentos ni identifica monitores de forma portable.

La distribución ad hoc puede requerir eliminar AMSpace de Accesibilidad y añadir de nuevo la copia de Aplicaciones tras actualizar.
