# Desarrollo y diagnóstico de AMSpace

## Estado de referencia

Referencia al crear esta guía: **0.9.3, build 58**. El informe de traslado del otro equipo se aclaró como grupo de atajos desactivado. 0.8.0 añadió plantillas con nombre y protecciones para distribuciones editadas; 0.8.1 muestra también esas plantillas guardadas en el selector de ⌃⌥Espacio; 0.8.2 sigue la pantalla del puntero; 0.8.3–0.8.5 dejan el mosaico con una única escritura por ventana al volver a un escritorio; 0.8.6 reduce el trabajo por cuadro en reposo (temporizador 0,25 s y una sola lista del compositor por cuadro); 0.8.7 generaliza la membresía del asentamiento a huecos en el medio o id extra provisional; 0.8.8 comparte una sola lectura nativa de pertenencia por cuadro, acota la ráfaga de lecturas incompletas y hace converger el mínimo aprendido; 0.8.9 extiende ese memo al grupo de pestañas nativa; 0.9.0 hace conmutable el lote interpolado del mosaico, dibuja el marco de reelección con esquinas suaves sobre el `visibleFrame` y suma los paneles de servicio del sistema a las flotantes permanentes; 0.9.1 usa ese mismo marco cuando el traslado no cambia de destino; 0.9.2 acorta los plazos fijos de verificación, reentrada, pausa por Space y asentamiento de saltos sin añadir sondeos, y 0.9.3 agrupa ventanas del mosaico como pestañas (un hueco por tira, `tile.groups`). La verificación física en varias pantallas sigue pendiente en [VALIDATION.md](VALIDATION.md). La revisión de 0.9.3 aprobó 202 pruebas, con ocho diagnósticos/benchmarks excluidos, y la matriz robusta de 512 idas y vueltas del modelo sigue intacta. Los números describen esa revisión; no son un requisito fijo para versiones futuras. El registro de publicación y los límites físicos están en [VALIDATION.md](VALIDATION.md).

La aplicación pasó de indicador de Spaces, borde y foco a un gestor opcional de mosaicos con arrastre, proporciones personalizadas, rejilla virtual, flotantes y selector de ocho plantillas. Posteriormente se simplificaron ajustes y previews; 0.7.0 añadió la sección Atajos, prioridad opcional y traslados verificados con colocación del destino en segundo plano. [ARCHITECTURE.md](ARCHITECTURE.md) detalla los mecanismos y [NATIVE_INVARIANTS.md](NATIVE_INVARIANTS.md) explica las decisiones que evitaron regresiones.

## Entorno reproducible

Se necesita macOS, herramientas de línea de comandos de Xcode, Rust/Cargo, Python 3 y utilidades nativas de empaquetado. El mínimo de ejecución del bundle es macOS 13; consulta `Cargo.toml` para edición y toolchain declarado. CI usa Rust estable en `macos-15`. No hay build de aplicación para Linux/Windows.

```sh
cargo build --locked
./scripts/check.sh
./scripts/bundle.sh
```

