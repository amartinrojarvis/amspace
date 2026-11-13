# Arquitectura

Para comenzar a contribuir: [guía de agentes](../AGENTS.md), [desarrollo y diagnóstico](DEVELOPMENT.md) y [contratos nativos y regresiones](NATIVE_INVARIANTS.md).

```text
NSStatusItem ─── clic izquierdo ─── cambio de escritorio
             └─ clic derecho ───── NSPanel de ajustes
     │                        │
     ├── NSWorkspace ── Spaces (SkyLight, lectura)
     │                        │
     ├── WindowServer ─ ID de ventana ─── panel de borde
     │                        │
     └── NSTimer opcional ── cursor quieto ── AX hit-test + foco
                              │
                         NSUserDefaults
```

## Módulos

| Módulo | Responsabilidad |
|---|---|
| `focus.rs` | Activación del teclado por ID de ventana, separada de AXRaise; protocolo documentado por yabai (MIT) |
| `space_switch.rs` | Cambio directo asíncrono mediante gestos nativos de escritorio |
| `space_style.rs` | Texto atribuido y SF Symbols; color activo/inactivo y preferencias por ID de escritorio |
| `model.rs` | Presentación del indicador y normalización de preferencias |
| `spaces.rs` | Resolución opcional de SkyLight, lectura y selección de pantalla (puntero, luego Space activo de WindowServer) |
| `accessibility.rs` | RAII de CoreFoundation, consultas AX, observador de foco y ciclo de vida, respaldo de geometría |
| `window_server.rs` | Identidad, orden, geometría y radios reales; notificaciones de movimiento, tamaño, creación y cierre |
| `native.rs` | Puente AppKit pequeño: vistas, texto y colores |
| `updater.rs` | Puente Rust a Sparkle, ciclo de actualización y búsqueda opcional |
| `ui.rs` | Ciclo de vida, ajustes, panel de borde, temporizador e ítems de inicio |
| `tiling.rs`, `tile_manager.rs` | Modelo de ocho plantillas y coordinación por pantalla/Space, gestos, mínimos, animación y restauración |
| `custom_templates.rs` | Biblioteca de plantillas con nombre, validación y persistencia por UUID |
| `tile_mouse.rs`, `tile_picker.rs`, `layout_thumbnail.rs`, `tile_rules.rs` | Intención de arrastre, selector, miniaturas y reglas de flotantes |
| `shortcuts.rs`, `tile_hotkey.rs` | Configuración transaccional, Carbon y tap prioritario |
| `window_move.rs`, `sky_bridge.rs` | Preparación, envío y confirmación de traslados nativos opcionales |
| `desktop_preview.rs`, `desktop_mode.rs` | Preview de geometría real y pausas por presentación del escritorio |
| `border.rs`, `bar_background.rs`, `instance.rs` | Dibujo del panel/fondo y exclusión de instancias |

## Seguridad de memoria y ciclo de vida

Todos los objetos de AppKit y las referencias AX se usan en el hilo principal. El tap de atajos utiliza un hilo propio únicamente para eventos y su run loop CF; encola las acciones de ventanas al hilo principal. Los objetos Objective-C propietarios usan `Retained`; AppKit retiene sus subvistas. Los punteros no propietarios a controles solo se usan mientras existe su ventana. Los resultados `Create` y `Copy` de CoreFoundation usan `Owned` y liberan su referencia en `Drop`.

El observador AX se elimina del run loop antes de liberarse o cambiar de ventana. Los temporizadores se invalidan al desactivar el foco o suspender la sesión. Los callbacks AX agrupan notificaciones en el siguiente turno del run loop, sin temporizador de 16 ms, para no modificar el observador mientras se ejecuta su callback. Los callbacks del compositor usan un ID atómico para mover el panel sin pasar por AppKit; los cambios de tamaño y foco se entregan al hilo principal mediante dispatch. La suscripción incluye todas las ventanas visibles de la app activa y eventos globales de creación/cierre, incluso cuando todavía no existe una ventana seleccionada. `try_borrow_mut` evita reentrancia de la UI durante llamadas a AppKit.

Las referencias a símbolos privados se resuelven una sola vez y el framework permanece cargado. Se comprueban los tipos de CFDictionary, CFArray, CFNumber y AXValue antes de leer datos. Los símbolos ausentes producen un estado explícito no disponible.

## Coordenadas y borde

WindowServer devuelve coordenadas globales con origen superior izquierdo de la pantalla principal. AppKit utiliza el origen inferior izquierdo de esa misma pantalla. El panel convierte `y = altura_principal - y_windowserver - altura_ventana`, sin margen exterior. El trazo se pinta hacia dentro del rectángulo real. El radio se consulta por ventana y se aplica con curva continua de Core Animation; si el metadato no está disponible se usa un radio de respaldo de 10 puntos.

El panel usa `NSWindowCollectionBehaviorTransient`: macOS lo oculta en Mission Control, en lugar de mantenerlo inmóvil como un panel `Stationary`. Es transparente, no activable e ignora eventos del ratón. Está en nivel flotante y se muestra sin ordenarse respecto a ninguna ventana ajena. Una máscara Core Animation conserva únicamente la región del objetivo que no cubren otras ventanas. El recorte usa resta secuencial de rectángulos disjuntos y descarta oclusores totalmente transparentes. Los cambios de orden solo actualizan esa máscara: nunca activan ni elevan al propietario del borde.

El movimiento adelanta la traslación del panel propio mediante `SLSMoveWindow`; el siguiente turno del hilo principal actualiza la máscara de oclusión si cambió. No se capturan texturas ni se dibuja una copia del contenido ajeno. Redimensionar actualiza el panel y su capa sin animaciones implícitas y fuerza el commit de Core Animation. Creación y activación ejecutan además una reconciliación a los 80 ms, porque una app puede activarse antes de ordenar su primera ventana. No hay sondeo permanente del borde.

