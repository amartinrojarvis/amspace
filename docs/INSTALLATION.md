# Instalar AMSpace y conceder sus permisos

El instalador guiado está en la rama de desarrollo; todavía no forma parte de la publicación 0.9.11. Las versiones que lo incluyan ofrecerán `AMSpace-installer.dmg` junto al ZIP habitual en [GitHub Releases](https://github.com/amartinrojarvis/amspace/releases).

Requisitos: macOS 13 o posterior. El bundle universal contiene Apple Silicon e Intel; compilar Intel no acredita una prueba física en ese hardware. AMSpace es código abierto con [licencia MIT](../LICENSE).

## Copiar a Aplicaciones

1. Descarga el archivo desde la publicación oficial. No necesitas cuenta ni token de GitHub.
2. Abre el DMG y arrastra **AMSpace** sobre **Aplicaciones**. La ventana muestra los pasos y contiene **Léeme primero.rtf**, que se puede leer sin ejecutar la app. Si ya está abierta, sal de AMSpace antes de sustituirla.
3. Espera a que termine la copia y expulsa el disco.
4. Abre **Aplicaciones/AMSpace.app**, no la copia del DMG ni la de Descargas.
5. AMSpace aparece en la barra de menús, sin icono permanente en el Dock. **Clic derecho en el indicador** abre Ajustes.

Con el ZIP, descomprímelo y mueve `AMSpace.app` a Aplicaciones; los pasos de apertura y permisos son los mismos. Para actualizar una instalación existente, también puedes usar General → Buscar ahora.

## Si macOS bloquea la primera apertura

**La distribución actual está firmada ad hoc, sin Developer ID ni notarización de Apple.** El DMG no elimina esta limitación ni concede permisos automáticamente. Un checksum comprueba igualdad de bytes, no aprobación de Apple.

Solo si la descarga procede de la publicación oficial y confías en ella:

1. Intenta abrir la copia de Aplicaciones.
2. Abre **Ajustes del Sistema → Privacidad y seguridad**.
3. Usa **Abrir igualmente** para AMSpace y confirma la apertura cuando macOS lo solicite. Los nombres pueden variar según la versión del sistema.

**No continúes ante una advertencia de malware o de que la app dañará tu equipo.** Conserva el aviso y comunícalo al proyecto. No desactives Gatekeeper ni SIP, no elimines la cuarentena y no ejecutes comandos para evitar las comprobaciones del sistema. También puedes revisar y compilar el código localmente siguiendo el [README](../README.md#compilar).

## Conceder Accesibilidad

1. En la guía de AMSpace pulsa **Abrir Accesibilidad…**, o abre **Ajustes del Sistema → Privacidad y seguridad → Accesibilidad**. En algunas versiones el panel puede llamarse «Control de dispositivos y acceso a datos».
2. Activa **AMSpace**. Si no aparece, pulsa **+** y selecciona **Aplicaciones/AMSpace.app**. Autoriza el cambio con contraseña o Touch ID si macOS lo pide.
3. Vuelve a la guía y pulsa **Revisar permiso**. Si el sistema solicita reiniciar la app, sal y abre otra vez la copia instalada.
4. Activa Borde, Foco o Mosaico solo cuando lo desees: están desactivados inicialmente. Los nuevos atajos de pestañas se activan expresamente en Ajustes → Atajos.

El indicador de escritorios funciona sin Accesibilidad; gestionar ventanas y cambiar de escritorio con clic requiere el permiso. **No se solicita Grabación de pantalla ni Monitorización de entrada.** AMSpace no puede concederse Accesibilidad a sí misma.

Tras una actualización ad hoc, macOS puede conservar la autorización para la firma anterior. Si la copia instalada sigue indicando permiso pendiente, sal de AMSpace, retira su entrada antigua de Accesibilidad y añade de nuevo la copia de Aplicaciones. No restablezcas los permisos de otras apps.

## Privacidad y seguridad

- **Accesibilidad es un permiso sensible:** permite consultar y manipular ventanas de otras aplicaciones, incluidos títulos, posición y tamaño. Concédelo solo a software de confianza. Puedes revocarlo desde Ajustes del Sistema.
- **Sin cuenta ni telemetría.** Las preferencias se guardan localmente en `NSUserDefaults`. Las previews usan geometría e iconos, no imágenes del contenido de las ventanas.
- **Conexiones de actualización:** Sparkle consulta el catálogo y descarga paquetes de GitHub. La búsqueda automática está desactivada inicialmente y se controla en Ajustes → General. Estas descargas implican una conexión a los servidores, como cualquier descarga web.
- **Ed25519 verifica el canal de actualización**, pero no sustituye Developer ID ni notarización. La instalación necesita tu intervención.
- **Copiar diagnóstico** coloca información técnica en el portapapeles. Revísala antes de compartirla: puede incluir identificadores de ventanas, procesos, pantallas y preferencias. La app no la publica automáticamente.
- Las licencias de AMSpace y sus componentes se incluyen dentro del bundle, en `Contents/Resources`.

## Ayuda y desinstalación

La guía se vuelve a abrir desde **Ajustes → General → Instalación y ayuda**. Puedes desmarcar **Mostrar ayudas en los primeros usos**; los errores operativos no se silencian. Clic derecho o Control-clic en una pestaña muestra sus acciones. **Pausar Mosaico** conserva la distribución y detiene sus colocaciones; Reanudar está siempre en la barra lateral de Ajustes.

Para desinstalar: desactiva Abrir al iniciar sesión, sal de AMSpace, elimina la app de Aplicaciones y retira su permiso de Accesibilidad. No hace falta modificar permisos o preferencias de otras apps.

## Construir el instalador

```sh
./scripts/bundle.sh --universal
./scripts/dmg.sh
```

El resultado es `dist/AMSpace-installer.dmg`. El empaquetado usa `hdiutil`, Swift/AppKit, `textutil` y Finder, sin dependencias de terceros. **Necesita una sesión gráfica con Finder**; macOS puede solicitar permiso de Automatización al proceso que empaqueta. Ese permiso es de la herramienta de construcción, no un requisito para quien instala AMSpace.

El script trabaja en un volumen temporal, comprueba la firma del bundle y la integridad del DMG, y desmonta/limpia sus recursos. No modifica la app instalada, sus preferencias ni los archivos publicados en `updates/`. Si Finder no puede guardar la disposición, falla en lugar de entregar un instalador incompleto. CI comprueba la sintaxis y los tipos de sus fuentes, pero no simula una instalación interactiva.

Para preparar una publicación con DMG y firma separada, sigue [UPDATES.md](UPDATES.md). Sparkle mantiene el ZIP como paquete de actualización.
