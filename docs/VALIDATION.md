## Validación 0.6.7

- Publicada [v0.6.7](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.7) sobre 1cb252e78d241cc4ed3f13eab2eea7a171ba6922. [CI](https://github.com/amartinrojarvis/amspace/actions/runs/42369017974) correcta. ZIP descargado sin autenticación idéntico al firmado; catálogo público idéntico al generado.
- Actualización real mediante Sparkle desde 0.6.6 completada: UI confirma 0.6.7; ejecutable instalado idéntico al universal publicado y firma estricta correcta. Preferencias de uso comparadas sin cambios. La UI requiere renovar Accesibilidad por el cambio de firma ad hoc; no se modificó ese permiso.

- Prueba local previa a publicar con la corrección instalada: Spark pid=2583, ventana=286 y Ghostty pid=774, ventana=46. Al minimizar Spark, Ghostty pasa a x=10, y=41, ancho=1660, alto=947. Al restaurarla, Ghostty queda en x=10 y Spark en x=845, ambas con ancho=825 y alto=947: hueco de 10 y sin solapamiento.
- 91 tests correctos, incluida regresión que impide clasificar escrituras AX rechazadas como restricciones permanentes. Formato, Clippy, plist y compilación universal verificados.

## Seguimiento local tras 0.6.6 (sin publicar)

- Observado Ghostty id=46 en x=10, ancho=1660 y Spark id=76 en x=845, ancho=825, ambas visibles en Space 1 y aptas para tiling. Preferencias activas, sin reglas de apps flotantes y UI con Accesibilidad concedida.
- Desactivar/reactivar tiling recuperó ambas ventanas: x=10 y x=845, anchos=825, y=41, alto=947. No se capturó el estado interno anterior; no se atribuye de forma concluyente a una referencia AX inválida ni a una restricción cacheada.
- Se refuerzan ambas rutas: recuperar la referencia AX si falla la geometría, renovar la referencia antes del reintento de colocación y no cachear escrituras AX rechazadas como límites permanentes. Los mínimos solo se aprenden tras una escritura final aceptada.
- 91 tests correctos, Clippy/formato/plist correctos, paquete universal compilado y copia de desarrollo instalada localmente conservando el número 0.6.6. No sustituye los artefactos publicados. Prueba nativa posterior: ambas ventanas siguen en sus mitades sin solapamiento.

## Validación 0.6.6

- Publicada [v0.6.6](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.6) sobre b74c12cf113d863967f3caf1d52f5a6b8c8a14da. [CI](https://github.com/amartinrojarvis/amspace/actions/runs/42368107317) correcta, incluido empaquetado universal. ZIP público anónimo idéntico al firmado y catálogo público idéntico al generado.
- Actualización real desde 0.6.5: Buscar…, Instalar actualización e Instalar y volver a abrir. UI confirma 0.6.6; ejecutable instalado idéntico al universal publicado y firma estricta correcta. Tras reiniciar, la UI requiere renovar Accesibilidad por la firma ad hoc; no se modificó ese permiso.

- Reproducido con Brave, Spark y Ghostty en un monitor de 1680 × 1019: Spark conserva 700 píxeles de alto ante un destino de 468 y solapa el tile de Brave.
- Corregida la redistribución a partir de los mínimos observados. Prueba real: Ghostty x=10 ancho=198, Spark x=218 ancho=758 y Brave x=986 ancho=684; todas con y=41 y alto=947, sin solapamientos y con huecos de 10 píxeles.
- Al cerrar la ventana temporal de Brave, Spark y Ghostty recuperan el espacio, sin solaparse.
- 90 tests correctos, un diagnóstico interactivo ignorado en la suite; Clippy sin advertencias. Prueba de regresión del mínimo de Spark y estabilidad de la distribución.
- No se ha reproducido por separado el caso de Brave ocupando toda la pantalla en el otro ordenador. El ajuste resuelve el solapamiento reproducido aquí. No se han probado físicamente Intel ni varios monitores en esta revisión.

## Validación de uso y publicación 0.6.5

El usuario confirma que el foco llega a la ventana correcta de Finder cuando otra ventana de Finder flota sobre los tiles, y que la revisión posterior elimina el destello del borde intermedio. La entrega verifica el foco y limita la corrección a dos reintentos; el borde conserva su objetivo durante la entrega. Transiciones de distribución ampliadas de 120 a 160 ms para repartir el movimiento en más fotogramas sin aumentar la frecuencia del temporizador. La matriz automatizada multipantalla no sustituye las pruebas físicas pendientes con varias escalas y orientaciones.

# Revisión local 0.6.5: borde pasivo y foco

- La 0.6.4 fue rechazada tras la prueba de uso: el usuario observó ausencias del borde y elevación ocasional de ventanas con el ajuste desactivado. Se restauró temporalmente la 0.6.2. Los muestreos de metadatos anteriores no eran suficientes para considerar resuelto el comportamiento completo.
- Se elimina toda ordenación relativa a ventanas ajenas (`orderWindow:relativeTo:` y `SLSOrderWindow`). El borde es un panel pasivo en nivel flotante, no activante y transparente al ratón. Una máscara Core Animation recorta la región cubierta por ventanas situadas encima del objetivo, sin cambiar el orden de ninguna de ellas. Se actualiza ante movimientos/orden de otras ventanas; esos avisos solo actualizan recorte, sin seleccionar ni activar ventanas.
- El recorte resta secuencialmente rectángulos y produce regiones disjuntas, evitando que dos ventanas superpuestas creen agujeros visibles por una regla par/impar. Las ventanas totalmente transparentes no se usan como oclusores; la opacidad se lee como decimal sin truncarla.
- Se retira el rechazo genérico de subroles AX desconocidos, que podía excluir ventanas reales de navegador. Solo se descartan roles auxiliares identificados expresamente (menú, tooltip, popover).
- Solo se omite una activación si coincide el proceso frontal y el ID concreto previamente activado por AMSpace. `AXFocusedWindow` no se considera prueba suficiente para saltarla: puede señalar una hermana obsoleta. La activación entre ventanas del mismo proceso se envía como tal aunque AX no identifique la anterior. Cancelar una entrega no vuelve a activar la ventana anterior; antes de completarla se validan aplicación frontal, posición del cursor y superficie bajo él.
- En la primera prueba de uso de 0.6.5 el usuario detectó que una flotante podía conservar el foco en lugar de un tile visible. Se encontró que el hit-test descartaba la identidad AX y usaba otra ventana del mismo proceso, y que un fallo temporal consumía el único intento por posición del cursor.
- El hit-test conserva ahora el ID AX si WindowServer corrobora que pertenece a una ventana visible bajo ese punto. Si el ID AX está obsoleto o fuera del punto, se usa la geometría nativa; no se sustituye una identidad corroborada por la primera ventana del proceso. Se filtran opacidad cero y pertenencia a Spaces no visibles, y se vuelve a comprobar el objetivo antes de entregar el foco diferido. Una lectura fallida permite hasta tres intentos, sin sondeo indefinido de esa posición.
- 89 pruebas automáticas correctas y un diagnóstico nativo de solo lectura ejecutado por separado; formato/Clippy/plist y compilación universal firmada verificados. El diagnóstico compara IDs nativos y la selección bajo el cursor sin activar ventanas ni leer títulos o contenido. Pruebas nuevas de recorte, opacidad, roles desconocidos, activaciones redundantes, flotante frente a tile, Spaces visibles y presupuesto acotado de reintentos. La validación física del caso concreto con flotante sigue en curso antes de publicar.

# Corrección local 0.6.4: oscilación del borde (retirada)

- Incidente reproducido con 0.6.3: ventanas inmóviles y panel del borde alternando delante/detrás de su objetivo, con cambios repetidos de orden. Captura de 308 muestras sin errores en `.cache/border-shake-before.json`. La muestra de pila y los metadatos se recogen sin títulos ni contenido de ventanas.
- Los avisos de orden del compositor ya no se reinterpretan como cambios de foco. Se mantienen movimiento/redimensionado del objetivo y las fuentes AX/NSWorkspace para foco y visibilidad. El panel visible se ordena mediante SLSOrderWindow opcional, con respaldo AppKit cuando hace falta mostrarlo o no está disponible la función.
- La barra de menús y los campos de texto no vuelven a escribir el mismo contenido; las notificaciones de pantallas solo invalidan el gestor cuando cambia la configuración real (identidad, frame, área útil o escala).
- Compilación local 0.6.4, build 13, universal y firma estricta verificadas. 82 pruebas correctas, formato, Clippy y plist correctos. Instalada en Aplicaciones, sin publicar una release 0.6.4. La traza temporal usada durante el diagnóstico se retiró del código final.
- Con borde activo, una medición de 302 muestras durante 12 s conservó un único estado y el borde exactamente sobre el rectángulo de la ventana izquierda, sin fallos de consulta. Se conserva el foco bajo cursor activado y el usuario renovó Accesibilidad para la copia final. Los muestreos adicionales registran también el Space real para distinguir cambios de escritorio durante la prueba.
- La medición de CPU sigue siendo elevada en este entorno (aproximadamente 46 % de un núcleo); no se presenta la estabilidad de orden observada como prueba de que ese consumo esté corregido ni como garantía universal de ausencia de cualquier parpadeo.

# Publicación e instalación de 0.6.3

- [Release v0.6.3](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.3) publicada con ZIP universal, catálogo firmado y SHA256SUMS sobre `70640c8df469573872df3bb69ad141d3e4e8a182`. [CI](https://github.com/amartinrojarvis/amspace/actions/runs/42348991525) correcta, incluyendo verificaciones y empaquetado universal.
- Descarga pública anónima del ZIP idéntica al artefacto firmado (`da1fef593a66a079c23d12322a848e22a41392d4569c71ed027df2cbcc9fe5ff`). El catálogo de main se publicó después de verificar la descarga; su contenido público coincide con el firmado y la firma Sparkle se verificó.
- Actualización real desde la app instalada 0.6.2 mediante Buscar… e Instalar actualización: Sparkle ofrece 0.6.3, descarga, instala y reinicia. UI confirma Versión 0.6.3; copia instalada idéntica al ejecutable universal publicado y firma estricta correcta.
- De las 18 preferencias comparadas, 17 no cambiaron y `tile.mods` pasó de 1 a 0 durante el intervalo. Hubo interacción concurrente con los ajustes; no se atribuye ese cambio al actualizador ni se sobrescribe la configuración actual. Se excluyen claves internas de AppKit, Sparkle y WebKit. La nueva UI indica que hay que renovar Accesibilidad para la firma ad hoc; no se modificó ese permiso durante la actualización.
- Respaldo previo en `.cache/AMSpace-before-0.6.3.zip`. Pendiente de validación física de los incidentes del usuario en Mullvad y hardware multipantalla, después de renovar el permiso.

# Preparación de 0.6.3

- Versión 0.6.3, build 12. `scripts/check.sh`: 81 pruebas correctas, formato, Clippy y plist correctos. Compilación universal Apple Silicon/Intel y firma estricta del bundle verificadas.
- ZIP y catálogo generados con `scripts/prepare-release.py`, firmados con la clave Sparkle del Llavero y verificados. SHA-256 del ZIP: `da1fef593a66a079c23d12322a848e22a41392d4569c71ed027df2cbcc9fe5ff`.
- Copia de la instalación previa en `.cache/AMSpace-before-0.6.3.zip` y preferencias guardadas en `.cache/preferences-before-0.6.3.plist`.
- La validación automatizada y sus límites físicos se detallan en las revisiones siguientes. La corrección específica del parpadeo de Mullvad sigue pendiente de prueba física.

# Revisión ampliada: borde, selector, monitores y animaciones (2026-11-07, desarrollo)

- El usuario confirma parpadeo del borde en Mullvad Browser incluso con la ventana quieta. Mullvad no está instalado en este entorno: se corrigen rutas compatibles con ese síntoma, sin presentar el incidente exacto como reproducido o resuelto físicamente.
- Borde: no se vuelve a escribir el frame si coincide con WindowServer, ni se repintan capas si grosor/color/radio/escala no cambiaron. Solo se reordena cuando cambia el objetivo, el panel está oculto o el orden real requiere corregirse. Se ignoran los eventos propios del panel para evitar realimentación. Las suscripciones se comparan por conjunto de IDs, sin renovarse por cambiar el orden de las ventanas.
- Se consideran candidatos sucesivos de la app activa y se descartan ventanas que AX confirma como no estándar, conservando la ventana normal que sigue debajo de popups auxiliares. Un fallo de la consulta del compositor admite un único reintento diferido para un objetivo del mismo proceso. No se consulta repetidamente el radio de una ventana sin cambiar de objetivo.
- Selector: abrirlo y preparar su confirmación usan una actualización del modelo sin colocar ventanas. Confirmar o restaurar una distribución cancela las escrituras anteriores de sus tiles, sin cancelar las de otros monitores. Evita aplicar o animar el reparto anterior antes del nuevo.
- Restauración/flotantes: la geometría original incluye su área de referencia y se remapea al trasladar entre monitores o al cambiar su orientación/área útil. Se limita al área nueva si no cabe; no se restaura hacia un monitor desconectado. Una ventana casi a medias en dos monitores con Space compartido conserva su monitor mientras los solapamientos difieran menos del 5 % del mayor, evitando cambios por 1–2 pt; una transferencia nativa o mayoría clara prevalece.
- Animaciones: todos los destinos pendientes empiezan a contar en su primer fotograma, evitando consumir los 120 ms durante la preparación AX. Una escritura intermedia fallida no se marca como aplicada. Se redondean bordes de forma consistente y se pospone la enumeración completa AX hasta finalizar la animación. Se conserva la curva quintic, el límite de 60 Hz y el sondeo de 12,5 Hz en reposo; no se añaden capturas, copias de texturas ni otro compositor.
- Borde y previsualizaciones siguen usando capas Core Animation, con composición acelerada por hardware según [Apple](https://developer.apple.com/documentation/quartzcore). La geometría de ventanas ajenas se solicita por AX: usar una capa GPU propia no elimina los tiempos de respuesta de otra aplicación. No se han medido FPS ni latencias de esta compilación en hardware multipantalla.
- Guardas adicionales ante ausencia transitoria de NSScreen al consultar cursor, convertir coordenadas, dibujar borde o abrir el selector. En ese estado no se intenta construir geometría con una pantalla nula.
- Matriz del modelo: 1.152 escenarios, 24 pasos por escenario (27.648 pasos): seis plantillas, seis áreas (horizontal, vertical, arriba, abajo, coordenadas negativas/fraccionarias y 640×480), cuatro configuraciones de rejilla y 1–8 ventanas. Combina selección de plantilla, resize, drops globales/locales, cierre/reincorporación, serialización y cambio del orden de enumeración. Comprueba identidades únicas, frames finitos/positivos, límites, cobertura de bordes exteriores, ausencia de solapamiento y conservación al reconciliar.
- `scripts/check.sh`: 81 pruebas correctas, formato, Clippy y plist correctos. Nuevas regresiones específicas cubren el orden del borde y sus propios eventos, arranque simultáneo de animaciones y fallos intermedios, cancelación por ventana al cambiar de plantilla, restauración entre áreas y estabilidad en el borde compartido.
- `cargo test --locked --release`: las mismas 81 pruebas correctas en modo optimizado. Paquete universal final regenerado; arm64/x86_64, firma estricta y diagnóstico nativo de solo lectura correctos. El diagnóstico solo expone un monitor físico. Sin instalar en Aplicaciones ni publicar estos cambios.
- Pendiente físico: Mullvad quieto con borde activo, abrir/cerrar popups y cambiar pestañas; varios monitores reales con escalas/Hz diferentes; rotación/desconexión durante arrastre; rendimiento/FPS y repetición prolongada de cambios de Space. Las pruebas automatizadas no certifican todos esos casos de hardware o aplicaciones.

# Revisión: recolocaciones al cambiar de escritorio (2026-11-07, desarrollo)

- Detectadas dos rutas que podían interferir con la transición nativa: reconciliar/aplicar posiciones durante el asentamiento de Spaces y conservar escrituras pendientes si el cambio llegaba con el ratón pulsado o una entrega pendiente. Corregidas ambas.
- Se conserva el árbol sin consultar/reconciliar ventanas ni escribir geometría durante 600 ms desde el último aviso. Los cambios consecutivos reinician la espera; el final de un cambio desde la barra también la reinicia. La consulta de escritorios visibles detecta cambios como respaldo ante notificaciones tardías/perdidas. Al reanudar se vuelven a leer pantallas y ventanas; los plazos de entrega/retención de monitor se prolongan para no agotarse durante la espera.
- `scripts/check.sh`: 74 pruebas correctas, formato, Clippy y plist correctos. Cuatro nuevas regresiones cubren cancelación de escrituras con ratón pulsado y conservación del árbol, cambios consecutivos, plazos de entrega entre Spaces y detección por consulta sin confundir orden o geometría.
- Paquete universal regenerado en `dist/AMSpace.app`, con firma estricta verificada. Durante la pausa tampoco se ofrece al selector un contexto de monitor/escritorio anterior ni se aplica una plantilla con ese contexto. Sin instalación en Aplicaciones ni publicación.
- Pendiente de comprobar físicamente el incidente del usuario, incluyendo gestos del trackpad, cambios consecutivos y traslados de ventanas entre Spaces en varios monitores. La pausa es una política temporal de protección, no una confirmación del compositor de que todas las animaciones hayan terminado.

# Segunda revisión: saltos al activar o usar ventanas (2026-11-07, desarrollo)

- El usuario informa de recolocaciones ocasionales al hacer clic o interactuar con Ghostty y Brave en la versión anterior; no había probado la compilación de la primera revisión. No se ha reproducido físicamente su incidente exacto.
- Corregido el uso de `dirty` como motivo para ignorar restricciones ya verificadas: las notificaciones de activación fuerzan una consulta, pero una geometría estable no vuelve a intentarse por ese motivo. Cambiar el destino, la geometría observada o elegir explícitamente una plantilla permite una nueva colocación.
- Un clic inmóvil o con hasta 3 pt de variación no inicia un movimiento/redimensionado del árbol aunque la aplicación o una escritura AX pendiente cambien la geometría. Se mantienen las pruebas de arrastres y redimensionados reales, incluidos cruces a pantallas verticales.
- Ventanas ya gestionadas con lecturas AX/Space inciertas o temporalmente fuera de pantalla mantienen su pertenencia con un margen de 650 ms de fallos consecutivos, comprobado en cada sondeo. En esa situación se cancelan sus escrituras pendientes y no se inicia otra colocación; se conservan orden y proporciones. Una recuperación limpia ese plazo. Un cierre confirmado o una transferencia nativa confirmada no retienen la hoja antigua.
- Un fallo al consultar la lista de WindowServer se distingue de una lista vacía y no borra el árbol. Animaciones y verificaciones comprueban PID e ID visibles; las verificaciones omiten ventanas ocultas/minimizadas. La reutilización de un ID por otro proceso elimina todo su estado pendiente antes de incorporarlo.
- `scripts/check.sh`: 70 pruebas correctas, formato, Clippy y plist correctos. Cuatro regresiones nuevas cubren activaciones repetidas con restricciones estables, clic inmóvil/selección sin movimiento, conservación de proporciones durante lecturas fallidas con caducidad/recuperación y limpieza del estado efímero de ventanas.
- Paquete universal final regenerado en `dist/AMSpace.app`: arquitecturas arm64/x86_64 y firma estricta verificadas. Diagnóstico nativo de solo lectura final correcto, con un monitor y geometría disponible. Sin instalación en Aplicaciones ni publicación de estos cambios.
- Pendiente: validar la interacción real de Ghostty y Brave con esta compilación y repetirla en varios monitores físicos. Estas pruebas del modelo no garantizan por sí solas la desaparición de todos los saltos.

# Revisión de estabilidad multipantalla (2026-11-07, desarrollo)

- `scripts/check.sh`: 66 pruebas correctas (cinco nuevas), formato, Clippy con warnings tratados como errores y plist correctos. Compilación universal Apple Silicon/Intel y firma estricta del paquete verificadas en `dist/AMSpace.app`. Estos cambios aún no están publicados ni instalados en Aplicaciones.
- Un destino nuevo elimina el reintento pendiente del anterior, evitando que una verificación antigua escriba coordenadas mientras se anima hacia otro monitor. Las verificaciones descartan ventanas cuya pertenencia nativa al Space ya cambió antes de registrar restricciones o volver a escribir.
- Cambiar la configuración de pantallas cancela animaciones, verificaciones, arrastres y entregas pendientes; elimina también destinos y restricciones ligados a la geometría anterior. Si el ratón seguía pulsado, no se captura otro arrastre a mitad del gesto. El siguiente reparto se aplica directamente durante el asentamiento.
- Los cambios del área útil de un monitor/escritorio también invalidan esas coordenadas aunque no llegue una notificación de pantallas. Reordenar la enumeración o cambiar únicamente de Space no se confunde con un cambio del área útil.
- La transacción AX termina reafirmando la posición después del último tamaño: algunas aplicaciones desplazan su origen al redimensionarse. Las pruebas simulan ese desplazamiento y fallos iniciales recuperables; se mantienen los reintentos acotados.
- Diagnóstico nativo de solo lectura correcto fuera del sandbox: un monitor visible, dos escritorios y geometría de ventana disponible. Dentro del sandbox el proceso termina al consultar SkyLight; no se interpreta como un fallo de la app en ejecución normal. La autorización AX observada corresponde al proceso de diagnóstico, no verifica la del lanzamiento gráfico.
- Pendiente de prueba física: arrastres repetidos entre monitores horizontal/vertical con escalas distintas, conexión/desconexión o rotación durante un gesto y cambios de Dock/barra de menús. Las pruebas automatizadas y la compilación no demuestran por sí solas que desaparezca toda inconsistencia en esos escenarios.

# Publicación de 0.6.2

- Versión 0.6.2, build 11. 61 tests, fmt, Clippy y plist correctos. [CI](https://github.com/amartinrojarvis/amspace/actions/runs/42308626666) correcta sobre `ff17f6de0a3e602b0363f68fa3a1c1f8f299be60`.
- [Release v0.6.2](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.2) pública con ZIP universal, catálogo firmado y SHA256SUMS. Descarga anónima idéntica al artefacto local. Catálogo público idéntico al firmado y firma Sparkle verificada.
- Actualización real desde 0.6.1 mediante Buscar… e Instalar actualización: descarga, extracción, instalación y reinicio correctos. UI confirma Versión 0.6.2. Preferencias de usuario conservadas; se excluyen claves internas de Sparkle/AppKit/WebKit de la comparación.
- La UI pide renovar Accesibilidad para la nueva firma ad hoc; no se modificó ese permiso. Respaldo previo en `.cache/AMSpace-before-0.6.2.zip`.

# Ampliación y sugerencias de atajos (desarrollo posterior a 0.6.1)

- 61 pruebas correctas, fmt/Clippy/plist sin errores y paquete universal firmado verificado. Instalada la compilación local; no se ha publicado otra release.
- Catálogo ampliado conservando índices anteriores; prueba de unicidad y preferencias fuera de rango. Prueba nativa de reserva exclusiva: un candidato ocupado no pasa el sondeo; liberarlo permite reservarlo de nuevo y el sondeo no deja la combinación ocupada.
- UI comprobada: Buscar atajos libres devuelve sugerencias, habilita Usar sugerido, y aplicarla actualiza los selectores y confirma Atajo listo. Probado ⌃⌥⇧Espacio; restaurado ⌃⌥Espacio al terminar.
- Se comprueban atajos de sistema y reservas globales. No se garantiza detectar atajos internos ni capturadores de teclado de otras aplicaciones. La búsqueda se ejecuta de nuevo en cada ordenador; no se ha probado físicamente el otro Mac del usuario.
- Respaldo anterior en `.cache/AMSpace-before-shortcut-suggestions.zip`.

# Publicación de 0.6.1

- Versión 0.6.1, build 10. 59 tests, fmt, Clippy y plist correctos. [CI](https://github.com/amartinrojarvis/amspace/actions/runs/42307264810) correcta sobre `41d2a4df3910370364c21b4ec0006b99be682814`.
- [Release v0.6.1](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.1) pública con ZIP universal, catálogo firmado y SHA256SUMS. Descarga anónima del ZIP idéntica al artefacto local; feed público idéntico al firmado y firma verificada.
- Actualización real desde 0.6.0 mediante Buscar… e Instalar actualización: descarga, extracción, instalación y reinicio correctos. UI confirma Versión 0.6.1. Preferencias de usuario conservadas; se excluyen claves internas de Sparkle/AppKit/WebKit de la comparación.
- La UI pide renovar Accesibilidad para la nueva firma ad hoc; no se modificó el permiso. Respaldo previo en `.cache/AMSpace-before-0.6.1.zip`.

# Corrección de animación y destino del selector (desarrollo posterior a 0.6.0)

- 59 pruebas correctas; fmt, Clippy y plist sin errores. Build universal y firma estricta verificados; instalada la compilación local con respaldo en `.cache/AMSpace-before-template-animation-fix.zip`.
- Una escritura intermedia rechazada ya no abandona el destino final. Los clics de ajustes/selector no cancelan las animaciones; se interrumpen al detectar movimiento o redimensionado real de una ventana.
- Fotogramas y destino AX usan coordenadas enteras calculadas por bordes compartidos. La animación usa una sola lista de ventanas visibles de WindowServer, eliminando dos consultas AX por ventana/fotograma.
- Sesión de geometría compartida por aplicación mediante Rc/Weak: suspende temporalmente AXEnhancedUserInterface solo cuando estaba activo y lo restaura al finalizar/cancelar la última transición de esa aplicación. Evita que AppKit interpole otra vez las escrituras del gestor. No solicita permisos ni cambia preferencias persistentes.
- Prueba interactiva: cuatro ventanas de Finder y una de Ghostty. Antes de la sesión de geometría, dos ventanas permanecían desplazadas. Con la corrección, tres columnas terminaron en x=10/568/1122 con las subdivisiones previstas; cuadrícula terminó en tres ventanas superiores y dos inferiores, con gaps de 10 pt, verificadas por WindowServer. Se aplicó la cuadrícula haciendo clic en el selector. Se restauró Espiral y se cerraron las dos ventanas temporales creadas para la prueba.
- No se han medido FPS ni consumo ni se ha probado en dos monitores físicos en este entorno. La duración programada sigue siendo 120 ms y la verificación final es independiente de la animación.

# Publicación de 0.6.0

- Versión 0.6.0, build 9. 58 pruebas, fmt, Clippy y plist correctos. [CI](https://github.com/amartinrojarvis/amspace/actions/runs/42305489910) finalizada correctamente sobre `4424b323dd7c6ff39902c5379afe61b28cb7f7d4`.
- [Release v0.6.0](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.0) pública con ZIP universal, catálogo firmado y SHA256SUMS. Descarga anónima del ZIP idéntica al artefacto firmado local.
- Catálogo subido después de publicar el ZIP; contenido del commit público verificado y firma Sparkle válida. La URL de main todavía devolvía el catálogo anterior desde la caché de GitHub: Buscar… en 0.5.0 mostró que estaba al día. No se considera probada la actualización completa por Sparkle para esta versión.
- Copia local 0.6.0 instalada y abierta, firma estricta y versión del diagnóstico verificadas; preferencias de usuario conservadas. Respaldo en `.cache/AMSpace-before-0.6.0.zip`.
- Se mantienen las limitaciones de validación física y las discrepancias de Finder documentadas a continuación.

# Selector de plantillas y depuración de monitores (desarrollo posterior a 0.5.0)

- 58 pruebas correctas; fmt, Clippy y plist sin errores. Paquete universal compilado y firma estricta verificada; copia local instalada en Aplicaciones, sin publicar otra release.
- Nuevas pruebas: selección por teclado, miniaturas horizontales/verticales, tres/cuatro divisiones adaptativas con 1–8 ventanas, cobertura y guardado/restauración, preferencias por monitor con fallback histórico, preview sin mutación y rechazo de opciones que no caben.
- Regresiones de traslado: tamaño automático en pantalla vertical no se interpreta como resize; movimiento fijado durante todo el gesto; frame antiguo no revierte la pertenencia del monitor; mínimo de aplicación que sobresale no provoca cambios de propietario; Space distinto o monitor retirado invalida retención; llegada sin zona usa plantilla de destino.
- Verificación AX diferida: se espera antes de declarar una restricción y se permite un único reintento. Las cancelaciones eliminan escrituras diferidas para no devolver ventanas a destinos antiguos.
- UI comprobada con Accesibilidad concedida en este lanzamiento: panel semitransparente, seis opciones, miniaturas de tres/cuatro columnas, selección con números/flechas, Escape y confirmación con Intro; la preferencia elegida se guarda por monitor/escritorio. Se abrieron y cerraron dos ventanas temporales de Finder; se restauró Espiral al terminar. Algunas posiciones/tamaños reales de Finder no coincidieron exactamente con el objetivo durante la comprobación; no se presenta la vista previa como prueba de cumplimiento de todas las restricciones AX de Finder.
- El registro de ambos atajos se comprobó y el despacho por EventHotKeyID tiene prueba propia. Las teclas sintéticas de la automatización no alcanzaron el manejador global; la apertura física con ⌃⌥Espacio se solicitó al usuario y no se considera confirmada. La apertura por ajustes sí se verificó.
- El entorno conectado solo expone una pantalla; no se ha reproducido físicamente la combinación horizontal/vertical del usuario. Las pruebas de varios monitores son del modelo. Quedan pendientes la prueba física del traslado y la desconexión/rotación durante un gesto.
- Respaldo de la versión anterior: `.cache/AMSpace-before-layout-picker.zip`.

# Preparación de 0.5.0

- Versión 0.5.0, build 8. Incluye el tiling y las correcciones desarrolladas después de 0.4.1.
- Comprobaciones de publicación: 43 tests correctos, fmt, Clippy y plist sin errores; compilación universal y firma estricta del bundle verificadas. ZIP y feed firmados con Sparkle y verificados antes de publicar.
- El usuario confirma que el comportamiento de la colocación inteligente parece correcto y autoriza publicar. Se mantienen las limitaciones de hardware indicadas en las comprobaciones siguientes.
- [CI de 0.5.0](https://github.com/amartinrojarvis/amspace/actions/runs/42301041838) completada correctamente sobre `64c13cf75045a8a68bee52f89664bc814179d08d`, incluyendo comprobaciones y empaquetado universal.
- [Release v0.5.0](https://github.com/amartinrojarvis/amspace/releases/tag/v0.5.0) publicada con ZIP, feed firmado y SHA256SUMS. Descarga anónima del ZIP idéntica al artefacto local. Feed público idéntico al firmado, verificado después de publicar.
- Actualización real mediante Buscar… desde la copia instalada 0.4.1: Sparkle detectó 0.5.0, verificó ambas firmas, instaló y reinició. La UI confirma Versión 0.5.0; el ejecutable instalado coincide con el publicado y pasa codesign estricto. Las 15 preferencias de usuario comparadas se conservaron (excluyendo claves internas de Sparkle/AppKit/WebKit).
- La UI de la versión nueva indica que requiere renovar Accesibilidad. No se modificó ese permiso. Respaldo previo en `.cache/AMSpace-before-0.5.0.zip`.


# Colocación inteligente y monitores (2026-11-06)

- 43 pruebas correctas; fmt, Clippy y plist sin errores. Nuevas pruebas cubren cuatro plantillas, las cuatro direcciones globales, cambio de eje, rejilla 120×6 y 12×6, persistencia tras reconciliar, intención local/central, transferencia entre árboles, destino vacío y cancelación de entrega pendiente.
- Coordenadas probadas con monitores a izquierda, arriba y debajo, áreas negativas y orientación vertical. Pertenencia por solapamiento comprobada con Space compartido y separado. Son pruebas del modelo; no equivalen a hardware real.
- Compilación universal y firma estricta verificadas. Instalada y abierta `/Applications/AMSpace.app`; respaldo anterior en `.cache/AMSpace-before-smart-drag.zip`.
- UI comprobada: Espiral, tiling y colocación inteligente siguen activados, y aparece la nueva ayuda de bordes/centro. La aplicación gráfica indica que requiere Accesibilidad; el proceso de diagnóstico lanzado desde terminal indica `true`, por lo que ese diagnóstico no se usa como prueba del permiso del proceso gráfico.
- Se abrió una ventana de Finder para intentar una comprobación interactiva y se cerró al terminar. El arrastre no se considera validado mientras la aplicación gráfica no tenga el permiso vigente. Solo hay un monitor conectado: pendiente la comprobación física de pantallas mixtas, distintas escalas, conexión/desconexión y entrega nativa entre monitores.

# Investigación de cierre y deriva de Espiral

- Al comenzar, AMSpace no estaba en ejecución. No se encontró un informe de crash de AMSpace en DiagnosticReports. El registro unificado del último proceso muestra `terminate:`, aprobación y finalización normal a las 22:40:04; esto no demuestra la causa de cualquier incidencia anterior.
- Relanzado con stderr en `.cache/amspace-runtime.log`; no apareció un error durante la comprobación inicial.
- Corregida la inserción en Espiral personalizada: antes dividía la ventana enfocada por su relación de aspecto y podía convertir tres ventanas en columnas. Ahora usa el slot final y continúa el giro según la división superviviente, incluyendo el cierre de la primera ventana.
- Eliminada la sustitución silenciosa por cuadrícula. Ajustar mínimos conserva la topología; seleccionar explícitamente una plantilla la restablece.
- 37 pruebas correctas, fmt y Clippy sin warnings. Build universal y firma estricta verificados; versión instalada y relanzada en Aplicaciones. El recorrido de interacción real sigue pendiente si macOS exige renovar Accesibilidad por la firma nueva.

# Conservación al cerrar ventanas y traslado entre Spaces

- Reproducido antes de corregir: una columna en Espiral ajustada a 928,33 pt volvía a 795 pt tras cerrar una de las dos ventanas apiladas a su derecha. La prueba falló con la versión anterior y pasa tras conservar el árbol ajustado y colapsar solo el nodo eliminado.
- 35 pruebas correctas, fmt y Clippy sin warnings. Incluyen cierre de cualquiera de las dos hermanas en Dwindle/Espiral/Principal con rejilla activa, nueva inserción después del cierre, y cancelación de animación con ráfaga de reconciliación limitada al cambio de Space.
- Quitada la espera de 400 ms del foco bajo cursor en el callback de tiling. Las activaciones de aplicaciones ya no cancelan las transiciones; solo lo hace el cambio real de Space.
- La limpieza usa una consulta global de IDs a WindowServer, no consultas AX síncronas a cada ventana fuera de pantalla. La llegada al escritorio aplica el destino directamente, sin otra animación. No se ha medido la latencia real de esta ruta.
- Bundle universal y firma estricta verificados; instalado y abierto desde Aplicaciones, con respaldo en `.cache/AMSpace-before-preserve-width.zip`. La UI muestra Accesibilidad pendiente para esta firma: falta la prueba física del traslado tras renovar el permiso.

# Rejilla virtual y regresiones de redimensionado (desarrollo)

- 32 pruebas correctas, fmt y Clippy sin warnings. Compilación universal y firma verificada; instalada en Aplicaciones con respaldo previo en `.cache/AMSpace-before-virtual-grid.zip`.
- Reproducción en el modelo de la disposición descrita por el usuario: una ventana a la izquierda, dos a la derecha. La inferior derecha responde tanto desde bordes compartidos como desde esquina exterior, y las restantes recuperan el espacio.
- Reordenación/intercambio desde los cinco destinos conserva exactamente las celdas y proporciones de una cuadrícula redimensionada, incluso tras reconciliación.
- Rejilla 120×6: divisiones alineadas, cobertura sin huecos, guardado/restauración con otros IDs de ventana, rechazo de datos corruptos y números de ventanas incompatibles.
- UI nativa comprobada mediante AX y captura. Introducir 1 columna muestra error; restaurados 120×6 y activada la casilla conforme a la petición. Selector y botones legibles.
- La compilación instalada sigue indicando que requiere Accesibilidad. No se ha ejecutado un arrastre real de Finder ni se presenta la simulación del modelo como validación de Finder. Pendiente esa comprobación una vez renovado el permiso.

# Corrección de flotantes, cobertura y animación eficiente (desarrollo)

- 28 pruebas correctas, fmt y Clippy sin warnings. Build universal y firma estricta verificados; nueva copia instalada y abierta desde Aplicaciones. La UI confirma que esta firma aún requiere Accesibilidad, por lo que no se ha validado físicamente el movimiento ni medido FPS/consumo.

- Regresión de flotante: moverla hacia cualquiera de los bordes o el centro no puede reinsertarla; solo la transición explícita del atajo la devuelve al mosaico.
- Regresión de animación cancelada: invalida el destino pendiente y se repara la diferencia entre geometría real y solicitada. Los límites estables impuestos por aplicaciones no producen un bucle de ajustes.
- Simulación de expansión limitada por la posición anterior: tamaño → posición → tamaño recupera todo el ancho disponible.
- Cobertura espacial de las cuatro plantillas tras redimensionar y eliminar 11 de 12 ventanas; no quedan huecos mayores que los gaps y la última ventana ocupa toda el área. Árboles demasiado profundos se compactan a cuadrícula.
- Curva quintic: extremos, aceleración/deceleración simétrica y monotonía. Las escrituras intermedias omiten atributos sin cambios. El temporizador vuelve a 80 ms tras terminar/cancelar el movimiento; 1/60 s solo durante actividad.
- No se ha medido FPS ni consumo de GPU; dependen del compositor y de las respuestas AX de las aplicaciones. La prueba del motor no sustituye esa medición.

# Redimensionado, animaciones y reglas flotantes (desarrollo)

- 20 pruebas correctas; fmt, Clippy sin warnings y plist válidos. Bundle universal Intel/Apple Silicon generado y firma estricta verificada; instalado en `/Applications/AMSpace.app`. Copia previa conservada en `.cache/AMSpace-before-resize.zip`.
- Nuevas pruebas: ajuste de fronteras en las cuatro plantillas sin solapamiento, límites de subárbol, bordes izquierdo/superior, conservación de proporciones tras reconciliación, simetría de cuadrícula con gaps, ausencia de deriva tras 100 muestras y extremos/monotonía de interpolación.
- Persistencia de reglas comprobada con NSUserDefaults en suite aislada: nombres iguales con IDs distintos, actualización sin duplicados y eliminación.
- UI real comprobada: Tiling → Apps flotantes → selector nativo; añadido Ajedrez (`com.apple.Chess`), comprobada su fila y eliminado después. Captura del listado inspeccionada, sin recortes.
- La nueva firma ad hoc vuelve a mostrar Accesibilidad no concedida. No se ha dado ese permiso automáticamente. Pendiente de prueba física: fluidez de las transiciones, redimensionado nativo con aplicaciones de distintos tamaños mínimos y apertura de ventanas de una aplicación marcada flotante. Las pruebas puras no sustituyen esa validación.

# Validación de tiling (desarrollo, sin publicar)

- `scripts/check.sh`: 15 pruebas correctas, fmt, Clippy sin warnings y plist válido. Build universal Apple Silicon/Intel y firma estricta del bundle correctos.
- Pruebas puras de las cuatro plantillas con 1–8 ventanas, áreas negativas de monitor, cobertura y ausencia de solapamiento; inserción lateral, intercambio, flotantes, eliminación completa, reutilización del árbol, división de ventana enfocada e independencia de escritorios.
- Diagnóstico nativo en esta máquina: permiso AX en el ejecutable de desarrollo, Space activo 1 y ventana visible asignada inequívocamente a Space 1. El bundle ad hoc de `dist` no hereda ese permiso.
- Pestaña Tiling abierta e inspeccionada mediante AX y captura: cuatro pestañas, ambos interruptores de ámbito, plantilla, modo inteligente y selectores de atajo legibles y sin recortes. Se mantuvo desactivado el gestor y no se cambiaron permisos. Después se volvió a abrir la instalación previa de Aplicaciones.
- Pendiente de validación interactiva con Accesibilidad concedida al bundle: arrastre real y previsualización, atajo flotante/retorno y conflictos, restauración de posiciones, convivencia con borde, minimización/cierre/fullscreen, dos pantallas físicas, desconexión de monitor y Mission Control. No se presenta la inspección de UI como prueba de esos comportamientos.

# Validación de 0.4.1

- El usuario reprodujo que la barra cambiaba al 2 y no regresaba al 1. El registro mostró que las selecciones posteriores también llegaban con ID 5 (escritorio 2); no aparecía la selección de ID 1.
- Con el botón nativo, el evento reenviado indicaba x=24, el centro del grupo de 48 pt. La selección ahora usa NSEvent.mouseLocation y la convierte mediante la ventana y la vista del botón.
- Diez pruebas correctas; fmt, Clippy y build universal sin errores. Firma estricta de la instalación verificada.
- Apertura normal de la versión final: renovada la autorización existente y observado «Accesibilidad concedida» en los ajustes.
- El usuario ejecutó físicamente 2 → 1 → 2 → 1 en la barra. El registro verificó x=39,16 → ID 5 → confirmado 5; x=10,95 → ID 1 → confirmado 1; x=33,75 → ID 5 → confirmado 5; x=13,63 → ID 1 → confirmado 1. La consulta independiente final indicó escritorio 1 activo. Esta prueba sí ejercita el botón de la barra.

# Validación de 0.4.0

- Diez pruebas correctas; fmt, Clippy sin warnings, plist y compilación universal verificados.
- Confirmado en macOS 27.2 el cambio real 1 → 2 (vacío) → 1 desde el menú de la aplicación. Esta comprobación no ejercitaba los clics físicos en los segmentos de la barra. El diagnóstico de SkyLight confirmó los IDs 5 y 1 tras las dos acciones, sin abrir Mission Control.
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