El último comando produce `dist/AMSpace.app`; integra Sparkle mediante una descarga con SHA-256 fijado. Para un paquete universal:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/bundle.sh --universal
```

`cargo run --locked` permite desarrollar sin el framework de actualización integrado. Para inspeccionar la integración completa usa el bundle. `scripts/make-icon.sh` genera los iconos; sus herramientas Swift no forman parte del ejecutable de la app.

La app normal toma un bloqueo de instancia en `~/Library/Application Support/io.github.amartinrojarvis.amspace/instance.lock`. Una segunda instancia puede terminar correctamente sin aparecer porque la primera sigue abierta. Sal de la app anterior antes de probar otro ejecutable; no borres el archivo del bloqueo para intentar vencer un bloqueo activo.

Accesibilidad es necesaria para controlar ventanas. Una recompilación con firma ad hoc o un cambio entre `target/`, `dist/` y Aplicaciones puede alterar el reconocimiento de TCC. Comprueba la ruta realmente ejecutada y el diagnóstico antes de atribuir un fallo al algoritmo. La app no concede permisos por sí sola. No es necesario capturar la pantalla para obtener previews.

## Mapa del código

| Archivos | Trabajo que corresponde aquí |
|---|---|
| `src/main.rs`, `src/instance.rs` | Entradas de ejecución/diagnóstico y exclusión de instancias |
| `src/ui.rs`, `src/native.rs` | AppKit, ajustes, callbacks, coordinación de acciones y notificaciones |
| `src/spaces.rs`, `src/space_switch.rs`, `src/space_style.rs` | IDs y orden nativo de Spaces, navegación por gestos y presentación del indicador |
| `src/accessibility.rs`, `src/window_server.rs`, `src/focus.rs` | Propiedad CF/AX, identidad y geometría, observadores y foco de teclado |
| `src/border.rs`, `src/bar_background.rs`, `src/model.rs` | Dibujo/oclusión del borde, fondo del widget y presentación normalizada |
| `src/tiling.rs` | Modelo de árboles y plantillas, mínimos, proporciones y serialización |
| `src/tile_manager.rs` | `Workspace`, ventanas gestionadas, reconciliación, animaciones, restauración y traslados |
| `src/tile_mouse.rs`, `src/tile_picker.rs`, `src/layout_thumbnail.rs` | Intención del gesto, selector y simulación de plantillas |
| `src/custom_templates.rs` | Biblioteca validada de plantillas con nombre, sin IDs de ventanas ni cambios de pantalla |
| `src/tile_rules.rs` | Exclusiones por aplicación para mantener ventanas flotantes |
| `src/desktop_mode.rs` | Pausa por Mostrar escritorio/Mission Control y recuperación |
| `src/desktop_preview.rs` | Preview de geometría real con iconos, sin imágenes del contenido |
| `src/tile_hotkey.rs`, `src/shortcuts.rs` | Reservas Carbon, catálogo compatible, conflictos y tap prioritario |
| `src/window_move.rs`, `src/sky_bridge.rs` | Captura y confirmación de traslado; entrada opcional del puente SkyLight |
| `src/updater.rs` | Sparkle, búsqueda e instalación de actualizaciones |

## Preferencias e identidades

`NSUserDefaults` del bundle usa el dominio `io.github.amartinrojarvis.amspace`. Las preferencias se consultan con `pref`/`pref_or`: una clave ausente puede tener un valor inicial distinto de cero. No normalices la ausencia convirtiéndola indiscriminadamente en cero; la edición transaccional de atajos conserva esa diferencia al revertir.

| Dato o clave | Contrato |
|---|---|
| `Workspace = (DisplayID, SpaceID)` | Un mismo Space puede abarcar varias pantallas; no lo uses como única clave del mosaico |
| PID + ID de ventana | Identidad de proceso/ventana; no es portable entre sesiones |
| Número de escritorio mostrado | Posición de los escritorios normales en Mission Control; puede cambiar |
| `tile.layout.<SpaceID>.<DisplayID>` | Plantilla por contexto; tiene prioridad sobre la clave histórica `tile.layout.<SpaceID>` |
| `tile.templates.v1` | Diccionario UUID → nombre/distribución con huecos ordinales; biblioteca global, aplicación a una pantalla |
| `tile.off.<SpaceID>` | Exclusión de mosaico por Space |
| `tile.grid.*` | Activación y dimensiones de la rejilla virtual |
| `tile.mods`, `tile.key`, `tile.picker.*` | Atajos históricos: conservar índices y compatibilidad |
| `shortcuts.enabled`, `shortcuts.priority` | Activación del grupo nuevo y prioridad opcional |
| `shortcuts.desktop.mods`, `shortcuts.send.mods` | Modificadores de las series 1–9; no son IDs de Space |
| `shortcuts.left.*`, `shortcuts.right.*` | Combinaciones para cambiar una ventana de pantalla |
| `shortcuts.stay` | Ausente equivale a 1: permanecer en el origen al enviar a otro escritorio |
| `spaceIcon.<SpaceID>` | Icono persistido por ID, no por posición visible |
| `desktopPreview.disabled` | Clave invertida: ausente permite la preview |
| `tile.animate` | Ausente equivale a 1: lote con curva quintic; 0 escribe cada destino en una sola pasada |
| `flash` | Ausente equivale a 1: marco rojo suave de pantalla completa al reelegir el escritorio actual |
| `tile.groups` | Ausente equivale a 1: el tercio superior de un hueco agrupa la ventana arrastrada como pestaña de ese hueco |

Los IDs de pantalla actuales no son perfiles portables por UUID. Los valores persistidos de `Layout` están en `tiling.rs`: Espiral=1, Cuadrícula=2, Principal=3, División en 3=4, División en 4=5, Principal centrada=6, Centrada XL=7, Principal vertical=8. El antiguo valor 0 se interpreta como Espiral. `selection_index` es otra representación: usa las conversiones existentes.

## Diagnósticos versionados y efectos

Compila antes de usarlos. Los flags se procesan antes del bloqueo de instancia normal; que exista una app abierta no los impide ejecutar. Cierra la app normal durante fixtures de traslado para que no intente gestionar sus ventanas temporales.

| Comando | Efectos y condiciones |
|---|---|
| `target/debug/amspace --diagnose` | Lee permisos, Spaces, ventanas y geometría; no solicita un traslado. Requiere sesión gráfica para datos útiles |
| `target/debug/amspace --check-transfer-preflight` | Planifica llegadas con caché vacía y mosaico habilitado en preferencias de registro aisladas; no mueve ventanas ni escribe preferencias persistentes. Necesita AX, una ventana normal visible y destinos reales para cubrirlos |
| `target/debug/amspace --check-priority-shortcuts` | Instala y retira veinte taps con una combinación temporal; no inyecta teclas. Necesita AX y verifica ciclo de vida, no que el teclado físico venza a todas las apps |
| `target/debug/amspace --check-space-transfer` | Crea otro proceso con ventana temporal, la envía a los demás escritorios y de vuelta, y verifica geometría oculta. Necesita AX y al menos dos escritorios. Modifica la ventana fixture, no documentos del usuario |
| `target/debug/amspace --render-desktop-previews` | Lee geometría/iconos y escribe `.cache/desktop-preview-<SpaceID>.png`. Crea `.cache/` antes; no captura contenido de las ventanas |
| `python3 scripts/benchmark.py <PID> --seconds 20` | Mide CPU y RSS del proceso; ajustes cerrados y ratón en reposo para una muestra comparable |

`--space-transfer-fixture` es una entrada interna del diagnóstico: no es una segunda app de uso diario. Las pruebas ignoradas del código incluyen diagnósticos interactivos y benchmarks; lee su atributo `#[ignore]` y su implementación antes de ejecutar una concreta.

