<p align="center"><img src="assets/icon.png" width="128" alt="Icono de AMSpace"></p>
<h1 align="center">amspace</h1>
<p align="center">Tu espacio. Tu foco.<br>Un compañero discreto para la barra de menús de macOS, escrito en Rust.</p>

AMSpace muestra dónde estás con un vistazo: **`1 · ·`**, **`· 2 ·`**, **`· · 3`**. Haz clic en el indicador para abrir unos ajustes nativos, compactos y adaptados al aspecto claro u oscuro del sistema.

## Funciones

- **Escritorio actual.** Número activo y puntos medios para los demás escritorios de la pantalla activa. A partir de diez escritorios se compacta a `3 / 12`.
- **Borde de la ventana activa.** Contorno interior sin separación, con radio de esquina leído del sistema, transparente al ratón, color libre o cuatro paletas y grosor de 1 a 6 puntos, en pasos de 0,5.
- **Foco bajo el cursor.** Activa y eleva la ventana tras detener el ratón unos 240–350 ms. Se pausa al arrastrar, mantener ⌥ o trabajar en los ajustes de AMSpace.
- **Mission Control.** El panel del borde se oculta con la vista de escritorios y reaparece al volver al escritorio.
- **Inicio de sesión.** Registro con `SMAppService`, gestionado por macOS. Desactivado inicialmente.
- **Preferencias persistentes.** Sin cuentas, conexión de red, telemetría, servicios auxiliares ni navegador integrado.
- **Icono propio.** Ventana redondeada con un espacio destacado; fuentes vectoriales y versión `.icns` incluidas.

El borde y el foco están desactivados inicialmente: actívalos cuando hayas concedido Accesibilidad.

## Instalar

Requiere **macOS 13 Ventura o posterior**. El paquete universal contiene Apple Silicon e Intel; la prueba de ejecución local se realiza en Apple Silicon. Descarga el ZIP desde [Releases](https://github.com/amartinrojarvis/amspace/releases), descomprímelo y mueve **AMSpace.app** a **Aplicaciones** antes de abrirlo.

Esta primera versión está firmada **ad hoc**, sin certificado Developer ID ni notarización de Apple. Un archivo descargado puede requerir autorización explícita en Privacidad y seguridad. No desactives Gatekeeper ni SIP; también puedes compilarlo localmente.

1. Abre AMSpace. Solo aparece en la barra de menús, sin icono permanente en el Dock.
2. Haz clic en `1 · ·` para abrir los ajustes.
3. Pulsa **Accesibilidad…** y autoriza AMSpace en **Ajustes del Sistema → Privacidad y seguridad → Accesibilidad**. Si no aparece, añádelo con `+` desde Aplicaciones.
4. Vuelve a los ajustes de AMSpace y activa el borde o el foco. Si macOS solicita reiniciar la app, sal y ábrela de nuevo.
5. Activa **Abrir al iniciar sesión** si lo deseas. macOS puede solicitar confirmar el ítem de inicio.

**El indicador de escritorios funciona sin Accesibilidad.** No se necesita Grabación de pantalla ni Monitorización de entrada. Accesibilidad se usa para consultar la posición de ventanas, recibir cambios de foco y activar ventanas.

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

La aplicación en ejecución es Rust + AppKit/Core Animation. `scripts/icon.swift` es únicamente una herramienta de desarrollo para rasterizar el icono vectorial; no forma parte del ejecutable. Solo hay tres dependencias directas de Rust: `objc2`, `objc2-foundation` y `libc`.

Para regenerar los iconos: `./scripts/make-icon.sh`. Para firmar con tu certificado, define `SIGNING_IDENTITY` al ejecutar el empaquetador. La distribución notarizada requiere además configurar hardened runtime, firmar con Developer ID y enviar el paquete al servicio de notarización; este script no realiza esos pasos.

## Uso y comportamiento

| Estado | Indicador |
|---|---|
| Escritorio 1 de 3 | `1 · ·` |
| Escritorio 2 de 3 | `· 2 ·` |
| Diez o más escritorios | `2 / 12` |
| Aplicación a pantalla completa | `▣` |
| API de escritorios no disponible | `—` |

Los números son relativos a los escritorios normales de la pantalla activa, en el orden de Mission Control. Las ventanas a pantalla completa no se cuentan como escritorios normales. El orden puede cambiar si macOS tiene activado reordenar Spaces por uso reciente.

El borde se aplica a la ventana normal visible situada delante de las demás en la aplicación activa; distingue ventanas de una misma app y recibe eventos de creación y cierre. No rodea la propia ventana de ajustes de AMSpace.

Los ajustes admiten navegación con teclado y etiquetas de Accesibilidad. Con la app activa: `⌘,` abre ajustes, `⌘W` cierra su ventana y `⌘Q` sale. Cerrar los ajustes deja el indicador en funcionamiento.

## Eficiencia

- Sin temporizador recurrente con el foco automático desactivado.
- Spaces se consulta al cambiar de escritorio, aplicación o pantalla y al abrir ajustes.
- El borde identifica la ventana por su ID de WindowServer. El movimiento se aplica directamente desde la notificación del compositor, sin redibujar ni esperar a Accesibilidad. El tamaño se actualiza en el siguiente turno del hilo principal, sin demora artificial; no hay un bucle de animación permanente.
- El foco opcional utiliza un temporizador tolerante cada 80 ms. Solo realiza hit-testing AX después de que el cursor se detenga, una vez por posición; no escanea continuamente todas las ventanas.
- Se suspende el foco y se oculta el borde al dormir la pantalla, suspender el equipo o desactivar la sesión.
- Compilación optimizada con LTO, eliminación de símbolos y un único codegen unit.

Consulta [VALIDATION.md](docs/VALIDATION.md) para las mediciones locales y el alcance real de las pruebas; no son una garantía para todos los equipos.

## Limitaciones conocidas

macOS no ofrece una API pública para enumerar todos los Spaces. El módulo `spaces.rs` resuelve en ejecución tres funciones privadas de SkyLight, **solo de lectura**. No inyecta código, no modifica Dock y no requiere desactivar SIP. Si Apple cambia esas funciones o sus datos, el indicador presenta `—` y el resto de ajustes continúa disponible. El borde también utiliza funciones privadas para recibir eventos de ventana, consultar radios y mover únicamente su propio panel. No modifica la geometría de las ventanas de otras apps. Por esas dependencias, esta versión está pensada para distribución directa, no para la Mac App Store.

- Al añadir, eliminar o reordenar escritorios sin cambiar de Space, el indicador puede actualizarse en el siguiente cambio de aplicación/escritorio o al abrir sus ajustes.
- La geometría y el orden del borde se leen de WindowServer, con notificaciones AX como respaldo si la API privada no está disponible. Algunas apps y versiones de macOS pueden exponer metadatos o notificaciones incompletos. El código limita los tiempos de espera AX.
- El foco se aplica solo a ventanas estándar; se omiten diálogos, menús, escritorio y ventanas de AMSpace. Activar implica elevar la ventana.
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