No se modifica ni se inyecta código en la ventana objetivo. Es un overlay externo; no puede ofrecer una garantía de composición atómica con cualquier aplicación o animación del sistema.
El marco de reelección (`border::Flash`) comparte nivel y comportamiento de colección con el borde: panel propio no activable, sin sombra, con su capa `borderColor` fija, `cornerRadius` de 8 pt y rect igual al `visibleFrame` de la pantalla, de modo que el trazo superior no queda oculto tras la barra de menús. Su vida es un `Instant` acotado; la entrada y la salida se programan con dos `performSelector:afterDelay:` que se cancelan y reemplazan, de modo que el cuadro nunca se reinicia dentro de la misma ventana. No tiene temporizador propio ni interviene en el escaneo del mosaico.

## Foco y permisos

El temporizador se registra con la constante exportada NSRunLoopCommonModes de Foundation. Solo existe con foco habilitado y permiso AX vigente. Consulta cursor, botones y modificadores; descarta arrastre y ⌥, exige 240 ms de reposo y realiza una consulta por posición. AX valida que el puntero pertenece a una ventana estándar (excluyendo menús y Mission Control); WindowServer identifica la ventana visible exacta. Esto evita tanto un AX hit-test obsoleto entre ventanas hermanas como la capa transparente de pantalla completa que publica Dock.

La activación utiliza GetProcessForPID, _SLPSSetFrontProcessWithOptions y registros de activación por ID de ventana. El formato está adaptado de yabai (MIT; licencia incluida). Entre ventanas del mismo proceso se separan desactivación y activación por 40 ms mediante un selector diferido, sin bloquear el hilo principal. La operación se cancela si el cursor se mueve, se pulsa un botón, se mantiene ⌥ o se desactiva la función antes de completarse. AXRaise solo se ejecuta si el usuario activa Traer al primer plano. No se inyecta código ni se sintetiza un clic de ratón.

El objetivo de foco se conserva para que el borde siga la ventana que recibe el teclado aunque no sea la primera en la pila. Se descarta al pulsar el ratón, cambiar de proceso o detectarse otro foco AX después del periodo de asentamiento. El borde adapta su máscara a las ventanas oclusoras, sin ordenar ventanas ajenas.

La app no concede permisos por sí misma. El botón Accesibilidad solicita al sistema el diálogo correspondiente y abre su panel de ajustes. Se vuelve a comprobar la autorización cuando la app recibe foco. Los errores del registro de inicio de sesión se muestran y se consulta el estado real de `SMAppService` para no dejar un interruptor engañoso.

## Referencias técnicas