Trazas optativas disponibles: `AMSPACE_TRACE_SPACES`, `AMSPACE_TRACE_FOCUS`, `AMSPACE_TRACE_TILING` y `AMSPACE_TRACE_TRANSFERS`. Por ejemplo, con la instancia normal cerrada:

```sh
AMSPACE_TRACE_TRANSFERS=1 target/debug/amspace --check-space-transfer
```

Las trazas ayudan con IDs y estado de las operaciones; no añadas registro de teclas arbitrarias, títulos ni contenido. No publiques logs de una sesión sin revisar los metadatos que incluyen.

## Matriz de comprobación para cambios de ventanas

Selecciona casos relacionados con el cambio; no conviertas cada edición en una prueba física de todo el producto.

| Dimensión | Casos relevantes |
|---|---|
| Escritorios | Origen/destino 1, 2 y 3; destino vacío, ocupado, visitado o sin caché; reordenar, borrar y crear Spaces; fullscreen intercalado |
| Pantallas | Una, dos o tres; Spaces independientes y compartidos; principal/secundaria; a izquierda/derecha/arriba/abajo, coordenadas negativas y escalas distintas |
| Geometría | Horizontal y vertical; rotación, cambio de área útil, escala o desconexión durante una operación |
| Layout | Las ocho plantillas, rejilla activa/inactiva, divisiones editadas, mínimos imposibles, más o menos ventanas que el ejemplo |
| Ventanas | Flotante, exclusión por app, mosaico desactivado en destino, pestañas nativas, cierre/minimizado/modal durante el traslado |
| Entrada y ciclo de vida | Clic y atajo, permanecer/seguir, acciones rápidas, ratón pulsado, tap desactivado, entrada segura, pérdida de AX, dormir/despertar y Mostrar escritorio |
| Resultado | Pertenencia única, origen liberado tras confirmar, destino preparado sin activarlo, otros escritorios intactos y ninguna escritura diferida al destino anterior |

Ejecuta `./scripts/check.sh` para código. La prueba `keyboard_round_trips_cross_all_template_pairs_spaces_and_orientations` cubre la matriz de commits del modelo; las pruebas de topología están en `window_move.rs`. Usa `rg` para localizar regresiones de pestañas, oclusión, reentrada, mínimos y gestos antes de crear otra prueba equivalente.

