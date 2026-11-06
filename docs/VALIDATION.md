# Validación de 0.4.0

- Diez pruebas correctas; fmt, Clippy sin warnings, plist y compilación universal verificados.
- Confirmado en macOS 27.2 el cambio real 1 → 2 (vacío) → 1 desde la acción nativa de selección. El diagnóstico de SkyLight confirmó los IDs 5 y 1 tras las dos acciones, sin abrir Mission Control.
- El equipo tiene desplazamiento natural desactivado. Las pruebas cubren los signos para ambas preferencias, ambas direcciones y el protocolo anterior a macOS 27; también validan longitudes, fases y velocidad con signo del payload IOHID.
- El usuario confirmó el clic derecho para ajustes. La selección conserva el ID real del escritorio; la prueba de orden cubre espacios a pantalla completa intermedios, reordenación y eliminación del destino.
- Una petición mantiene como máximo un salto en curso; los clics nuevos actualizan el destino. Cada salto y la operación completa tienen plazo límite, y el temporizador desaparece al terminar.
- Pendiente de prueba física: varios monitores, selección de más de nueve escritorios, destinos separados por espacios a pantalla completa, cambios rápidos durante una transición y hardware Intel. No se ha cambiado la preferencia de desplazamiento natural del usuario para probarla físicamente.

# Validación de 0.3.1

- Siete pruebas correctas. La prueba de instancia única comprueba exclusión entre dos manejadores y disponibilidad después de liberar el primero.
- Realizada actualización completa desde una copia de 0.2.0: búsqueda en la app, oferta 0.3.0, descarga, validación, instalación y reinicio. Verificada versión 0.3.0 del bundle instalado y su firma estricta; las preferencias se conservaron.
- Localización española verificada en la app instalada: «¡Estás al día! AMSpace 0.3.1 es la versión más nueva disponible».
- Añadido bloqueo de instancia para impedir dos procesos de AMSpace ante relanzamientos simultáneos. Comprobado además con una segunda ejecución real: permanece un único PID.
- Versión final abierta normalmente desde Aplicaciones, con Accesibilidad renovada y estado «Accesibilidad concedida» observado en sus ajustes.
- El usuario confirmó que el borde desaparece al abrir Mission Control y vuelve correctamente al salir.

# Validación de 0.3.0

- Seis pruebas correctas, fmt y Clippy sin warnings; build universal y firma del bundle verificada.
- Inspeccionadas las tres pestañas y la ventana Acerca de. El copyright ya no se acerca al botón Historial.
- Modo Iconos probado en la UI: icono activo blanco e inactivo atenuado. Los 30 nombres SF Symbols comprobados con NSImage, sin ausencias en el sistema de prueba.
- El diagnóstico registró alternancia repetida entre las dos ventanas de Ghostty y ventanas de otras aplicaciones. En todos los cambios observados, AXFocused coincidió con el ID objetivo, con elevación desactivada. Esto comprueba identidad del foco; no sustituye las pruebas de orden con todas las aplicaciones y ventanas superpuestas.
- Detectada la capa transparente de pantalla completa de Dock, nivel 20, que bloqueaba el hit-test por geometría. Ahora AX decide si el elemento recibe entrada y WindowServer desambigua las ventanas de la misma app.
- Las APIs privadas se resuelven en ejecución; no se desactiva SIP ni se inyecta código.

# Validación de 0.2.0

- Compilación universal, fmt, Clippy sin warnings y las cinco pruebas de regresión: correctos.
- Interfaz nativa inspeccionada: versión instalada, búsqueda, búsqueda automática desactivada inicialmente y Acerca de con autoría y enlaces. Corregido el espacio entre historial y copyright.
- Sparkle 2.10.1 descargado del repositorio oficial y verificado mediante SHA-256 fijado; framework y auxiliares incluidos en el bundle.
- ZIP y feed firmados con la clave Ed25519 del Llavero. Ambas firmas verificadas. Un ZIP alterado tras firmar fue rechazado por el verificador.
- Eliminado el bloqueo de foco que afectaba a otras apps mientras los ajustes de AMSpace estaban activos. La comparación de ventana ya enfocada usa el ID de WindowServer.

# Validación de 0.1.1

- Reproducido el fallo del temporizador: la prueba de integración falla con la cadena literal `NSRunLoopCommonModes` y pasa usando la constante exportada por Foundation (`kCFRunLoopCommonModes`).
- La misma prueba verifica que el temporizador se invalida y deja de dispararse.
- Corregida la pausa por ventana de ajustes: requiere que AMSpace esté activa; una ventana de ajustes en segundo plano no bloquea el seguimiento.
- Para Mission Control, sustituido `Stationary` (permanece visible) por `Transient` (ocultado por el sistema), conforme a la cabecera de AppKit y la documentación de Apple.
- Cinco pruebas automatizadas en total. La comprobación física de foco y Mission Control requiere Accesibilidad vigente y la app actualizada.

