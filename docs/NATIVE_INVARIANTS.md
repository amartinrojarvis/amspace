# Contratos nativos y regresiones

Esta guía recoge decisiones de implementación y fallos observados hasta 0.7.0. Su finalidad es que una solución aparentemente más simple no vuelva a introducirlos. El código y las pruebas definen los detalles actuales; consulta también [ARCHITECTURE.md](ARCHITECTURE.md) y [VALIDATION.md](VALIDATION.md).

## Identidad, coordenadas y pertenencia

- Comprueba PID e ID antes de escribir a una ventana. Los IDs pueden reutilizarse; un título no es una identidad.
- El mosaico pertenece a `(DisplayID, SpaceID)`. Con Spaces compartidos, el mismo ID nativo existe en varios monitores: la geometría y el solapamiento determinan el propietario físico.
- WindowServer usa coordenadas globales con origen superior izquierdo; AppKit usa origen inferior izquierdo. Convierte respecto a la pantalla principal, no a la secundaria. No interpretes un desplazamiento de la animación de Space como un arrastre real de la app.
- Un fallo al consultar ventanas no significa que el escritorio esté vacío. Una entrada nativa de capa cero tampoco demuestra que sea una ventana estándar redimensionable.
- `CGWindowIsOnscreen` y una lista de ventanas visibles no confirman por sí solos que la presentación real haya cambiado. Pueden conservar entradas de otro Space. La confirmación del traslado usa identidad y pertenencia nativa; la prueba visual sigue teniendo un alcance distinto.

## Cambiar de escritorio no es cambiar un metadato

Una candidata de 0.7.0 intentó una selección directa con una operación privada de Space. El usuario observó que cambiaban los números de la barra, pero seguía viendo el mismo escritorio. Esa ruta se retiró.

`space_switch.rs` conserva los gestos nativos asíncronos, respeta desplazamiento natural y confirma cada paso. Un destino lejano puede pasar por escritorios intermedios. No reintroduzcas un setter que solo haga coincidir el indicador con el destino ni declares éxito al enviar un evento.

El puente de **traslado de ventanas** es otra operación: `window_move.rs` construye `SLSBridgedMoveWindowsToManagedSpaceOperation` y la presenta mediante la entrada asíncrona que resuelve `sky_bridge.rs`. Invocar directamente `performWithWMBridgeDelegate` no fue suficiente en la investigación. Resolver el símbolo exacto no garantiza que cada operación sea aceptada: hay que confirmar su resultado.

`sky_bridge` lee la tabla Mach-O de la imagen SkyLight cargada, valida límites y dirección ejecutable y devuelve ausencia si no encuentra una entrada válida. No escanea instrucciones, parchea memoria, inyecta código ni desactiva SIP. El antecedente de esta técnica está en yabai; su licencia se distribuye en [YABAI-LICENSE.txt](YABAI-LICENSE.txt). No simplifiques los límites de `__LINKEDIT`: la caché compartida de dyld puede tener una tabla de cadenas mucho mayor que un binario corriente.

## El traslado tiene preparación, petición y confirmación

1. Capturar ventana activa, identidad, geometría AX/WindowServer y contexto visible real. Rechazar fullscreen, modales, minimización, pertenencia ambigua, entrada segura o ratón pulsado.
2. Resolver el destino por IDs reales en ese momento. Preparar una copia del layout y comprobar mínimos sin confirmar ni destruir el origen.
3. Actualizar la pertenencia del destino, incluso si había caché, conservando plantilla y divisiones. No vetar todo el envío porque una entrada auxiliar u oculta no tenga ventana AX verificable.
4. Solicitar una sola vez el traslado nativo. Confirmar identidad y pertenencia con dos observaciones consecutivas, con plazo global de dos segundos. Los reintentos de geometría no son reenvíos ciegos de Space.
5. Colocar la propuesta del destino oculto sin activarlo. Confirmar el modelo después de la llegada; liberar la hoja del origen y conservar flotantes/restauración. Si falla, reconciliar con la ubicación nativa real, sin activar una propuesta no confirmada ni hacer rollback nativo a ciegas.

La configuración de pantallas participantes se captura al iniciar. Rotación, posición, área útil, escala o desaparición invalidan las coordenadas preparadas. Un cambio de orden de enumeración no equivale a un cambio físico. Se usan las áreas validadas del plan hasta su cancelación o confirmación.

No deben quedar animaciones, verificaciones o gestos anteriores escribiendo al origen mientras se confirma un nuevo destino. La retención breve de monitor tolera geometría nativa retrasada, pero no puede anular otro Space confirmado o recuperar una pantalla desconectada.

## Colocación en segundo plano y AX

Primero se preparaba el modelo oculto y se aplicaba geometría al entrar; el usuario veía la ventana llegar con su tamaño anterior y reajustarse después. Ahora se colocan también las ventanas verificables del destino mientras permanece oculto.

Las referencias AX se retienen antes de trasladar: algunas apps omiten ventanas ocultas de `AXWindows`. Retener una referencia no autoriza escribir indefinidamente: comprobar propietario, ID, Space y flags antes de cada colocación. No activar, elevar ni seleccionar el escritorio para conseguir el tamaño deseado.

