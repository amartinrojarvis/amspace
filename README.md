<p align="center"><img src="assets/icon.png" width="128" alt="Icono de AMSpace"></p>
<h1 align="center">amspace</h1>
<p align="center">Tu espacio. Tu foco.<br>Un compañero discreto para la barra de menús de macOS, escrito en Rust.</p>

AMSpace muestra dónde estás con un vistazo: números, puntos o iconos por escritorio. El activo destaca y los demás aparecen atenuados. Haz clic izquierdo en un número, punto o icono para cambiar a ese escritorio. El clic derecho abre unos ajustes nativos, compactos y adaptados al aspecto claro u oscuro del sistema.

## Funciones

- **Escritorio actual.** Tres modos: Puntos, Números e Iconos. 30 iconos nativos (trabajo, código, vídeo, fotografía, comunicaciones, audio, tareas, etc.) y la opción de mantener un número en cada escritorio. El activo destaca en blanco en modo oscuro; el color se adapta al aspecto del sistema. A partir de diez escritorios se compacta a `3 / 12`.
- **Cambio de escritorio desde la barra.** Cada número, punto o icono es un botón. El clic izquierdo selecciona ese escritorio y el derecho abre ajustes. Con más de nueve escritorios, el indicador compacto abre una lista de selección.
- **Borde de la ventana activa.** Contorno interior sin separación, con radio de esquina leído del sistema, transparente al ratón, color libre o cuatro paletas y grosor de 1 a 6 puntos, en pasos de 0,5.
- **Foco bajo el cursor.** Activa el teclado de la ventana tras detener el ratón unos 240–390 ms, incluso entre ventanas de una misma aplicación, sin cambiar su orden. Se pausa al arrastrar o mantener ⌥ y omite las propias ventanas de AMSpace. Salir de sus ajustes con el cursor permite enfocar otra app.
- **Traer al primer plano.** Ajuste independiente y desactivado inicialmente: además de enfocar, eleva la ventana sobre las demás.
- **Mission Control.** El panel del borde se oculta con la vista de escritorios y reaparece al volver al escritorio.
- **Inicio de sesión.** Registro con `SMAppService`, gestionado por macOS. Desactivado inicialmente.
- **Actualizaciones desde la app.** Versión instalada, búsqueda manual, avisos diarios opcionales, descarga verificada y reinicio mediante Sparkle.
- **Acerca de.** Autoría de Alberto Martín y enlaces a Instagram y X: [@amartinro](https://x.com/amartinro).
- **Preferencias persistentes.** Sin cuenta ni telemetría. Solo se conecta a GitHub al buscar o descargar actualizaciones; Sparkle utiliza auxiliares durante la instalación.
- **Icono propio.** Ventana redondeada con un espacio destacado; fuentes vectoriales y versión `.icns` incluidas.

El borde y el foco están desactivados inicialmente: actívalos cuando hayas concedido Accesibilidad.

## Instalar

Requiere **macOS 13 Ventura o posterior**. El paquete universal contiene Apple Silicon e Intel; la prueba de ejecución local se realiza en Apple Silicon. Descarga el ZIP desde [Releases](https://github.com/amartinrojarvis/amspace/releases), descomprímelo y mueve **AMSpace.app** a **Aplicaciones** antes de abrirlo.

La distribución actual está firmada **ad hoc**, sin certificado Developer ID ni notarización de Apple. Un archivo descargado puede requerir autorización explícita en Privacidad y seguridad. No desactives Gatekeeper ni SIP; también puedes compilarlo localmente.

1. Abre AMSpace. Solo aparece en la barra de menús, sin icono permanente en el Dock.
2. Haz clic derecho en un escritorio de la barra para abrir los ajustes.
3. Pulsa **Accesibilidad…** y autoriza AMSpace en **Ajustes del Sistema → Privacidad y seguridad → Accesibilidad**. Si no aparece, añádelo con `+` desde Aplicaciones.
4. Vuelve a los ajustes de AMSpace y activa el borde o el foco. Si macOS solicita reiniciar la app, sal y ábrela de nuevo.
5. Activa **Abrir al iniciar sesión** si lo deseas. macOS puede solicitar confirmar el ítem de inicio.

**El indicador de escritorios funciona sin Accesibilidad; cambiar de escritorio con un clic requiere ese permiso.** No se necesita Grabación de pantalla ni Monitorización de entrada. Accesibilidad se usa para consultar ventanas, recibir cambios de foco, activar ventanas y enviar el cambio nativo al escritorio solicitado.

## Actualizar desde AMSpace

En ajustes → **Aplicación**, pulsa **Buscar…**, revisa las novedades y elige **Instalar actualización**. AMSpace descarga, comprueba la firma, sustituye su paquete y se reinicia conservando tus preferencias. Si no hay novedades, lo indica. Puedes activar **Buscar actualizaciones automáticamente** para recibir avisos diarios; no se instala sin tu intervención.

El menú **Acerca de…** incluye la versión, autoría y el historial de versiones. La firma Ed25519 protege tanto el catálogo como los archivos; la clave privada permanece en el Llavero del desarrollador. Es independiente de la firma de Apple: mientras los paquetes sean ad hoc, macOS puede exigir renovar Accesibilidad tras actualizarlos.

## Compilar

Instala las herramientas de línea de comandos de Xcode y Rust con Cargo. Después:

```sh
git clone https://github.com/amartinrojarvis/amspace.git
cd amspace
./scripts/check.sh
./scripts/bundle.sh
open dist/AMSpace.app
```

Compilación universal:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/bundle.sh --universal
```

La aplicación en ejecución es Rust + AppKit/Core Animation. `scripts/icon.swift` es únicamente una herramienta de desarrollo para rasterizar el icono vectorial; no forma parte del ejecutable. El empaquetado descarga Sparkle 2.10.1 desde su repositorio oficial y verifica un SHA-256 fijado antes de integrarlo. Solo hay tres dependencias directas de Rust: `objc2`, `objc2-foundation` y `libc`.

Para regenerar los iconos: `./scripts/make-icon.sh`. Para firmar con tu certificado, define `SIGNING_IDENTITY` al ejecutar el empaquetador. La distribución notarizada requiere además configurar hardened runtime, firmar con Developer ID y enviar el paquete al servicio de notarización; este script no realiza esos pasos.

## Uso y comportamiento

El clic izquierdo selecciona el escritorio pulsado; pulsar el escritorio actual no hace nada. El clic derecho abre ajustes sin cambiar de escritorio. El cambio es directo, sin abrir la vista de Mission Control, y respeta el ajuste de desplazamiento natural. Para un destino lejano recorre los espacios intermedios y confirma cada cambio. El foco automático y el borde se pausan durante esa transición. No se modifican los atajos ni la configuración del sistema.

Los ajustes se organizan en **Ventanas**, **Escritorios** y **Aplicación**. En Escritorios elige Iconos y configura cada escritorio con su menú; puedes mezclar símbolos y números. Las elecciones se guardan por ID del escritorio, no por su posición, para acompañarlo al reordenarlo. Un escritorio nuevo obtiene su propia elección inicial.

En Ventanas, **El foco sigue al cursor** mueve el foco del teclado; **Traer al primer plano** controla por separado la elevación. Una ventana parcialmente cubierta puede recibir el teclado al pasar por su parte visible sin apartar las ventanas que la cubren.

| Estado | Indicador |
|---|---|
| Escritorio 1 de 3 | `1 · ·` |
| Escritorio 2 de 3 | `· 2 ·` |
| Diez o más escritorios | `2 / 12` |
| Aplicación a pantalla completa | `▣` |
| API de escritorios no disponible | `—` |

Los números son relativos a los escritorios normales de la pantalla activa, en el orden de Mission Control. Las ventanas a pantalla completa no se cuentan como escritorios normales. El orden puede cambiar si macOS tiene activado reordenar Spaces por uso reciente.

El borde sigue la ventana enfocada por AMSpace y respeta su posición en la pila; fuera de ese seguimiento, usa la ventana normal delantera de la aplicación activa; distingue ventanas de una misma app y recibe eventos de creación y cierre. No rodea la propia ventana de ajustes de AMSpace.

Los ajustes admiten navegación con teclado y etiquetas de Accesibilidad. Con la app activa: `⌘,` abre ajustes, `⌘W` cierra su ventana y `⌘Q` sale. Cerrar los ajustes deja el indicador en funcionamiento.

## Eficiencia

- Sin sondeo de ventanas con el foco automático desactivado. La búsqueda automática de versiones, opcional y desactivada inicialmente, se programa una vez al día.
- Spaces se consulta al cambiar de escritorio, aplicación o pantalla y al abrir ajustes.
- El borde identifica la ventana por su ID de WindowServer. El movimiento se aplica directamente desde la notificación del compositor, sin redibujar ni esperar a Accesibilidad. El tamaño se actualiza en el siguiente turno del hilo principal, sin demora artificial; no hay un bucle de animación permanente.
- El foco opcional utiliza un temporizador tolerante cada 80 ms. Solo realiza hit-testing AX y de WindowServer después de que el cursor se detenga, una vez por posición; no escanea continuamente todas las ventanas.
- Se suspende el foco y se oculta el borde al dormir la pantalla, suspender el equipo o desactivar la sesión.
- Compilación optimizada con LTO, eliminación de símbolos y un único codegen unit.

Consulta [VALIDATION.md](docs/VALIDATION.md) para las mediciones locales y el alcance real de las pruebas; no son una garantía para todos los equipos.

## Limitaciones conocidas

macOS no ofrece una API pública para enumerar todos los Spaces. El módulo `spaces.rs` resuelve en ejecución tres funciones privadas de SkyLight, **solo de lectura**. No inyecta código, no modifica Dock y no requiere desactivar SIP. Si Apple cambia esas funciones o sus datos, el indicador presenta `—` y el resto de ajustes continúa disponible. El borde también utiliza funciones privadas para recibir eventos de ventana, consultar radios y mover únicamente su propio panel. No modifica la geometría de las ventanas de otras apps. Por esas dependencias, esta versión está pensada para distribución directa, no para la Mac App Store.

- Al añadir, eliminar o reordenar escritorios sin cambiar de Space, el indicador puede actualizarse en el siguiente cambio de aplicación/escritorio o al abrir sus ajustes.
- La geometría y el orden del borde se leen de WindowServer, con notificaciones AX como respaldo si la API privada no está disponible. Algunas apps y versiones de macOS pueden exponer metadatos o notificaciones incompletos. El código limita los tiempos de espera AX.
- El foco se aplica solo a ventanas estándar; se omiten diálogos, menús, escritorio y ventanas de AMSpace. Activación y elevación son independientes. La activación sin elevar usa símbolos opcionales de SkyLight y puede no estar disponible si Apple los modifica; no requiere desactivar SIP.
- El borde es un panel independiente: no se inyecta dentro de otras apps. El movimiento usa una ruta directa del compositor y el redimensionado es reactivo; no se garantiza sincronía atómica con todos los fotogramas de otra aplicación. Mission Control, Stage Manager, apps de ventanas flotantes y animaciones de pantalla completa pueden presentar transiciones visuales. El borde no intenta integrarse con sus animaciones privadas.
- El inicio de sesión requiere un paquete `.app` instalado. `cargo run` no es un método de instalación.
- Tras recompilar con firma ad hoc, macOS puede requerir **eliminar AMSpace de la lista de Accesibilidad y añadir de nuevo la copia de Aplicaciones**. Apagar y encender el interruptor no siempre renueva la firma guardada. En versiones recientes del sistema el panel puede llamarse «Control de dispositivos y acceso a datos».

## Desarrollo

```sh
./scripts/check.sh                         # formato, Clippy, pruebas y plist
./scripts/bundle.sh --universal             # compila, empaqueta y verifica la firma
dist/AMSpace.app/Contents/MacOS/amspace --diagnose
python3 scripts/benchmark.py <PID> --seconds 20
```

[Arquitectura](docs/ARCHITECTURE.md) · [Validación y pruebas manuales](docs/VALIDATION.md) · [Licencia MIT](LICENSE)

Las preferencias viven en el dominio `io.github.amartinrojarvis.amspace` de `NSUserDefaults`. Para desinstalar: desactiva inicio de sesión, sal de la app, elimina AMSpace de Aplicaciones y retira su permiso de Accesibilidad si lo deseas.

## Autoría

Creado por **Alberto Martín** · [Instagram @amartinro](https://www.instagram.com/amartinro/) · [X @amartinro](https://x.com/amartinro).

[Publicación de actualizaciones](docs/UPDATES.md) · [Licencia de Sparkle](docs/SPARKLE-LICENSE.txt) · [Atribución del protocolo de foco de yabai](docs/YABAI-LICENSE.txt) · [Licencia del protocolo de gestos de iss](docs/ISS-LICENSE.txt) · [Atribución de strafe](docs/STRAFE-LICENSE.txt)