# Historial: validación de 0.1.0

Fecha: 2026-11-06. Equipo local: Apple Silicon, macOS 27.2 (26B123). Toolchain: Rust 1.99.1. Objetivo mínimo del paquete: macOS 13.

## Comprobaciones realizadas

- `cargo fmt --all -- --check`: correcto.
- `cargo clippy --locked --all-targets -- -D warnings`: correcto.
- `cargo test --locked`: cuatro pruebas correctas. Cubren presentación/compactación, preferencias no finitas, creación de ventanas sin un borde previo y enrutamiento diferenciado entre geometría y cambio de ventana.
- Build release para `aarch64-apple-darwin` y `x86_64-apple-darwin`: correcto. Binario universal generado con `lipo`.
- `plutil -lint` y verificación estricta de la firma ad hoc: correctos.
- Ejecución de la app nativa, apertura de ajustes, aspecto oscuro, etiquetas accesibles, paletas, color personalizado y grosor: inspeccionados. Persistencia observada al relanzar.
- Indicador real: detectados dos escritorios y observados `1 ·` y `· 2`.
- Inicio de sesión: probado registro y desregistro; la UI consulta el estado real de `SMAppService`. No se cerró la sesión del usuario para probar el arranque completo.
- Accesibilidad: comprobado el comportamiento sin permiso y con permiso. Detectada y resuelta una entrada TCC que apuntaba a la firma de una compilación anterior, eliminándola y añadiendo de nuevo la app instalada.
- Borde corregido: `swift scripts/verify-border.swift` verificó igualdad exacta de posición y tamaño entre el panel y la ventana delantera de Ghostty, con dos ventanas de ese proceso abiertas. El radio consultado en esa ventana fue 16 pt. La comprobación es geométrica; no mide visualmente cada fotograma durante un arrastre.

## Correcciones incorporadas tras la prueba de uso

1. Eliminado el margen exterior: el borde se dibuja dentro del rectángulo real de la ventana.
2. Sustituido el radio fijo por metadatos de la ventana y curva continua.
3. Identificación por ID y orden de WindowServer, evitando depender de un `AXFocusedWindow` obsoleto entre ventanas del mismo proceso.
4. Movimiento directo del panel desde los eventos del compositor; eliminada la demora artificial de 16 ms y el trabajo AX de la ruta de movimiento.
5. Suscripción a creación/cierre de ventanas incluso sin objetivo previo; reconciliación única de apertura/activación para ventanas que aparecen después de activarse la app.
6. El panel se ordena sobre su ventana, en lugar de flotar sobre todas las aplicaciones.

## Alcance pendiente de validación física

- Intel en hardware real y versiones anteriores de macOS.
- Varias pantallas físicas, escalas mixtas, Stage Manager y todos los modos de Mission Control.
- Arranque completo después de cerrar y volver a iniciar sesión.
- Medición con vídeo de alta velocidad para cuantificar desfase por fotograma durante redimensionados extremos. El overlay es externo, no parte del proceso de la otra aplicación.
- Compatibilidad de foco al pasar el cursor con todas las apps de terceros, diálogos personalizados y aplicaciones que incumplen las convenciones AX.

No se considera que compilar o aprobar pruebas unitarias sustituya estas pruebas físicas.

## Reproducir la comprobación del borde

Activa el borde, concede Accesibilidad y enfoca una ventana de otra aplicación:

```sh
swift scripts/verify-border.swift
```

El script lee metadatos de WindowServer, sin capturar la pantalla, títulos ni contenido. Compara el rectángulo de la ventana normal delantera de la app activa con los paneles visibles de AMSpace. Devuelve 0 si coinciden, 1 si no coinciden y 2 si falta una condición previa. No cambia el foco ni mueve ventanas.

Prueba manual recomendada: abre dos ventanas de Ghostty, alterna con clic y `⌘\``, muévelas, cambia tamaño desde los cuatro bordes, crea una tercera, minimiza/cierra la activa y abre otra aplicación. Repite la comprobación anterior en cada estado asentado. Comprueba además que ⌥ y el arrastre suspenden el foco automático.

## Medición de recursos

Muestra local de 0.3.1 con borde y foco habilitados: 20,33 s, CPU media del 0,394 % de un núcleo y RSS entre 69,14 y 69,47 MiB. Es una muestra puntual de uso ligero, no un límite ni una comparación de equipos.

```sh
python3 scripts/benchmark.py <PID> --seconds 20
```

El script mide CPU acumulada con `ps` y RSS cada segundo. RSS incluye páginas compartidas de frameworks: no equivale a memoria exclusiva ni a la memoria total del compositor. Para obtener la huella física utiliza `sample <PID> 1 1` o Monitor de Actividad. Las mediciones dependen del tamaño de la ventana, Retina, opciones activadas y actividad del usuario.