La colocación oculta tiene dos intentos separados al menos 120 ms y un límite de 600 ms; si el destino se hace visible se detiene esa ruta. Las apps que no aceptan geometría oculta se ajustan mediante la reconciliación habitual al entrar. Es una alternativa deliberada, no permiso para reintentar permanentemente.

Otro fallo real bloqueaba el envío al escritorio 1 con «Una ventana del destino no responde; no se ha movido nada». La causa era tratar una entrada nativa sin AX verificable como un veto de toda la preparación. Se omiten entradas no verificables nuevas, se conservan las ranuras conocidas cuando corresponde y no se escriben geometrías de ventanas no comprobadas. No lo arregles reasignando el número 1 a otro Space ni eliminando todas las validaciones.

## Atajos: prioridad acotada y compatibilidad

`shortcuts.rs` centraliza configuración y conflictos. Carbon usa reservas exclusivas; la prioridad opcional usa un event tap de sesión con hilo/run loop dedicado. El callback no hace consultas AX ni coloca ventanas: encola trabajo en el hilo principal. La entrada segura y las decisiones del sistema pueden limitarlo; no existe una promesa de prioridad absoluta.

Se consumen únicamente combinaciones configuradas y sus liberaciones, se suprimen repeticiones y se descartan acciones de más de 350 ms o de una generación anterior. Conservar esas guardas al editar atajos mientras hay acciones encoladas. El timeout del tap puede reactivarlo; una desactivación explícita no se combate en bucle.

Los cambios de preferencias son transaccionales: si hay duplicados se restaura el valor anterior, incluida la ausencia de una clave. No renumerar las entradas históricas de `KEYS`, `MODIFIERS`, acciones ni plantillas. Las sugerencias de Carbon no enumeran todos los atajos locales de otras apps. La serie de envío predeterminada de 0.7.0, ⌘⇧1–9, se solapa con atajos de captura del sistema. Antes de alterar el puente nativo, comprobar el estado de reservas/tap y probar otra combinación; ⌃⌘ es una alternativa configurable, no una garantía universal de ausencia de conflictos.

La comprobación física del usuario validó las combinaciones; la simulación de teclado usada durante el desarrollo no era una prueba equivalente. Un test del ciclo de vida del tap no sustituye pulsar el atajo en el entorno que presenta el conflicto.

## Mosaico, foco, overlays y ciclo de vida

- Mantener proporciones personalizadas y serialización por escritorio/pantalla. Cambiar una plantilla o cerrar una ventana debe invalidar solo el trabajo que corresponda, sin contaminar otros contextos.
- Separar arrastre real, redimensionado iniciado por un borde y cambio de tamaño automático al cruzar de monitor. Una animación propia o un clic inmóvil no son una nueva intención de arrastre.
- Las escrituras AX finales se verifican; una app puede aceptar la llamada y aplicar el cambio después, o imponer un mínimo. No convertir una respuesta intermedia o rechazada en una restricción permanente.
- `GeometrySession` comparte y restaura el estado original de `AXEnhancedUserInterface` de la app, también al cancelar. No dejar ese cambio activado tras una transición.
- Foco de teclado no implica `AXRaise`. Solo elevar cuando el usuario active esa función. La preview de escritorio es un panel no activable, transparente al ratón y sin captura del contenido; debe conservarse el clic para navegar.
- Mostrar escritorio, Mission Control, sueño, pérdida de permisos y cambios de pantalla cancelan o pausan el trabajo según su contexto. Volver de una presentación requiere reconciliar, no reutilizar a ciegas coordenadas pendientes.
- No mover ni ordenar ventanas ajenas para corregir la oclusión del borde. El borde adapta su propio panel y máscara; el mosaico y los traslados son las funciones que sí modifican geometría ajena mediante AX.

## Evidencia antes de afirmar compatibilidad

Las pruebas de modelo verifican plantillas, mínimos, aislamiento y transacciones. Las pruebas nativas con fixtures comprueban un contrato del sistema en ese equipo. Solo una prueba física en varias pantallas puede validar su compositor, escalas y comportamiento de apps reales. Documenta cuál de esas evidencias existe; mantén explícitos Intel real, macOS anteriores o configuraciones no disponibles.

No copies una implementación de un tercero sin revisar licencia y atribución. Las licencias ya incluidas para los protocolos de foco/gesto y Sparkle están enlazadas en [README.md](../README.md) y se copian al bundle mediante `scripts/bundle.sh`.

## Casos añadidos durante 0.8 (candidata)

Una consulta AX temporalmente vacía durante más de 650 ms no debe eliminar una ventana que continúa confirmada por PID, Space visible y geometría física. Mantener el hueco no permite escribir sobre una ventana incierta. Los casos sin confirmación conservan su plazo acotado y los cierres/traslados nativos siguen retirándose.

Cambiar el escritorio de otro monitor invalida un baseline de gesto antiguo, pero no el presupuesto de verificación de una ventana en un contexto que conservó Space y área. Reiniciar ese retry puede producir ajustes repetidos de una fila inferior con límites de tamaño. Las regresiones cubren diseños modificados y no solo la plantilla inicial. La comprobación física del caso concreto sigue siendo necesaria.

Las plantillas con nombre deben ser portables a IDs de ventana nuevos y no cambiar la rejilla global de otros monitores. Su aplicación requiere el mismo número de huecos y debe ser atómica ante datos inválidos, cambio de pantalla o mínimos incompatibles.