Si un caso necesita hardware no disponible, indícalo. Un resultado correcto del modelo no confirma el compositor, los controladores, una renovación de TCC ni una instalación física por Sparkle.

## Diagnóstico por síntoma

| Síntoma | Comprobar primero |
|---|---|
| La app nueva no aparece | Instancia anterior y bloqueo; ruta del ejecutable/bundle |
| Cambia el número pero no el escritorio | Confirmación nativa de `space_switch`; no usar un setter de metadatos como sustituto del gesto real |
| No se mueve entre escritorios en otro equipo | Confirmar versión/arquitectura de macOS, versión de AMSpace, AX y estado de Atajos. `--diagnose` distingue `Accessibility` de `Native Space transfer`; macOS 13 como mínimo del bundle no garantiza que el puente privado esté disponible en todas las versiones |
| Capturas de pantalla al enviar | ⌘⇧1–9 incluye los atajos de captura ⌘⇧3/4/5. Probar «Enviar ventana a escritorio» con ⌃⌘ y verificar físicamente. No prometer que el tap venza a atajos del sistema ni concluir que esto explica por sí solo un fallo de ⌘⇧1/2 |
| Solo falla un destino | Footer de ajustes, pertenencia nativa, caché y clasificación de entradas auxiliares; no asumir conflicto del atajo |
| La ventana se ajusta al entrar | Plan, referencias AX retenidas, colocación oculta y rechazo de tamaño por la app; revisar el fallback de reentrada |
| Foco cambia al abrir preview | Panel activable, captura del ratón o foco/raise incidental |
| Ventana vuelve al monitor anterior | Cancelación de animaciones/verificaciones, pertenencia nativa y retención breve del monitor |
| AX no disponible tras compilar | Firma/ruta/TCC antes de cambiar el algoritmo; contrastar con el estado de la instancia abierta desde Aplicaciones, pues `--diagnose` ejecuta otro proceso; no resetear permisos globales |
| Error de descarga de Releases | Verificar también la URL pública del ZIP que usa el catálogo y sus bytes; un 504 del host alternativo no prueba un fallo de firma |

## Publicar trabajo

Para documentación: comprobar enlaces/rutas, revisar el diff, hacer commit y push a la rama acordada. No modificar binarios, versión ni catálogo para publicar solo estas guías.

Para una app nueva: [UPDATES.md](UPDATES.md) es el procedimiento de referencia. El artefacto de CI no sustituye al ZIP firmado con la clave del Llavero. Mantén alineados código, plist, notas y paquete; publica el ZIP inmutable y verifica su descarga antes de anunciar el feed. Registra el commit de código, CI, checksum, instalación comprobada y límites de validación. No conviertas datos locales ignorados en requisitos de publicación.

`AXIsProcessTrusted` consulta el proceso actual, según la [documentación de Apple](https://developer.apple.com/documentation/applicationservices/1460720-axisprocesstrusted). Los atajos de captura del sistema se describen en la [guía de Apple](https://support.apple.com/en-euro/guide/mac-help/mh26782/mac); un resultado diferente entre el diagnóstico y la app gráfica requiere contrastar sus estados, no asumir una causa de TCC sin comprobarla.

## Cambios de estabilidad en la candidata 0.8

La pausa por Space cancela baselines de gesto para que un mouse-up retrasado no confirme geometría anterior. La reentrada conserva las verificaciones de los contextos que realmente mantuvieron Space y área: su retry no debe empezar de nuevo tras cada cambio de otro monitor. `MembershipGrace` distingue una lectura incierta de una ventana positivamente confirmada en la instantánea nativa; esta última conserva su hueco aunque AX tarde más de 650 ms, sin recibir escrituras mientras sea incierta.

Las plantillas con nombre reutilizan la serialización ordinal de `Desktop`, validan el mismo número de huecos y se aplican mediante una propuesta antes de confirmar. No cambiar las preferencias globales de rejilla al aplicar un diseño en una sola pantalla. El guardado rápido histórico conserva su contrato anterior.

Para soporte de la instancia que está fallando, **General → Copiar diagnóstico** evita confundir su permiso con el de un proceso lanzado desde Terminal. Incluye un historial acotado de acciones resueltas, sin registrar teclas arbitrarias. El aviso genérico de tamaño no constituye un resultado de traslado.
