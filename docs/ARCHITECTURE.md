# Arquitectura

```text
NSStatusItem ─── clic ─── NSPanel de ajustes
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
| `model.rs` | Presentación del indicador y normalización de preferencias |
| `spaces.rs` | Resolución opcional de SkyLight, lectura y selección de pantalla activa |
| `accessibility.rs` | RAII de CoreFoundation, consultas AX, observador de foco y ciclo de vida, respaldo de geometría |
| `window_server.rs` | Identidad, orden, geometría y radios reales; notificaciones de movimiento, tamaño, creación y cierre |
| `native.rs` | Puente AppKit pequeño: vistas, texto y colores |
| `updater.rs` | Puente Rust a Sparkle, ciclo de actualización y búsqueda opcional |
| `ui.rs` | Ciclo de vida, ajustes, panel de borde, temporizador e ítems de inicio |

## Seguridad de memoria y ciclo de vida

Todos los objetos de AppKit y las referencias AX se usan en el hilo principal. Los objetos Objective-C propietarios usan `Retained`; AppKit retiene sus subvistas. Los punteros no propietarios a controles solo se usan mientras existe su ventana. Los resultados `Create` y `Copy` de CoreFoundation usan `Owned` y liberan su referencia en `Drop`.

El observador AX se elimina del run loop antes de liberarse o cambiar de ventana. Los temporizadores se invalidan al desactivar el foco o suspender la sesión. Los callbacks AX agrupan notificaciones en el siguiente turno del run loop, sin temporizador de 16 ms, para no modificar el observador mientras se ejecuta su callback. Los callbacks del compositor usan un ID atómico para mover el panel sin pasar por AppKit; los cambios de tamaño y foco se entregan al hilo principal mediante dispatch. La suscripción incluye todas las ventanas visibles de la app activa y eventos globales de creación/cierre, incluso cuando todavía no existe una ventana seleccionada. `try_borrow_mut` evita reentrancia de la UI durante llamadas a AppKit.

Las referencias a símbolos privados se resuelven una sola vez y el framework permanece cargado. Se comprueban los tipos de CFDictionary, CFArray, CFNumber y AXValue antes de leer datos. Los símbolos ausentes producen un estado explícito no disponible.

## Coordenadas y borde

WindowServer devuelve coordenadas globales con origen superior izquierdo de la pantalla principal. AppKit utiliza el origen inferior izquierdo de esa misma pantalla. El panel convierte `y = altura_principal - y_windowserver - altura_ventana`, sin margen exterior. El trazo se pinta hacia dentro del rectángulo real. El radio se consulta por ventana y se aplica con curva continua de Core Animation; si el metadato no está disponible se usa un radio de respaldo de 10 puntos.

El panel usa `NSWindowCollectionBehaviorTransient`: macOS lo oculta en Mission Control, en lugar de mantenerlo inmóvil como un panel `Stationary`. Es transparente, no activable, ignora eventos del ratón y se ordena inmediatamente encima de su ventana objetivo, en el nivel normal, en lugar de flotar encima de todas las ventanas. Su identificación usa el orden visible del compositor y el PID activo; un `AXFocusedWindow` antiguo no puede seleccionar otra ventana de Ghostty por error.

Mover una ventana requiere únicamente consultar sus límites y trasladar el panel propio con `SLSMoveWindow`: no hay redibujado, llamadas AX ni espera en el hilo de AppKit en esa ruta. Redimensionar actualiza el panel y su capa sin animaciones implícitas y fuerza el commit de Core Animation. Creación y activación ejecutan además una única reconciliación a los 80 ms, porque una app puede activarse antes de ordenar su primera ventana. No es sondeo permanente y no retrasa la actualización inicial.

No se modifica ni se inyecta código en la ventana objetivo. Es un overlay externo; no puede ofrecer una garantía de composición atómica con cualquier aplicación o animación del sistema.

## Foco y permisos

El temporizador se registra con la constante exportada `NSRunLoopCommonModes` de Foundation, nunca con una cadena inventada. Existe únicamente si el usuario activa el foco y AX está autorizado. Consulta posición, botones y modificadores. Descarta arrastre y ⌥, exige 240 ms de reposo y realiza un único hit-test por posición. Antes de activar valida `AXStandardWindow`, descarta el propio proceso y evita repetir el foco actual. El hit-test excluye las ventanas propias; mantener abiertos o activos los ajustes no bloquea enfocar otra app al salir con el cursor. El ID real de WindowServer decide si la ventana ya está delante, evitando un AXFocusedWindow obsoleto entre ventanas de la misma app. Para enfocar otra aplicación se solicita activación y después se establecen `AXFrontmost`, `AXMain` y `AXFocused` antes de elevar la ventana. El sistema decide qué ventanas son accesibles.

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