- [Apple: NSWorkspace.activeSpaceDidChangeNotification](https://developer.apple.com/documentation/appkit/nsworkspace/activespacedidchangenotification).
- [Apple: AXObserverCreate](https://developer.apple.com/documentation/applicationservices/1460133-axobservercreate).
- [Apple: SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice).
- [objc2: interfaz con Objective-C](https://docs.rs/objc2/latest/objc2/).
- [JankyBorders: declaraciones de SkyLight y notificaciones de WindowServer](https://github.com/FelixKratz/JankyBorders/blob/main/src/misc/extern.h).
- [Hammerspoon: investigación e integración de SkyLight/Spaces](https://github.com/Hammerspoon/hammerspoon/blob/master/extensions/spaces/libspaces.m).

Las cabeceras del SDK de macOS son la referencia para los selectores, las firmas C y las constantes AppKit. La implementación Rust es propia; las firmas y los IDs de eventos privados se contrastan con proyectos que documentan el comportamiento de WindowServer.

## Actualizaciones

El bundle incorpora Sparkle 2.10.1, cargado mediante NSBundle desde Contents/Frameworks. Todos sus objetos permanecen en el hilo principal. SPUStandardUpdaterController ofrece los diálogos nativos y SPUUpdater persiste la preferencia de búsqueda. Se verifica la firma Ed25519 del feed y la del ZIP antes de extraerlo. No se envía perfil del sistema y las instalaciones automáticas están deshabilitadas. El modo cargo run funciona sin el framework, con los controles de actualización deshabilitados.

El empaquetador fija versión y SHA-256 de Sparkle. El script prepare-release.py comprueba coherencia de versiones, firma ZIP y feed con la clave del Llavero y genera sumas SHA-256. El feed público se actualiza después de publicar el archivo, evitando anunciar una descarga inexistente.

## Estilos de escritorio

Snapshot incluye IDs y el índice activo de la pantalla actual. space_style construye una cadena atribuida con NSColor semántico, opacidad 1/0,38 y SF Symbols nativos. No hay redibujado periódico. Los cambios de escritorio/app/pantalla y las preferencias refrescan el indicador. Los iconos se guardan por ManagedSpaceID en NSUserDefaults; el panel reconstruye sus filas al cambiar la lista de escritorios y admite desplazamiento. Las preferencias desconocidas se normalizan a números legibles.

## Instancia única

Un bloqueo advisory flock en Application Support permanece abierto durante la vida del proceso. Una segunda apertura sale sin crear otro indicador ni otro temporizador. El descriptor se libera al terminar; la comprobación de diagnóstico puede ejecutarse simultáneamente. La carpeta no contiene contenidos de ventanas ni datos de actividad.

## Cambio de escritorio

El NSStatusItem usa un único NSStatusBarButton para recibir clics físicos. Sus segmentos de 24 pt dibujan cada escritorio y conservan acciones AX individuales, pero su hitTest devuelve nil para que el botón nativo gestione el ratón. La selección utiliza la posición real del cursor convertida a las coordenadas del botón: el evento reenviado por el anfitrión de la barra puede indicar el centro del grupo y seleccionar siempre el segundo segmento. El clic derecho abre ajustes. Los segmentos se conservan cuando solo cambia el estado activo. Con más de nueve escritorios, un menú nativo conserva el acceso a todos los destinos sin ensanchar indefinidamente la barra. El menú se presenta fuera del préstamo de App para que sus callbacks puedan ejecutarse.

La selección vuelve a consultar la pantalla, el ID y la posición real del destino, incluyendo las posiciones de espacios a pantalla completa. Se resuelve CGDisplayID a partir del UUID de pantalla. `space_switch` envía una secuencia nativa de gesto horizontal sin abrir Mission Control. macOS 27 exige un payload IOHID serializado y orientado según la preferencia de desplazamiento natural. El protocolo está adaptado de [iss](https://github.com/joshuarli/iss) y [strafe](https://github.com/rileycx/strafe); se incluyen sus licencias. No hay interceptores globales de gestos.

Solo durante una petición hay un temporizador de 50 ms, registrado en los modos comunes del run loop. Cada salto debe confirmar su ID real antes del siguiente: dos segundos de plazo por salto y ocho para la operación completa, sin bloquear el hilo principal. Se concede un margen de 120 ms al asentarse cada salto. Un clic nuevo sustituye el destino pendiente, conservando el salto que ya está en curso. Un cambio inesperado de escritorio cancela la petición. La operación no modifica preferencias, atajos ni propiedades de SkyLight y no desactiva SIP. El borde y el foco automático se suspenden durante el cambio. Al terminar se libera el estado pendiente; no queda sondeo de escritorios en reposo.

Las selecciones se entregan mediante un temporizador de una sola ejecución en los modos comunes del run loop. Si una llamada AX reentra mientras App está prestada, la entrega se reintenta en lugar de perder el clic.


## Gestor de mosaico

`tiling.rs` contiene el árbol binario y las plantillas puras, así como la clasificación normalizada del punto de destino (cuatro bordes y centro). `tile_manager.rs` adapta ese modelo a AX y SkyLight. `tile_hotkey.rs` gestiona registro y liberación de la combinación global con Carbon. No añade dependencias ni toca los observadores del borde.

La pertenencia se valida con `SLSCopySpacesForWindows`, sin inferirla de coordenadas ni del escritorio enfocado. Solo se aceptan ventanas con un único Space conocido. Cada display visible aporta su Space no fullscreen y `NSScreen.visibleFrame`, convertido a coordenadas globales AX. Si los símbolos privados no están disponibles, no se mueven ventanas.

Con el modo activado y permiso AX hay un temporizador adaptativo de 1/60 s durante arrastres de tiles y animaciones, de 80 ms mientras asienta un Space o hay verificación/modelo sucio, y de 250 ms en reposo. Sin trabajo pendiente, el límite de escaneo de 500 ms se comprueba **antes** de enumerar ventanas. Cuando hace falta una lista del compositor, verificación, animación y pertenencia comparten sus IDs y marcos; las guardas de presentación previas a escribir siguen vigentes. Las escrituras responden a cambios del modelo/área o desviaciones de la geometría real. Durante movimiento/redimensionado nativo solo se actualizan previews; el reparto se confirma al soltar. Se exige movimiento real de ventana y cursor. Suspensión, bloqueo y cambio de Space cancelan el gesto; desactivar elimina el temporizador. Las limitaciones estables de tamaño no reinician su presupuesto por un simple aviso de activación.

Preferencias: `tile.enabled`, `tile.off.<SpaceID>`, `tile.layout.<SpaceID>`, `tile.simple`, `tile.mods`, `tile.key`. Los IDs de ventana y objetos AX son efímeros y nunca se guardan como perfiles de pantalla. La futura persistencia completa deberá resolver monitores por UUID y ventanas por identidad de aplicación/documento antes de reconstruir el árbol.


### Proporciones, transiciones y reglas por aplicación

Cada nodo de división mantiene su proporción. Las cuatro plantillas producen árboles de divisiones, con filas/columnas compensadas por los gaps para conservar la simetría. Un gesto conserva una copia inicial del árbol y su área; cada muestra modifica los bordes compartidos respecto a esa copia, sin acumular error. La búsqueda recorre la rama de la ventana y ajusta el nodo cuya frontera coincide con el borde original. Los mínimos de cada subárbol limitan el ajuste. No se escribe la geometría de la ventana arrastrada hasta terminar el gesto.

Las transiciones guardan origen, destino y tiempo monotónico; interpolación quintic smootherstep de 160 ms. El caché `applied` guarda destinos, no fotogramas intermedios. Un gesto cancela las transiciones e invalida el reparto para evitar dejar ventanas a mitad de camino. Se cancelan también al cambiar de Space, suspender o perder autorización. Antes de escribir una transición se vuelve a comprobar la pertenencia al escritorio visible. Con `tile.animate` en 0 el mismo `transition_from` omite el lote: una escritura completa y su verificación, y el temporizador usa el ritmo de reparación en lugar de 60 Hz.

`tile_rules.rs` persiste `tile.floatingApps` como diccionario bundle ID → nombre visible en NSUserDefaults. El selector nativo NSOpenPanel obtiene los bundles sin lanzarlos. La regla solo se consulta al incorporar una ventana al gestor, por lo que una elección manual de tile no se revierte en el siguiente sondeo. Las pruebas de persistencia utilizan una suite separada, sin modificar las preferencias del usuario.


### Correcciones de flotantes, cobertura y coste de animación

El conjunto `floating` solo cambia por atajo, regla inicial o destrucción/reutilización de la identidad de ventana. Las flotantes quedan fuera de la captura de arrastre y `place_drag` vuelve a comprobarlo antes de modificar el árbol; no existe reinserción por ratón. Tampoco se restauran al desactivar el mosaico.

Cancelar una transición invalida su destino pendiente. La reconciliación compara WindowServer con los frames esperados, incluso si el modelo no ha cambiado. Los tamaños impuestos por una app se registran junto con destino y geometría observada; solo vuelven a intentarse si cambia alguno de ellos o se elige explícitamente una plantilla. La escritura final sigue tamaño → posición → tamaño → posición: evita límites por el monitor anterior y desplazamientos introducidos al redimensionar. El redimensionado final también compara geometría real en lugar del caché de solicitudes.

Las muestras usan un único instante monotónico por fotograma. La curva quintic tiene velocidad y aceleración cero en ambos extremos. Los pasos intermedios solo envían tamaño/posición si cambia ese atributo; la colocación completa y verificación AX se reservan para el final. La caché de pantallas caduca a los 500 ms y se invalida con las notificaciones de Space/pantalla; no se vuelve a enumerar NSScreen cada fotograma. El temporizador cambia de frecuencia solo al entrar/salir de movimiento y se elimina sin permiso AX. No se prometen tiempos de GPU ni cifras de consumo sin medición en una sesión autorizada.


### Rejilla y snapshot de tamaños

`Desktop.grid` cuantiza las fronteras de los árboles en coordenadas globales del área útil, compensando medio gap a cada lado. Se encaja cada frontera dentro de los mínimos de sus dos subárboles. Los campos persistentes `tile.grid.enabled`, `tile.grid.columns` y `tile.grid.rows` controlan esta función. El redimensionado parte de la geometría del modelo; para bordes exteriores traslada el delta a la frontera compartida opuesta. El hit test prioriza el handle de la ventana enfocada para no confundirlo con una vecina.

Las plantillas geométricas conservan su árbol ajustado durante movimientos: remapean las hojas según la nueva asignación de ventanas y actualizan la firma de orden sin borrar proporciones. Dwindle conserva sus proporciones al intercambiar y sigue permitiendo cambios de topología en inserciones laterales.

`tile.saved.<SpaceID>` guarda un snapshot textual versionado de plantilla, número de tiles, rejilla y árbol con proporciones. Las hojas usan índices de slot, nunca IDs de ventanas. El lector valida versión, profundidad, proporciones finitas, rejilla, número e identidad única de slots y ausencia de tokens sobrantes antes de cambiar el estado. La restauración requiere el mismo número de tiles y conserva las ventanas flotantes fuera del snapshot.


### Cambios de composición y de Space

Eliminar una ventana poda la hoja tanto del árbol Dwindle como del árbol personalizado de las otras plantillas, actualizando su firma de orden. Se colapsa solo su padre; las demás proporciones se mantienen. Insertar otra ventana en una distribución personalizada divide el destino dentro del árbol existente.

NSWorkspaceActiveSpaceDidChange tiene un callback separado de activación de aplicaciones: cancela todas las escrituras pendientes, incluso con el ratón pulsado o una entrega de ventana pendiente. `space_quiet_until` pausa el gestor durante 400 ms desde el último aviso, sin consultar/reconciliar ventanas ni escribir geometría. Los cambios consecutivos reinician ese plazo; la finalización de un cambio solicitado desde la barra lo reinicia también. La consulta periódica detecta cambios de escritorios visibles como respaldo si falta la notificación. Al reanudar se invalidan los datos previos y hay una fase de reconciliación de 100 ms durante otros 600 ms, sin interpolación adicional; la barrera de reentrada admite dos muestras consecutivas que coinciden durante 60 ms y `SpacePause` comparte el mismo suelo de 400 ms, así que el borde vuelve con el primer escaneo autoritativo. Los árboles se conservan y los plazos de entregas y retenciones de monitor se prolongan para no agotarse durante la pausa.

La eliminación de objetos AX ya no consulta AXRole de cada ventana oculta; una lista global de WindowServer identifica cierres y conserva las ventanas minimizadas/fuera del Space activo. Un traslado observado elimina inmediatamente la hoja del árbol de origen y descarta destinos/transiciones anteriores. Las consultas fallidas de la lista global no borran estado.


### Inserción y recuperación de Espiral

La inserción en un árbol personalizado de Espiral usa el último slot del orden, no el foco. El siguiente giro se deriva del eje y lado de la última división existente; no se infiere de la relación de aspecto ni de una profundidad que cambia al cerrar nodos. Las proporciones de los demás nodos se conservan.

`fit` mantiene la topología y limita ratios con los mínimos de subárbol, sin cambiar a cuadrícula. Si esa topología no cabe, el adaptador informa al usuario. La selección explícita de plantilla llama a `reset_template`, incluido seleccionar la plantilla ya activa, para permitir recuperar su estructura canónica.


### Intención espacial y varios monitores

El destino es `DropIntent::Screen(lado)`, `Window(id, zona)` o `Group(id)`. Los bordes de pantalla tienen prioridad, con una banda de 8 % del eje menor limitada a 28–80 pt; el resto usa zonas normalizadas dentro del tile. `proposed_drop` genera una copia completa del modelo que comparten preview y commit. El borde global envuelve el árbol restante; un borde local divide solo su destino; el centro conserva geometría e intercambia identidades. Las propuestas demasiado pequeñas se rechazan. Una espera de 70 ms estabiliza cambios de destino; salir de una zona válida oculta el fantasma inmediatamente.

Con **Agrupar como pestañas** activo (`tile.groups`, ausente = 1), el tercio superior de un hueco entrega `Group(id)`: `join_group` añade `id` a la tira de `target`, lo convierte en la pestaña visible y deja una única hoja para las dos ventanas, de modo que el número de hojas coincide siempre con el de `order`. Si `id` ya era hoja propia, esa hoja se colapsa; si pertenecía a otra tira, su hermana promovida conserva el hueco de aquella. `proposed_drop` comprueba antes el límite `Desktop::max_group_tabs` (`tile.groups.max`, predeterminado 5, ajustes 2–20): una admisión rechazada no muta ningún grupo; seleccionar un miembro existente sigue permitido aunque se haya reducido el límite. Cada tira se guarda en `Desktop::groups` como `members` más `active`. `sync_tabs` adopta la pestaña que el compositor muestra realmente antes de reconciliar; los miembros ocultos no son llegadas.

`draw_strips` pinta una tira por grupo de al menos dos miembros: tarjeta translúcida, icono de la app y título AX, con la pestaña seleccionada resaltada. El resultado `Copy` del título sigue vivo durante la conversión y el icono prestado se retiene mediante el retorno tipado de `msg_send!`. La firma de contenido incluye los IDs, títulos, iconos, selección y tamaño, no la posición. Un `NSScrollView` limita el ancho al hueco y permite recorrer tiras largas. Cada `NSButton` envía `selectTileTab:` con el ID; `activate_tab` valida identidad y pertenencia antes de enfocar y elevar. El panel es no activable, pero recibe esos clics; la captura de arrastre excluye su superficie solo cuando el panel está visible y en un Space activo. Las selecciones externas también actualizan la hoja a partir de la ventana frontal. `hidden_tab_frames` calcula los destinos de todos los miembros independientemente de si la hoja activa necesita colocación; la escritura oculta reutiliza verificación y restricciones acotadas. Los badges de intención siguen siendo transparentes al ratón.

Las tiras se reutilizan por `Workspace` y miembros del grupo, no por su índice entre los escritorios visibles. Los paneles no usan `CanJoinAllSpaces` ni `MoveToActiveSpace`. WindowServer puede publicar una lista vacía inmediatamente después de ordenarlos: la confirmación de pertenencia usa la cadencia existente durante un máximo de un segundo, sin bloquear el hilo principal ni añadir un temporizador. Una vez confirmada, permanecen ordenados durante la barrera de reentrada para que el compositor gestione el cambio. Sin confirmación, el fallback conserva el overlay global y lo oculta durante la transición. `trim_overlays` conserva las tiras de grupos vivos de escritorios inactivos y retira las que ya no tienen modelo; un cambio de pantallas descarta la caché espacial. Pausa, pérdida de permisos y presentaciones del escritorio mantienen la ocultación explícita. No se añade ningún escaneo permanente.

El indicador de arrastre usa `DropHint` sobre el hueco lógico original, no sobre el futuro rectángulo físico de la ventana agrupada. Entre huecos ya gestionados muestra ambas zonas, con el mismo `GROUP_DROP_FRACTION` que el hit-test: tercio superior para agrupar y resto para intercambiar. `PreviewContext` conserva las propuestas de ambas acciones, también las rechazadas por capacidad o mínimos. Un destino rechazado permanece visible pero sin resaltado de aceptación; oculta la preview anterior durante la histéresis. Las llegadas con divisiones de borde conservan su preview real y un indicador de colocación, sin anunciar dos zonas que no correspondan.

Los iconos alcanzan 72 pt y los textos se adaptan al espacio. `HintBadge` reutiliza panel y vistas al mover el destino o cruzar el separador; cambiar la selección solo retinta las placas existentes. Solo tamaño, tipo de destino o disponibilidad reconstruyen contenido. Son capas de color translúcido, sin blur añadido, captura de ventanas, consultas AX ni temporizadores nuevos.

`Desktop::frames` conserva los huecos lógicos del árbol y del hit-test. `window_frames` transforma esos mismos rectángulos sin una segunda asignación: para grupos de dos o más miembros reserva `TAB_BAR_HEIGHT` (22 pt) y `TAB_BAR_GAP` (4 pt). Colocación AX, miembros ocultos, previews y miniaturas personalizadas usan los rectángulos físicos. `window_frame`/`slot_frame` convierten ambos sentidos al redimensionar; el mínimo del grupo añade la reserva al máximo de los mínimos físicos de sus miembros. La barra queda encima de la ventana, dentro del hueco lógico, alineada a la izquierda con 8 pt de margen salvo `tile.groups.centered=1`. Un singleton no reserva cabecera.

Cada botón usa `AMSpaceTabPan`, subclase de `NSPanGestureRecognizer` con umbral de 6 pt: el reconocedor nativo acepta incluso 1 pt. `AMSpaceTabButton` descarta el mouse-down diferido que AppKit puede reproducir al cancelar un pan reconocido, sin bloquear `performClick:` ni el clic siguiente. `dragTileTab:` entrega coordenadas nativas de ventana a `Manager::tab_drag`. Dentro de la barra, con tolerancia de 12 pt verticales y 24 pt en los extremos, se calcula una inserción por los centros de los títulos, excluyendo el miembro arrastrado. La conversión a coordenadas de documento respeta el scroll; el movimiento cerca de los extremos desplaza el contenido sin reloj nuevo. Un marcador reutilizado muestra la inserción, sin reconstruir controles durante el gesto. Fuera de esa franja se presenta la separación, si cabe.

Solo `Ended` confirma tras revalidar identidad, pertenencia, área y modelo. La reordenación conserva la selección por ID, el árbol, los rectángulos y las colocaciones pendientes; no enfoca ni eleva ventanas. `Desktop::reorder_tab` rota el tramo afectado in situ, sin asignar memoria, y no aplica el límite de admisión a miembros existentes. Una separación rechazada por mínimos no impide reordenar. El mouse-up global abre una gracia acotada de 350 ms para recibir `Ended`; si no llega se cancela sin mutación. Solo separar invalida las colocaciones del escritorio afectado.

La × es un `NSButton` hermano del botón de selección, sin reconocedor de arrastre, con 22 pt de ancho reservados y etiqueta accesible «Cerrar ventana: …». `closeTileTab:` llama a `Manager::close_tab`, que rechaza transiciones, gestos, identidades obsoletas y foco protegido. Selecciona explícitamente el miembro oculto para que cualquier confirmación sea visible y solicita `accessibility::request_close` (`AXCloseButton` + `AXPress`, con tipo CF y habilitación comprobados). No elimina el miembro ni asume que la petición haya cerrado la ventana: los observadores y la reconciliación existentes confirman su desaparición. La UI no llama a `settle_focus` tras cerrar, para respetar posibles diálogos de guardar.

La presentación conserva el material oscuro nativo y limita el fondo al ancho visible de la tira. El borde redondeado, el contraste de selección y la sombra del panel se establecen al construir las vistas. Cada pestaña conserva un área de seguimiento nativa: entrar o salir solo cambia la transparencia del botón de cierre inactivo, no su geometría ni su acción. No se añaden relojes, animaciones continuas ni consultas AX; los paneles y su contenido siguen reutilizándose mientras su firma no cambie.

Durante un gesto de ratón, una tira con miembros, selección y tamaño idénticos reutiliza títulos/iconos y controles: no consulta AXTitle por fotograma. Al soltar vuelve a la reconciliación normal. Los escritorios sin grupos no clonan su árbol para dibujar tiras inexistentes. El badge de intención solo reconstruye contenido al cambiar de símbolo, no al trasladar su panel.

Cambiar/restaurar plantilla o confirmar redimensionado cancela las colocaciones anteriores de **todos** los miembros de sus grupos, no solo de las hojas activas. La restauración de geometría al desactivar exige identidad PID/ID, coincidencia AX y Space nativo actual; una consulta fallida, un traslado o la desaparición de pantalla no autorizan escribir coordenadas retenidas.


Los árboles en memoria se indexan por `(DisplayID, SpaceID)`. Las áreas usan `visibleFrame`, convertido con la altura del monitor primario a coordenadas AX globales en puntos, incluyendo coordenadas negativas. La pertenencia se determina por mayor solapamiento dentro del Space nativo, evitando mezclar monitores que comparten Space. El identificador SkyLight `Main` se expande a las pantallas conectadas. Los perfiles geométricos usan `tile.saved.<SpaceID>.<DisplayID>` y leen también la clave histórica por Space; aún no son perfiles portables por UUID.

El gesto distingue el redimensionado iniciado en un borde del cambio de tamaño nativo al cruzar a otra pantalla. Al soltar entre monitores, una entrega pendiente de hasta 700 ms espera la pertenencia nativa antes de modificar origen y destino. Durante esa espera no se recoloca de vuelta al origen. Suspender, desactivar o comenzar otro gesto elimina la entrega. La animación sigue usando el compositor nativo y el preview es un único panel sin captura de entrada.


### Selector de plantillas y estabilidad al cruzar monitores

`tile_picker.rs` presenta un NSPanel no activante con ocho botones accesibles para las plantillas integradas, más una tarjeta por cada plantilla guardada, con miniaturas de orientación real y un único contenedor de previsualización. NSTrackingArea actualiza el hover por eventos, sin sondeo adicional. Teclado: números (1–8 y 9/0 para las dos primeras plantillas guardadas), flechas, Tab, Intro y Escape. El gestor de foco y la distribución quedan suspendidos mientras el selector está abierto. Cambiar de Space, desconectar/rotar pantalla, perder foco o suspender cancela. Solo confirmar modifica el modelo y valida de nuevo monitor, Space, área y conjunto de ventanas. Una tarjeta guardada está habilitada si tiene el mismo número de huecos que el mosaico actual y sus rectángulos caben en el área; las demás aparecen deshabilitadas.

Las tarjetas personales y su preview grande se calculan sobre una copia del `PickerContext.desktop`, con identidades, grupos, reserva de cabecera, mínimos de aplicaciones y rejilla del contexto. No se usa el ejemplo ordinal de la biblioteca para decidir si una plantilla cabe. Solo las miniaturas de plantillas rechazadas pueden mostrar ese ejemplo; no habilita aplicar ni dibuja un destino válido.


Los índices históricos 0–3 de Layout se conservan. Los índices 4 y 5 añaden tres/cuatro divisiones principales: columnas en horizontal, filas en vertical. Los excedentes se distribuyen en el eje secundario y las ventanas insuficientes llenan toda el área. `tile.layout.<SpaceID>.<DisplayID>` tiene prioridad sobre la preferencia histórica por Space. Carbon distingue ambos atajos por EventHotKeyID; un manejador devuelve eventNotHandled si el evento corresponde al otro.

El arrastre fija su intención al primer movimiento/redimensionado observado. Un cambio de tamaño automático al entrar en un monitor vertical no modifica el árbol del origen. El monitor del cursor determina el destino, evitando la ambigüedad de las bandas de borde compartidas. Sin zona concreta al soltar en otra pantalla, se usa la plantilla del destino.

Después de confirmar una transferencia, la propiedad del monitor se retiene durante un plazo máximo de 1,2 s para tolerar frames antiguos de WindowServer; no se interpola desde una pantalla a otra. Un Space nativo distinto, un nuevo gesto sobre esa ventana o la desaparición del monitor invalida esa retención. Los tamaños mínimos impuestos por aplicaciones se registran también en las colocaciones directas para no volver a clasificar una ventana por el área que sobresale del monitor vertical.


Las escrituras finales AX se verifican después de un margen de 60 ms, mediante WindowServer. Si la geometría no ha llegado, se permite un único reintento y otro margen de 60 ms antes de registrar un límite estable. Durante la verificación no se inician animaciones repetidas hacia el mismo destino. Un nuevo gesto, una transferencia o la cancelación elimina escrituras diferidas antiguas.

Iniciar una transición elimina la verificación del destino anterior. Cada verificación comprueba primero la pertenencia nativa al Space, también antes de registrar una restricción. Las notificaciones de configuración de pantallas y los cambios detectados en el área útil cancelan escrituras y gestos pendientes, descartan retenciones de monitor y límites anteriores, y fuerzan reconciliación sin animación durante el asentamiento. El botón del ratón permanece marcado como pulsado hasta soltarlo para impedir recapturar un gesto cancelado.

Las notificaciones de activación invalidan el sondeo, sin descartar restricciones estables: solo un destino o una geometría observada diferentes, o la selección explícita de plantilla, permiten volver a intentarlo. La detección de gesto exige también movimiento del cursor superior a 3 pt, evitando interpretar ajustes de geometría durante un clic inmóvil como cambios de proporciones.

`MembershipGrace` conserva durante 650 ms la hoja de una ventana gestionada sin confirmación nativa suficiente. Una ventana visible con PID, Space y solapamiento físico confirmados conserva su hoja durante un fallo AX más largo, sin escribir su geometría mientras sea incierta. Recuperarse reinicia el plazo; un cierre o traslado nativo confirmados no se retienen en el origen. Una consulta fallida a WindowServer no se interpreta como escritorio vacío. Antes de animar o verificar se comprueban PID e ID visibles, y al reutilizarse un ID por otro proceso se elimina su estado anterior completo.


### Escrituras AX durante las animaciones

La geometría final se redondea por bordes para conservar gaps. Un fallo intermedio no cancela la escritura final. Solo un gesto real de movimiento/tamaño interrumpe las transiciones; un clic en el selector no invalida el destino. La visibilidad se consulta en lote a WindowServer durante la animación, sin leer AXFullScreen/AXMinimized por ventana y fotograma.

La transacción final escribe tamaño, posición, tamaño y posición: la segunda escritura de tamaño evita límites impuestos por el monitor de origen y la última de posición corrige desplazamientos que la aplicación pueda introducir al redimensionarse. El resultado se decide por las dos escrituras finales y se comprueba después mediante WindowServer.

El reloj de cada transición se inicia en su primer fotograma: preparar sesiones AX para varias aplicaciones no consume la duración de animación. Los destinos preparados en una misma reconciliación comparten el inicio del siguiente fotograma. Solo una escritura aceptada actualiza `last`, y no se ejecuta la enumeración AX completa mientras queden transiciones activas. Las notificaciones de pantalla/Space y los gestos reales siguen cancelando el lote. El selector utiliza `refresh_for_picker`, que actualiza el modelo sin colocar ventanas; confirmar cancela los destinos antiguos solo de las ventanas de su contexto antes de aplicar el reparto nuevo.

La geometría original de una ventana conserva su área de referencia. Restaurarla en otra área remapea proporcionalmente su posición y limita el tamaño para mantenerla dentro del monitor de destino. En Spaces compartidos, la asignación conserva el monitor anterior cuando los solapamientos son casi iguales (margen del 5 % respecto al mayor). Ni ese margen ni una retención pueden anular una transferencia nativa confirmada o recuperar un monitor desconectado.

El borde compara su geometría con WindowServer y conserva el último estilo pintado (grosor, color, radio y escala) y la máscara aplicada. No realiza `setFrame` ni repintados redundantes, y no hay llamadas de ordenación relativa a ventanas ajenas. Los eventos de su propio panel se ignoran. Se observan las ventanas normales y flotantes para actualizar oclusión; una notificación de orden no selecciona otro objetivo. Los subroles desconocidos no excluyen ventanas reales: únicamente se rechazan roles auxiliares reconocidos como menú, tooltip o popover.

`GeometrySession` conserva temporalmente el estado app-level AXEnhancedUserInterface. Las transiciones de una misma aplicación comparten la sesión; la última referencia restaura el valor original, también al cancelar. Las escrituras finales y los reintentos adquieren la misma sesión. La motivación y el antecedente técnico se documentan en [Rectangle PR #285](https://github.com/rxhanson/Rectangle/pull/285). No se copió código de Rectangle.


### Sugerencias de atajos

Se conservan los cuatro índices originales de teclas/modificadores para mantener preferencias. El catálogo actual se define en `KEYS` y `MODIFIERS` de `tile_hotkey.rs`, con límites derivados de la longitud de cada catálogo. `CopySymbolicHotKeys` excluye combinaciones habilitadas del sistema; después se prueba `RegisterEventHotKey` con `kEventHotKeyExclusive`, liberando cada candidato inmediatamente. Los registros definitivos también son exclusivos. Antes se usaba opción cero, que admite compartir una combinación entre procesos y no prueba disponibilidad global.

Buscar devuelve hasta seis candidatos y excluye el atajo flotante. Usar registra de nuevo para detectar carreras con otras apps. No se desregistran los atajos actuales durante la búsqueda ni se cambian preferencias hasta que el usuario aplica una sugerencia. Los menús internos de aplicaciones no son enumerables por esta API; esa limitación se comunica en ajustes. Fuente de constantes y contrato: CarbonEvents.h del SDK de macOS instalado.

## Atajos y traslados por teclado (0.7)

`tile_hotkey` conserva los índices históricos de teclas/modificadores y las acciones Float/Layouts. Añade IDs de acción disjuntos para nueve escritorios, nueve envíos y dos direcciones de pantalla. Carbon mantiene reservas exclusivas y la entrega en la liberación de la tecla, como antes. `shortcuts` reúne la configuración, detecta duplicados y aplica cambios de varias preferencias como una transacción; el rechazo restaura también la ausencia original de una clave.

El modo prioritario instala un event tap activo de sesión en un hilo con run loop propio, sin temporizador de sondeo. El callback solo compara tecla/modificadores, mantiene las liberaciones consumidas y encola la acción; no consulta AX ni modifica ventanas. El apagado se solicita con un bloque del run loop y se espera al hilo antes de liberar objetos. El arranque usa confirmación entre hilos y referencias CF explícitas para que un timeout no deje interceptores huérfanos. Los timeouts del tap permiten reactivarlo; una desactivación explícita por usuario/sistema se respeta. La entrada segura protegida la retiene macOS y se comprueba de nuevo en el hilo principal. Se descartan acciones antiguas (>350 ms), deshabilitadas o pertenecientes a una generación anterior de configuración.

`window_move` captura la ventana activa por PID/ID y contrasta su geometría AX con WindowServer. El origen debe coincidir con un escritorio visible real. Se rechazan modales, fullscreen, minimización, membresía ambigua, entrada segura y ratón pulsado. La dirección de monitor se decide por centros y proximidad, no por el orden de NSScreen. `spaces::desktop_ids_for_display` también contempla Spaces compartidos.

`Manager::prepare_keyboard_transfer` prepara una copia de la distribución de destino mediante `arrival_proposal`. Antes de preparar la llegada refresca la pertenencia nativa del destino, también con caché existente, y omite flotantes y ventanas auxiliares. Mantiene las ranuras conocidas durante una falta temporal de AX si la pertenencia nativa sigue siendo válida. Una entrada nativa sin ventana AX verificable no veta el envío completo: se conserva su geometría y se deja para la reconciliación al entrar. Para las ventanas ya gestionadas se reutiliza su referencia AX validando propietario e ID. No confirma árboles antes de la operación nativa. `sky_bridge` localiza la entrada exacta de la operación asíncrona en la imagen SkyLight cargada: comprueba comandos Mach-O, offsets de __LINKEDIT, nombres completos, tipo N_SECT y dirección dentro de segmentos ejecutables. No busca patrones de instrucciones ni escribe en memoria. La motivación y el antecedente están en yabai; se incluye su licencia MIT.

Mientras se espera, se pausa el trabajo de tiling y foco del cursor. Se confirma PID/ID y pertenencia al Space durante dos observaciones consecutivas. El cambio de monitor usa AX tras confirmar el Space y permite un único reintento de geometría; el origen permanece inmutable hasta confirmar. El plazo global es de dos segundos. Ratón, suspensión, cierre, pérdida de Accesibilidad o desaparición del destino evitan nuevas escrituras. Solo una llegada confirmada confirma el modelo, conserva el estado flotante y crea una retención breve del monitor. Un fallo no activa la propuesta guardada; la conciliación normal vuelve a observar la ubicación nativa.

Tras confirmar la llegada nativa se colocan las ventanas de la propuesta en el escritorio oculto mediante AX, sin activación ni animación. Se conservan referencias antes del traslado (AXWindows puede omitir ventanas ocultas) y se revalidan propietario, ID, Space, flags y capacidad de redimensionado antes de cada escritura. La colocación tiene dos intentos, separados al menos 180 ms, con límite de 600 ms; si el destino se hace visible, se detiene y lo reconcilia la ruta habitual. Las apps que rechacen la colocación oculta conservan la barrera de reentrada como alternativa. Con el ajuste de permanecer en origen desactivado, el cambio al destino se solicita después de confirmar el traslado. La ruta de selección de escritorio por gesto permanece intacta: la alternativa que solo cambiaba el Space reportado se descartó tras la comprobación física.

El traslado conserva las geometrías de origen/destino del plan y una instantánea de las pantallas participantes. Antes de escribir o confirmar compara ID, frame, área útil y escala con la configuración real: una rotación, reordenación física, cambio de escala o desconexión cancela coordenadas antiguas. La comparación no depende del orden de NSScreen ni de cambios en un monitor ajeno.

## Plantillas personales y reentrada (0.8, candidata)

`custom_templates` guarda nombre y distribución serializada por índices ordinales de hueco, sin IDs nativos. La biblioteca se valida al leer, no reemplaza nombres duplicados y permite borrar una entrada sin cambiar el mosaico. `custom_context` refresca el modelo sin colocar ventanas y fija la pantalla de la ventana de gestión; el Space actual se resuelve antes de cada acción. Aplicar valida pertenencia, área, número de ventanas y mínimos, conserva la rejilla actual y confirma solo el contexto elegido.

Al cambiar de Space se descartan el gesto y sus candidatos, sin desbloquear un botón físico todavía pulsado ni perder un handoff ya preparado. `reentry_preserved` identifica los contextos que conservaron Space y área; sus animaciones/verificaciones siguen con su presupuesto original en lugar de reiniciarse. La entrega de atajos con estado prestado se difiere 5 ms con límite de 350 ms y generación vigente. El diagnóstico de General lee la instancia real y separa el historial de traslados de los errores genéricos de tiling.
