## Preparación 0.6.18, build 36

- Preview reducida a la simulación: se eliminan título y contorno exterior del monitor. Cada monitor conserva una superficie azul muy tenue y separación entre pantallas; las ventanas mantienen sus divisiones azules e iconos centrados.
- La imagen conserva exactamente la proporción del conjunto real de pantallas y cabe en un máximo de 280×180 puntos. El popup suma solo seis puntos de margen por lado; no reserva espacio para cabeceras ni bandas vacías.
- Se mantiene el panel no activable y transparente al ratón, la aparición suave, el ajuste de desactivación y el enrutamiento existente de clic izquierdo/derecho.
- `scripts/check.sh`: formato, Clippy, plist y 149 pruebas correctas; ocho diagnósticos/benchmarks excluidos. Regresión nueva: aprovechamiento completo de la imagen en configuraciones horizontales, verticales y varias pantallas, conservando la proporción real.
- Bundle universal arm64/x86_64 con firma estricta correcta; ZIP y catálogo Ed25519 generados y verificados. Renderizado nativo de tres escritorios mediante `--render-desktop-previews`, con inspección de las imágenes de Brave y de Ghostty/Finder. Se observan posiciones actuales diferentes a la muestra anterior, sin usar ejemplos de plantilla ni capturar contenido.
- No se atribuye a este cambio una nueva prueba física del cambio de Space, renovación de Accesibilidad o varias pantallas reales; se conserva el comportamiento anterior en esas rutas.

## Publicación 0.6.17, build 35

- Publicada la versión solicitada: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.17 sobre `30439d837cd81690814148bcaa0e34200b115db0`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42519121650.
- ZIP universal, catálogo Ed25519 y SHA256SUMS publicados. Copia inmutable en `updates/0.6.17/AMSpace-universal.zip`, descargada por HTTPS sin autenticación y comparada byte a byte con el ZIP firmado. SHA-256: `1ee7d993cba41d82abc36e7e66398b8f025fd7a05aa88d30957d7f265c898142`; tamaño: 2473650 bytes.
- Instalación local de Aplicaciones actualizada a 0.6.17, build 35. Firma estricta verificada y ejecutable idéntico al del paquete final. Copia anterior conservada en `.cache/AMSpace-before-0.6.17.zip`. App abierta desde el ejecutable instalado.
- El catálogo firmado anuncia build 35 después de verificar la descarga pública. Se conservan tags, archivos y rutas de las versiones anteriores. La comprobación no incluye instalación por Sparkle desde la versión anterior ni renovación automática de Accesibilidad.

## Preparación 0.6.17, build 35

- Ajustes ampliados hasta 960×726 puntos, limitados por el área visible. Mosaico ya no contiene NSScrollView: sus ocho plantillas y controles se muestran completos, con escala uniforme cuando la pantalla ofrece menos espacio.
- Barra lateral convertida en dos listas nativas NSTableView (secciones y acciones), con iconos SF Symbols y selección de fila. Inspección visual y AX de Ventanas, Barra de menús, Mosaico y Acerca de; versión 0.6.17 observada y salida desde la lista/menú comprobada.
- Muestras de color rasterizadas en imágenes de 24×24, círculo de 18 puntos centrado, bordes redondeados y selección mediante contorno. Barra de menús inspeccionada visualmente: siete muestras alineadas y tres escritorios reales visibles sin recortar el tercero. Modo Iconos habilita los tres selectores; después se restauró Números. Interruptor de preview probado y devuelto a activado.
- Los escritorios para configurar iconos se consultan para todas las pantallas, con identidad persistente. El panel se refresca cada segundo mientras se consulta Barra de menús, y el temporizador se invalida al cerrar ajustes; no se añade sondeo en reposo con ajustes cerrados. No se ha añadido/eliminado físicamente un Space en esta comprobación.
- Preview de 264×172 puntos: retraso de entrada de 300 ms y fundido de 160 ms, sin activación ni interacción del panel. Tracking areas independientes de la ruta existente de clic. Cancelación al salir, pulsar, cambiar de escritorio, dormir o desactivar la opción. Se desactiva el tooltip nativo para evitar duplicación.
- Miniaturas construidas con geometría actual de WindowServer, pertenencia real a Spaces e iconos de NSRunningApplication, sin títulos ni captura de píxeles. Proyección de todos los monitores, con el escritorio solicitado en su monitor y los escritorios visibles en los demás. Con Spaces compartidos se representa el solicitado en todas las pantallas. Ventanas fijadas a varios Spaces se incluyen por pertenencia; tiling conserva la exigencia de pertenencia única.
- `scripts/check.sh`: formato, Clippy sin advertencias, plist y 148 pruebas correctas; ocho diagnósticos/benchmarks excluidos. Tres regresiones nuevas cubren límites del popup, conversión y recorte de coordenadas negativas y conservación de límites/posiciones relativos entre monitores.
- Compilación universal arm64/x86_64 y firma estricta correctas. ZIP y catálogo Ed25519 firmados y verificados con Sparkle.
- Renderizado nativo de solo lectura mediante `--render-desktop-previews`, ejecutado en el hilo principal de AppKit: tres imágenes generadas e inspeccionadas. La ventana real de Ghostty se representa con su geometría y su icono; los dos escritorios vacíos muestran solo el marco. No se capturan contenidos. Un intento inicial de renderizar desde un test fue descartado porque AppKit requiere el hilo principal.
- La candidata ad hoc no tiene Accesibilidad concedida: se observó el aviso correspondiente. No se presenta esta revisión como una prueba física del cambio de escritorio ni de arrastre; la ruta de clic existente se conserva y la preview ignora los eventos de ratón. La aparición al mover físicamente el cursor y varias pantallas reales quedan pendientes de comprobación del usuario.

## Publicación 0.6.16, build 34

- Actualización solicitada por el usuario: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.16 sobre `4a4c6a16fadd44dcc1572fc6b72b87ef5a448839`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42505309460.
- Incluye conservación de la distribución al entrar en un Space y la corrección del contorno residual. 145 pruebas correctas en debug/release, formato, Clippy, plist y paquete universal arm64/x86_64 con firma estricta.
- ZIP, catálogo firmado y SHA256SUMS publicados. Copia inmutable en `updates/0.6.16/AMSpace-universal.zip`, descargada sin autenticación y comparada byte a byte con el ZIP firmado. SHA-256: `09af7bb3ad3232c9b50e71a9df98f16b9ec52c2caad080b1b1f9e695a3b7bb2a`; tamaño: 2446043 bytes. Firmas de ZIP y feed verificadas con Sparkle antes de anunciar build 34.
- Se conserva la instalación local de prueba en build 33. Los doce cambios físicos de escritorio verificaron la estabilidad de la geometría en esa build; el arreglo adicional del contorno en build 34 tiene prueba automática y CI, pendiente de comprobación física en el otro Mac. No se atribuye una instalación por Sparkle ni una prueba del hardware remoto a esta publicación.

## Corrección de reentrada en Spaces 0.6.16, build 34

- Caso comunicado: saltan de tamaño las ventanas del escritorio de destino. La ruta anterior podía conciliar una lista parcial tras 600 ms y agotar otros 600 ms de conservación; quitar una ventana viva y reinsertarla altera temporalmente la distribución.
- Se añade una barrera de observación: instantánea coherente de WindowServer con ventanas visibles y no visibles, comprobación de pertenencia de las ventanas esperadas y estabilidad de identidades, áreas y geometría CG/AX durante 160 ms. Una lectura incompleta reinicia la observación sin modificar árboles. El plazo previo de protección se conserva, pero por sí solo ya no autoriza reanudar.
- Se diferencia ordered-in de onscreen, ocultación/minimización y sustitución de pestañas mediante AXTabGroup. La consulta nativa de ordered-in es opcional; su ausencia no desactiva el resto de WindowServer. Las referencias AX caducadas se recuperan por PID/ID antes de esperar. Se impide olvidar una identidad viva a partir de una única lista incompleta.
- Al reanudar se cancelan orígenes de animación y verificaciones anteriores a la transición, sin modificar distribución ni mínimos conocidos. No se añaden dependencias, hilos o temporizadores; las lecturas adicionales solo se realizan mientras hay una reentrada pendiente.
- Cinco regresiones nuevas: llegada incompleta incluso tres segundos después del cambio (ocho plantillas × dos orientaciones × dos rejillas); cambios de geometría, lectura fallida, cierre y retorno; cambios rápidos con dos pantallas; movimiento acumulado por debajo de la tolerancia entre muestras y cambios de área; metadatos CG con la clave opcional onscreen ausente. El estado vacío estable puede reanudar.
- `scripts/check.sh`: formato, Clippy sin advertencias, plist y 145 pruebas correctas; ocho diagnósticos/benchmarks excluidos por defecto. Suite release: 145 correctas. Cuatro diagnósticos nativos de solo lectura ejecutados expresamente y correctos: foco, puntero, instantánea CG y barrera de reentrada tanto sin caché como con referencias AX retenidas.
- Observación física en este Mac con 0.6.15: las listas CG mezclan brevemente ventanas salientes y entrantes; en el recorrido local con dos ventanas no se reprodujo el salto de tamaño comunicado. La prueba modelada sí reproduce la redistribución causada por quitar una ventana que reaparece. Prueba adicional de Finder: una pestaña inactiva tiene ordered-in=false; se cerraron las dos pestañas temporales y su ventana al terminar.
- Build 33 instalada y comprobada: CI correcta sobre `608058a7a0b106a2a1096065391369fd5de8e494`, https://github.com/amartinrojarvis/amspace/actions/runs/42504720276. Accesibilidad renovada por el usuario y confirmada en la interfaz de 0.6.16. Recorrido físico: doce cambios entre Spaces 1 y 5; tamaños y posiciones AX constantes para las tres ventanas observadas (990×947 y 660×947 en el primero, 1660×947 en el segundo). No hubo escrituras de geometría deducibles en ese recorrido.
- El usuario identificó un contorno residual. Se añade una corrección independiente en build 34: ocultación inmediata del borde y descarte de la identidad anterior en el cambio de escritorio, bloqueo de redibujo durante 600 ms y una única entrega diferida reemplazable, sin depender del mosaico. El test nuevo cubre cambios rápidos y callbacks antiguos. La corrección del contorno todavía no se ha probado físicamente.
- La instalación local se conserva en build 33; el paquete que se prepara para publicar es build 34. No se atribuye una prueba física al Mac remoto ni a varias pantallas; esos casos tienen cobertura del modelo. El catálogo público sigue en 0.6.15 hasta terminar la publicación solicitada.

## Publicación 0.6.15, build 32

- Release estable autorizada por el usuario: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.15 sobre `9436462b352e13a742af265153b78416ee5bff93`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42498838150.
- Incluye las optimizaciones de la candidata inicial y las tres correcciones de QA; 139 pruebas correctas en debug/release, dos diagnósticos AX nativos y cuatro benchmarks CPU ejecutados explícitamente. La validación y sus límites se describen en las secciones siguientes.
- ZIP universal, catálogo firmado y SHA256SUMS publicados. Copia inmutable en `updates/0.6.15/AMSpace-universal.zip`, descargada mediante HTTPS público sin autenticación y comparada byte a byte con el ZIP firmado. SHA-256: `debcd97d0f5fac40c880cbb2e08d7efd6afaa04b72b5091f50ea8a624e3cb797`; tamaño: 2418486 bytes.
- ZIP y catálogo verificados mediante Sparkle. Se anuncia 0.6.15, build 32, mediante el catálogo firmado de la raíz. Se conservan los archivos y tags de las versiones anteriores. No se atribuye una instalación física por Sparkle ni una prueba del hardware remoto a esta publicación.

## Comprobación de calidad 0.6.15, build 32

- CI completa correcta sobre `9436462b352e13a742af265153b78416ee5bff93`: https://github.com/amartinrojarvis/amspace/actions/runs/42498838150. Comparación final de solo lectura: Accesibilidad concedida y dos ventanas con idéntica elegibilidad/geometría AX entre instalada y candidata. Firma del paquete y del catálogo público verificadas; ejecutable instalado idéntico al ZIP publicado de 0.6.14.
- Tres fallos reproducidos y corregidos antes de publicar la candidata. La instalación y el feed permanecen en 0.6.14.
- Rejilla: al ajustar un padre, un hijo sin línea de rejilla disponible conservaba una proporción calculada sobre el tamaño anterior. Podía quedar por debajo de su mínimo, incluso después de `fit`. Regresión mínima con contenedor de 1000×1000, rejilla 2×2, padre de proporción 0,2 y mínimos del hijo de 90/250 puntos. Ahora se ajusta la división en el rectángulo nuevo antes de buscar una línea. Los huecos compatibles conservan su geometría.
- Datos guardados: se aceptaban un árbol centrado sin el núcleo principal correcto y ramas vacías no permitidas. Podían perder el ajuste guardado o dejar parte de la pantalla sin ocupar. El parser comprueba el núcleo centrado y los huecos permitidos (solo el reservado en una plantilla centrada con dos ventanas), antes de modificar el estado.
- Restauración: un archivo con el mismo número de ventanas pero incompatible con los mínimos en la pantalla actual devolvía éxito y cambiaba la plantilla/rejilla. Ahora se prepara y valida una copia (rectángulos completos, finitos, dentro de pantalla y con los límites duros), y solo después se confirma. La regresión verifica preferencias, modelo y verificaciones pendientes sin cambios al fallar, y restauración correcta en un área compatible. El mensaje de ajustes contempla compatibilidad de ventanas y pantalla.
- Suite ampliada: 38.400 operaciones reproducibles de altas, bajas, redimensionamientos, intercambios, cambios de orientación/rejilla/plantilla y guardado/restauración (cuatro semillas, ocho plantillas iniciales, 1200 pasos). Comprueba identidades, límites, ausencia de solapamientos, rectángulos completos y mínimos duros de 80×60. Otros tests conservan los mínimos heterogéneos conocidos de las apps.
- Datos dañados: 512 variantes deterministas de archivos guardados, entradas malformadas, identidades duplicadas, NaN y áreas nulas/no finitas. Los rechazos no modifican modelo ni preferencias. Se mantiene el catálogo de casos de varias pantallas/Spaces, Mostrar escritorio, foco, menús, modales y las 6.672 parejas de intercambio.
- Formato, Clippy, plist y `scripts/check.sh`: correctos, 139 tests y seis diagnósticos/benchmarks excluidos por defecto. Suite optimizada `cargo test --release --locked`: 139 correctos. Los seis excluidos se ejecutan explícitamente: dos diagnósticos nativos AX/puntero de solo lectura y cuatro benchmarks CPU, todos correctos. La previsualización con caché mantiene unos 69 ns por muestra en el fixture; no se extrapola a latencia total.
- Sintaxis de los cuatro scripts shell y de los tres scripts Python: correcta. Paquete universal arm64/x86_64 y firma estricta comprobados. Los casos del otro Mac siguen validados mediante el modelo, sin atribuirles una prueba física.
- Resumen estructurado: `qa-0.6.15.json`. Candidata build 32 preparada; no publicada.

## Revisión de eficiencia 0.6.15, build 31

- CI completa correcta sobre `6c4cd508ee61bb551d6535fc9a58a4fb33eb4441`: https://github.com/amartinrojarvis/amspace/actions/runs/42496626161. Confirmado que la instalación y el catálogo públicos siguen correspondiendo a 0.6.14 y que el ejecutable instalado coincide con su ZIP publicado.
- Revisión conservadora de consultas AX/WindowServer, conciliación de ventanas, preferencias, previsualización, borde, temporizadores y perfil de compilación. Se mantienen los controles de Space/monitor, protección de foco, Mostrar escritorio, geometría, reintentos y planificación existentes. No se introducen dependencias, hilos ni temporizadores.
- Perfil del proceso instalado 0.6.14: muestreo nativo de dos segundos con la mayor parte del tiempo en espera de eventos. Muestra de CPU/RSS de 20,33 segundos: 0,689 % de un núcleo y 36,83–37,06 MiB de RSS. Es una observación de referencia; no se usa como prueba de reducción global posterior.
- Foco de inserción diferido: una LazyCell local a la conciliación obtiene AXFocusedWindow como máximo una vez y solo si llegan ventanas a una distribución personalizada que usa ese dato. Espiral y las variantes centradas no lo usan. Regresión de equivalencia con la ruta anterior en las ocho plantillas, dos orientaciones, distribución normal/ajustada, distintas altas/bajas/órdenes y focos válidos/ausentes.
- Reglas flotantes: consulta directa del diccionario mediante el identificador nativo de la app; se evita convertir a String, enumerar nombres y ordenarlos para decidir pertenencia. La lista ordenada de ajustes permanece igual. Se comprueban cambios/eliminaciones inmediatos, valores inválidos, nombres vacíos y Unicode, y equivalencia con el catálogo anterior.
- Claves AX: dieciséis nombres habituales usan NSString constantes. Los nombres no reconocidos conservan el camino dinámico. Prueba de igualdad exacta de cada nombre para proteger las consultas de rol, geometría, foco, modales y estado.
- `scripts/check.sh`: 134 pruebas correctas y seis diagnósticos/benchmarks ignorados; formato, Clippy y plist correctos. Cuatro benchmarks CPU ejecutados por separado de los tests normales y en serie, con `cargo test --release --locked benchmark_ -- --ignored --nocapture --test-threads=1`: correctos.
- Benchmark de reglas con 64 entradas: 31.276 ns por comprobación al enumerar/ordenar y 424,9 ns con búsqueda directa. Preparación/comparación de claves AX: 155,0 → 18,8 ns por clave. Son costes aislados, no mejoras de latencia total ni mediciones de IPC. Datos, límites y referencia de reposo en `benchmark-0.6.15-optimizations.json`.
- Paquete universal arm64/x86_64 compilado y firma estricta verificada. Comparación nativa de solo lectura entre la versión instalada y la candidata (`--diagnose`): Accesibilidad concedida en ambos procesos; las dos ventanas candidatas coinciden exactamente en elegibilidad y geometría AX. No se mueven ventanas durante esta comprobación.
- Ejecutable universal: 1.472.592 bytes instalado frente a 1.472.608 bytes candidato (+16 bytes). La aplicación instalada permanece en 0.6.14; el paquete de la 0.6.15 está preparado para revisión, todavía sin publicar ni cambiar el catálogo firmado.

## Publicación 0.6.14, build 30

- Release estable autorizada: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.14 sobre `a1f0c7af10051e1303c83eb0dd87031a66c02513`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42494962010.
- ZIP universal, catálogo firmado y SHA256SUMS publicados. Copia inmutable en `updates/0.6.14/AMSpace-universal.zip`, descargada mediante HTTPS público sin autenticación y comparada byte a byte con el ZIP firmado. SHA-256: `5796f2227d5ae348d3a5c7bcd2613261ea04537219d1d3570342189ef65dc479`; tamaño: 2414680 bytes.
- ZIP y catálogo verificados mediante Sparkle antes de publicar. Se anuncia 0.6.14, build 30, mediante el catálogo firmado de la raíz. Las versiones previas conservan sus archivos y tags.
- El diagnóstico nativo de dos minutos terminó; contiene pulsaciones/liberaciones sin una previsualización de arrastre capturada. No se presenta como prueba física del parpadeo ni como medida de latencia total. El arreglo se fundamenta en el recorrido que mostraba y ocultaba el panel en el mismo tick y las regresiones descritas a continuación.

## Preparación 0.6.14, build 30: parpadeo y eficiencia del arrastre

- Causa concreta encontrada en 0.6.13: el tick mostraba el destino de un movimiento y después llamaba a `resize_drag(false, ..., preview=false)`, que ocultaba el mismo panel antes de comprobar el tipo de gesto. El cambio de 0.6.13 que omitía el cálculo del redimensionamiento al desactivar su previsualización introdujo esa interferencia.
- Ahora la función de redimensionamiento devuelve geometría y no modifica paneles. Solo el tick decide qué previsualización mostrar, una vez por muestra y únicamente mientras el botón está pulsado. Se desactivan las animaciones implícitas del NSPanel y solo se ordenan fuera los paneles que realmente están visibles. La pérdida breve de destino comparte la histéresis de 70 ms; al soltar se elimina sin espera cualquier destino inválido.
- Caché acotada a dos contextos y 32 propuestas por contexto durante un gesto. Conserva huecos y resultados, incluidos los rechazos, e invalida por pantalla/Space, área, identidad de la ventana, árbol, orden, plantilla, rejilla o mínimos. La confirmación final sigue calculándose contra el estado vigente; la caché no autoriza escrituras.
- Se elimina la consulta AX a la ventana enfocada en la captura. WindowServer y la geometría de candidatos próximos resuelven el propietario. Se filtran los escritorios inactivos antes de pedir geometría; un gesto clasificado no vuelve a consultar sus límites en cada tick. Se elimina otra lectura duplicada durante el movimiento.
- `scripts/check.sh`: 131 pruebas correctas y cuatro diagnósticos/benchmarks ignorados; formato, Clippy y plist correctos. Regresiones del recorrido de movimiento con previsualización de redimensionamiento apagada, falta de consultas nativas tras clasificar, separaciones breves/liberación y caché con todos sus cambios de contexto. Permanecen las matrices de ocho plantillas y varias pantallas/Spaces.
- Benchmark CPU del cálculo de previsualización, ejecutado con `cargo test --release --locked benchmark_drop_previews -- --ignored --nocapture`: 64.000 muestras, 6.867 ns por muestra reproduciendo el cálculo anterior y 74,2 ns con caché. Datos y alcance en `benchmark-0.6.14-preview.json`. No mide latencia total, renderizado ni IPC de AX/WindowServer; no se presenta esa mejora como aceleración global de la aplicación.
- Paquete universal compilado y firma estricta verificada. Build 30 instalada con copia de retorno de 0.6.13; la interfaz arranca. Diagnóstico de visibilidad nativa limitado a dos minutos, sin leer títulos/contenidos ni modificar ventanas; no se atribuye una prueba física del arrastre en el Mac remoto a esta revisión.
- Publicación autorizada por el usuario tras preparar la compilación.

## Publicación 0.6.13, build 29

- Release estable autorizada por el usuario: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.13 sobre `a9598ebecfd368b1e1e7a667393e7bd8c63d4a05`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42491144405.
- ZIP universal, catálogo firmado y SHA256SUMS publicados. Copia inmutable en `updates/0.6.13/AMSpace-universal.zip`, descargada por HTTPS público sin autenticación y comparada byte a byte con el ZIP firmado. SHA-256: `65e7362218122dd07b25803a188833abb569e06bf915764913c631ad917c7008`; tamaño: 2409614 bytes.
- ZIP y catálogo verificados con Sparkle antes de publicar. Se anuncia 0.6.13, build 29, mediante el catálogo firmado de la raíz. Las versiones anteriores conservan sus archivos y tags. No se atribuye una prueba física del Mac remoto ni una instalación por Sparkle a esta publicación.

## Preparación 0.6.13, build 29: filas y previsualización opcional

- El usuario informa de ausencia de previsualización al intercambiar con la fila central en el monitor vertical. Regresión reproducida en el modelo: tres bandas de 740/380/740 puntos con mínimos 690/240/690; 1 → 2 no produce propuesta en 0.6.12 aunque las tres ventanas caben ajustando las divisiones. El rechazo preventivo por hueco impedía esos destinos. El nuevo recorrido conserva los huecos compatibles y permite resolver los mínimos sobre el árbol de la plantilla al intercambiar.
- La propuesta parte del árbol con sus divisiones de rejilla ya aplicadas, evitando reconstruir el intercambio desde proporciones anteriores al ajuste. Si la distribución resultante es imposible, no modifica el escritorio.
- La matriz ampliada reproduce dos fallos adicionales: Espiral con siete ventanas omitía el ajuste de un mínimo por una diferencia numérica en el límite; Centrada con rejilla 17×9 podía dejar un lateral por debajo de su mínimo al recentrar. Se aplica tolerancia de 0,001 puntos al límite y se ajustan las divisiones internas de los laterales en sus rectángulos definitivos, sin desplazar la principal del centro.
- Tres filas/columnas: todas las parejas en ambas orientaciones, con y sin rejilla, y las 36 transiciones entre sus seis ordenaciones. Matriz adicional de 6.672 parejas (ocho plantillas, dos orientaciones, tres opciones de rejilla, 2–7 ventanas) con mínimos heterogéneos: identidad, límites, ausencia de solapamientos, mínimos, conservación exacta de huecos compatibles y persistencia al reconciliar. Se mantienen las regresiones de varios monitores/Spaces.
- Ajustes → Mosaico: interruptor «Previsualizar al redimensionar», preferencia global `tile.resize.preview`, desactivada cuando no existe. No afecta al indicador de destino del arrastre. Sin previsualización, el gestor no calcula propuestas de tamaño mientras el botón está pulsado; confirma y verifica al soltar.
- `scripts/check.sh`: 127 pruebas correctas y tres diagnósticos ignorados; formato, Clippy y plist correctos. Paquete universal compilado y firma estricta verificada. Build 29 instalada con copia de retorno de 0.6.12; ejecutable idéntico al paquete. Interfaz nativa inspeccionada: interruptor apagado, ocho miniaturas y controles sin solapamiento; Accesibilidad concedida tras refrescar la página. No se presenta el caso físico del Mac remoto como comprobado.
- Publicación autorizada por el usuario tras preparar e inspeccionar esta compilación.

## Publicación 0.6.12, build 28

- Autorizada por el usuario tras aceptar la candidata. Release estable: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.12 sobre `ecf4ae851343b5616a566b09118e0b387b284e4d`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42489695905.
- ZIP universal, catálogo firmado y SHA256SUMS publicados. Copia inmutable en `updates/0.6.12/AMSpace-universal.zip`, descargada mediante HTTPS público normal sin autenticación y comparada byte a byte con el ZIP firmado. SHA-256: `00e92ea53368de2b4cfcee8ab36c9911b9916896add44bb65cab95c8dbf1a169`; tamaño: 2.408.561 bytes.
- ZIP y catálogo verificados mediante Sparkle antes de publicar. Ejecutable instalado idéntico al del paquete. Se anuncia versión 0.6.12, build 28, mediante el catálogo firmado de la raíz. No se sustituye el ZIP ni el tag de 0.6.11.
- Se mantiene el alcance de validación descrito a continuación; no se atribuye una instalación física mediante Sparkle ni una comprobación del hardware remoto a esta publicación.

## Candidata 0.6.12, build 28: gestos nativos y traslados

- Arreglo común al gestor, sin excepciones por aplicación. Durante un redimensionamiento nativo no se escriben rectángulos AX ni se modifica la distribución guardada. Cada muestra genera una propuesta desde la misma distribución inicial y muestra los huecos previstos. Al soltar se confirma por la ruta normal de colocación, verificación y reintento limitado. La principal de las variantes centradas conserva su simetría.
- Un observador AppKit recibe únicamente pulsación/liberación del botón izquierdo; no intercepta eventos ni observa el teclado. Las coordenadas proceden del evento CoreGraphics y se resuelven sobre las pantallas actuales. El temporizador existente agrupa el movimiento. El observador se elimina al desactivar tiling, dormir, perder permiso o entrar en Mostrar escritorio/Mission Control.
- Se comprueba la geometría de las ventanas próximas al borde para resolver el propietario del gesto aunque el foco aún pertenezca a la vecina. Se conserva ese propietario durante el gesto. La liberación vuelve a calcular el hueco bajo el cursor, también en gestos breves y antes de vencer la histéresis de la previsualización.
- Una llegada desde otro monitor incorpora la ventana mediante la política de la plantilla de destino y ocupa un hueco de esa plantilla. Ya no inserta una división libre que invalide la estructura centrada/vertical. La propuesta incluye los mínimos conocidos de la ventana entrante antes de confirmar el traslado; se rechaza una propuesta imposible sin mutar el destino.
- Pruebas nuevas: secuencias de muestras con avance/retroceso desde los cuatro bordes de cada ventana en las ocho plantillas, horizontal/vertical y rejilla opcional; invariancia del estado inicial, ausencia de acumulación entre muestras, conservación tras reconciliar y permutar. Matriz de llegadas a las ocho plantillas con 0–6 ventanas existentes, todas las zonas de destino, proporciones previamente ajustadas, ambas orientaciones y rejilla. Regresiones del borde de la vecina, primer muestreo al soltar, destino rápido y mínimos de una llegada a Principal vertical.
- `scripts/check.sh`: 125 pruebas correctas, tres diagnósticos ignorados; formato, Clippy y plist correctos. Paquete universal arm64/x86_64 compilado y firma estricta verificada. Instalada localmente con copia de retorno de 0.6.11. La interfaz de la candidata muestra Accesibilidad pendiente; falta validar los gestos físicos tras renovar el permiso. Las combinaciones de varios monitores/Spaces se verifican mediante el modelo y las regresiones existentes; el caso físico del otro Mac no se presenta como comprobado.
- El usuario acepta la candidata y autoriza publicar 0.6.12. Se conserva la limitación anterior: no se atribuye una comprobación física al Mac remoto ni a combinaciones de hardware no disponibles.

## Reparación de la descarga de 0.6.11

- Tras el informe de error al descargar la actualización, se publica una copia inmutable del ZIP existente en la rama `updates`, ruta `0.6.11/AMSpace-universal.zip`. Descarga HTTPS pública normal, sin autenticación ni cabeceras especiales: correcta e idéntica byte a byte al ZIP firmado (SHA-256 `92cd8a00c50a6ef6b86c06f3dfdedc62e4fb83afb542df28786fddf035e2343e`). No se cambia el ejecutable, la versión, el tag ni la firma Ed25519 del ZIP.
- Se regenera y firma el catálogo con la nueva URL mediante `prepare-release.py --feed-only`, que verifica que el archivo existente coincide con el paquete compilado y no vuelve a empaquetarlo. Se verifican las firmas del ZIP y del catálogo. Se actualizan los metadatos de descarga y checksums de la release; el asset ZIP permanece intacto.
- El usuario confirma que la actualización vuelve a descargar e instalar correctamente con el catálogo corregido.

## Publicación 0.6.11, build 27

- Release: https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.11 sobre `ee13b0dcc5c5d650a059bbbc3851b719361acc5a`. CI completa correcta: https://github.com/amartinrojarvis/amspace/actions/runs/42468912904.
- ZIP, catálogo firmado y SHA256SUMS publicados. El ZIP público se descarga sin autenticación mediante la API oficial y es idéntico byte a byte al ZIP firmado local. SHA-256: `92cd8a00c50a6ef6b86c06f3dfdedc62e4fb83afb542df28786fddf035e2343e`. La ruta de descarga directa devuelve 504 desde este entorno; queda registrada esa limitación de red, sin atribuirle una validación de instalación completa mediante Sparkle.
- El ejecutable instalado localmente es idéntico al publicado. Las comprobaciones físicas pendientes de redimensionado e intercambio de la build 27 permanecen detalladas debajo. Se revisará aparte el nuevo informe de Espiral no redimensionable, recibido durante la publicación.

## Preparación de publicación 0.6.11, build 27

- CI inicial supera formato, Clippy y pruebas, pero dos intentos fallan al descargar Sparkle por HTTP 504. Se añade una ruta alternativa por la API oficial al mismo asset, con reintentos acotados y el mismo SHA-256 fijado. Prueba desde caché vacía: se reproduce el 504, la alternativa descarga el archivo correcto y se verifica antes de moverlo a caché y extraerlo. No cambia el ejecutable preparado ni las firmas del paquete de publicación.

- Comprobaciones finales: 120 pruebas correctas, tres diagnósticos/microbenchmark ignorados, formato, Clippy y plist correctos. Paquete universal recompilado para arm64 y x86_64; firma estricta del bundle correcta. ZIP y catálogo Sparkle firmados con la clave existente del Llavero y ambas firmas verificadas. Las limitaciones de validación de uso real permanecen detalladas en las secciones siguientes.

## Redimensionado y tamaños por posición (build 27)

- Las ocho plantillas se prueban redimensionando cada ventana en pantallas horizontal/vertical, con rejilla desactivada, 120×6 y 17×9. Se comprueban cambios de geometría, límites, ausencia de solapamiento, intercambio de ventanas conservando exactamente los rectángulos de los huecos y guardado/restauración.
- En Centrada, Centrada XL y Principal vertical se iguala el espacio a ambos lados de la principal. Los mínimos asimétricos y la rejilla no desplazan su centro; cuando no hay una distribución simétrica válida no se escribe una geometría imposible. Con dos ventanas se reserva el lateral libre para mantener la principal centrada; los huecos vacíos se serializan sin inventar identificadores de ventana.
- El redimensionado de la principal se calcula desde el desplazamiento del cursor, incluido un primer muestreo tardío, para evitar que su propio frame simétrico realimente el gesto nativo. Los vecinos y la principal se actualizan durante el gesto. El resto de gestos mantiene el control nativo de la ventana arrastrada.
- `scripts/check.sh`: 120 pruebas correctas y tres ignoradas por defecto; formato, Clippy y plist correctos. Pendiente de validar en vivo el redimensionado y el intercambio con cinco ventanas. La última captura de build 26 muestra los mismos dos Finder superiores antes y después del intento de intercambio; se conserva esa limitación y no se declara validado. La build 27 incorpora registro optativo de captura/liberación/commit y límites, con IDs y geometría, sin títulos ni contenido y sin temporizador adicional.

## Recolocación de plantillas y ampliación de la galería (build 26)

- Regresión reproducida antes del arreglo: aplicar Espiral con la ruta de propuesta del selector y arrastrar sobre el borde de un hueco generaba una nueva división a todo el ancho, en lugar de usar ese hueco. La misma prueba pasa tras el cambio. El arrastre de una ventana ya gestionada resuelve el hueco virtual bajo el cursor (también bordes y separaciones), y permuta las identidades sin crear divisiones. Conserva proporciones y rejilla; una permuta incompatible con los mínimos se rechaza sin mutar el escritorio. Las propuestas vacías se rechazan también en los traslados.
- Centrada XL: siete ventanas, principal centrada de dos tercios del ancho y tres por lado. Principal vertical: cinco ventanas, principal de dos tercios del alto y dos ventanas arriba y dos abajo. Los identificadores de preferencias anteriores 1–6 se mantienen; se añaden 7 y 8. El selector incluye ocho opciones, flechas y teclas 1–8.
- Se amplían las matrices de vida de ventanas y Mostrar escritorio a ocho plantillas. Prueba de propuesta → arrastre al borde/centro → soltar → reconciliar, horizontal/vertical y rejilla opcional; matriz adicional con cinco contextos en tres monitores y varios Spaces (incluido un Space compartido), plantillas y rejillas mezcladas, verificando aislamiento y guardado/restauración de la distribución.
- Se aumentan los márgenes de las tarjetas y controles, se reducen las miniaturas y se adapta el panel al área visible. Las pantallas estrechas usan tres columnas y desplazamiento si es necesario; las amplias, cuatro columnas. Build 25 inspeccionada en la app: ocho tarjetas y controles sin solapamiento.
- Prueba física de build 25 tras renovar Accesibilidad: el usuario aplica Principal centrada con su atajo real ⌃⌘⌥Espacio. El arrastre permuta los Finder de izquierda y derecha; Ghostty mantiene exactamente x=531, y=41, 618×947. La muestra posterior contiene dos Finder nuevos y revela una regresión de crecimiento: los laterales se siguen dividiendo junto al foco (una ventana a la izquierda y tres a la derecha). El usuario lo confirma; se cierra esa copia sin publicar.
- Build 26: en las tres plantillas centradas, añadir o quitar miembros reconstruye los dos grupos laterales a partir de su orden, manteniendo las dos divisiones exteriores de la principal, incluidas las ajustadas manualmente. El árbol anterior solo aporta esas proporciones; no se insertan hojas junto al foco. Regresión 3 → intercambio → 4 → 5/7 ventanas reproducida antes del arreglo; incluye perfil real 1660×947 con mínimos 618×700 de la principal y 511×300 de las laterales, cierres y nuevas aperturas, con y sin rejilla.
- `scripts/check.sh`: 117 pruebas correctas y tres ignoradas por defecto; formato, Clippy y plist correctos. Build 26 instalada y Accesibilidad renovada: lectura AX de cinco ventanas confirma dos Finder a cada lado, todos dentro del área útil. Izquierda: x=10, 511×469 en y=41 y 511×468 en y=520; derecha: x=1159 con los mismos tamaños y alturas. Ghostty conserva exactamente x=531, y=41, 618×947. Pendiente de comprobar el intercambio con esas cinco ventanas y la secuencia de cierre/apertura tras el intercambio.

## Revisión ampliada 0.6.11, build 25: Mostrar escritorio, auxiliares y Espiral

- Primera copia local de prueba (build 20) rechazada por el usuario: la esquina activa no suspendía el tiling y las ventanas anteriores cambiaban al volver. Se cerró esa copia, sin publicar 0.6.11 ni actualizar el feed. Las señales AX de Dock y las notificaciones distribuidas no aparecieron en el registro de esa prueba; no se presenta su registro correcto como prueba funcional.
- Registro continuo de metadatos, sin títulos ni contenido: en macOS 27.2 la esquina superior derecha de Mostrar escritorio crea una ventana de WindowManager en nivel 18 que cubre la pantalla (1680 × 1050). Durante la transición aparecen ventanas de ese proceso en nivel 1500. Las ventanas existentes quedan desplazadas hacia los bordes, con su tamaño conservado, y vuelven a sus rectángulos al salir. Se confirmó con la acción física del usuario, con AMSpace cerrado.
- La nueva detección identifica proceso, nivel, opacidad y cobertura de una pantalla. Reutiliza las listas que ya consultan tiling, foco y borde: no añade un sondeo permanente en reposo normal. Mientras la presentación está activa solo se comprueba la salida cada 200 ms, con tolerancia, sin leer AX ni colocar ventanas; el temporizador se suspende al dormir la sesión.
- Se conservan árboles, proporciones, rejilla y mínimos por pareja pantalla/Space. Se cancelan colocaciones, gestos, previsualizaciones y entregas de foco pendientes, sin reconciliar pertenencia durante la pausa. Al salir hay un margen de 600 ms para la restauración nativa. Las ventanas nuevas permanecen flotantes por preferencia explícita del usuario, también si se abren antes de entregar el aviso de entrada; se compara con el último conjunto visible normal y con el inventario previo. Un fallo al leer el inventario de salida no habilita escrituras prematuramente.
- Regresión de ida/vuelta: cuatro contextos de dos pantallas horizontal/vertical y distintos Spaces, combinaciones de las ocho plantillas, rejilla activada/desactivada, divisiones ajustadas y mínimos aprendidos. Compara todos los rectángulos y los árboles serializados exactamente antes/después, incluidos contextos no visibles al salir. Las ventanas nuevas no entran en esos árboles.
- Obtener información de Finder observado localmente como AXStandardWindow con AXIdentifier `Info`; se excluyen Info/Inspector solo en Finder, sin títulos traducidos. Los modales se rechazan incluso si exponen subrol estándar. Se excluye permanentemente `com.apple.archiveutility`, confirmado en el paquete nativo de la Utilidad de Compresión.
- Menú contextual de Finder observado en nivel nativo 101; AXFocusedUIElement devolvía AXApplication, no AXMenu. La validación del objetivo examina también la presencia de un menú visible en el mismo snapshot de WindowServer, y bloquea activaciones diferidas. Se prueban barra de menús, preview y transparencia para evitar falsos bloqueos.
- Espiral: se retira la sustitución automática por columnas ante mínimos de apps. Los mínimos se propagan por el árbol elegido y por la rejilla; si no cabe, se rechaza esa plantilla. Pruebas con mínimo de Spark de 720 × 700 que conserva la forma de Espiral sin solapamiento, con y sin rejilla, y caso imposible deshabilitado en el selector.
- Ajustes rediseñados con navegación lateral; fondo estático de NSStatusBarButton con la paleta de Kairos Monitor (opacidad 0,24, radio 6), sin cambiar los botones de escritorio ni añadir temporizadores. La build 22 se inspeccionó visualmente en las cuatro secciones; selección de color comprobada, incluido repetir el color activo y volver a Sin color. La build 24 añade seis tarjetas nativas con miniaturas calculadas con el modelo real y cacheadas por orientación, sin temporizadores. Tarjetas inspeccionadas en la app instalada: seis miniaturas dentro de sus recuadros, nombres legibles, selección resaltada con el color de acento y Principal centrada con cinco ventanas. La copia instalada vuelve a indicar Accesibilidad pendiente tras recompilar; no se presenta esta inspección visual como prueba de colocación real.
- Principal centrada probada con tres, cuatro y cinco ventanas en horizontal y vertical: principal centrada de dos tercios del ancho, identidad de ventanas a ambos lados y restauración serializada. La matriz de ciclo de vida incluye la nueva plantilla. Preferencias estables 1–5 conservadas, nueva plantilla 6; Dwindle 0 pasa a Espiral y su árbol antiguo no se restaura como si fuera otra plantilla.
- `scripts/check.sh`: 111 pruebas correctas y tres diagnósticos/microbenchmark ignorados por defecto; formato, Clippy y plist correctos. Pendiente de comprobar esta segunda copia local con la esquina activa, carpetas nuevas y restauración real, antes de publicar. También pendientes la validación física del otro Mac con Hermes y dos monitores y el diálogo real de contraseña.

## Preparación 0.6.11: pertenencia de ventanas durante cambios de Space

- Incidente comunicado en otro Mac: al cambiar el escritorio del monitor horizontal, una ventana de Hermes Desktop en el vertical cambia de tamaño y vuelve al anterior. El vertical permanece en el mismo escritorio, con dos ventanas y plantilla de tres filas; no se está arrastrando ni trasladando ninguna ventana.
- Se localiza una ruta que permite esa redistribución: la pertenencia se calculaba con el frame de WindowServer antes de leer AX. Si una muestra transitoria no solapa el área del monitor, la ventana se omite de los miembros aunque siga viva y en el mismo Space; su compañera ocupa temporalmente el hueco. En Spaces compartidos, un desplazamiento del compositor también puede atribuirla a otro monitor.
- Ahora se lee AX antes de decidir la pertenencia y se reutiliza ese frame para clasificación, geometría original, reconocimiento de pestañas y colocación. Si la geometría de una ventana gestionada resulta incierta en su mismo Space, se aplica la gracia acotada existente y no se escribe su frame. Los cambios nativos confirmados siguen prevaleciendo; los Spaces no visibles se descartan antes de consultar AX.
- Regresión específica: monitor principal pasa de Space 7 a 9; el vertical permanece en Space 8 con área idéntica y dos ventanas en tres filas. El control con metadatos desplazados reproduce la expansión de la compañera; usando AX se conservan ambas celdas sin generar colocaciones. Regresión adicional de Spaces compartidos, traslado real y desconexión del monitor anterior.
- 99 pruebas correctas, formato, Clippy y plist correctos. La reproducción es del modelo y de la ruta de clasificación; no se dispone de una captura del estado del otro Mac. Pendiente de confirmar el síntoma real con Hermes Desktop y dos pantallas. No se atribuye el incidente a un mínimo de Hermes sin evidencia ni se añade una excepción por aplicación.
- Primera preparación 0.6.11, build 20, anterior a los casos adicionales de Mostrar escritorio y ventanas auxiliares. Incluye las mejoras de 0.6.10.

## Preparación 0.6.10

- Versión 0.6.10, build 19: 97 pruebas correctas; formato, Clippy y plist correctos. Compilación universal Apple Silicon/Intel y firma estricta verificadas.
- ZIP y catálogo Sparkle generados mediante `scripts/prepare-release.py`, firmados con la clave del Llavero y verificados. Incluye la revisión de rendimiento y protección del foco documentada a continuación, con su alcance y pruebas de uso pendientes.
- Sin instalar localmente ni presentar como completada la prueba de actualización desde la app anterior.
- Publicada [v0.6.10](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.10) sobre `66e2a5540550db3670b7a6f2107cc43ce3ef65c2`, con [CI correcta](https://github.com/amartinrojarvis/amspace/actions/runs/42454234936). ZIP público descargado sin autenticación idéntico al firmado. No se anunció en el feed durante la investigación del incidente multipantalla.

## Revisión local de rendimiento y protección del foco · 2026-11-08

- Compilación de desarrollo sobre 0.6.9, sin publicar ni sustituir la app instalada. Paquete universal en `dist/AMSpace.app`; firma ad hoc estricta verificada. Se conservan los intervalos del foco, animaciones y reconciliación de Spaces.
- Una muestra de tres segundos de la instancia instalada sitúa gran parte del trabajo del hilo principal en `NSStatusItem._updateReplicants`, capturas de capas y dibujo de `NSButtonCell`. La reproducción aislada con cuatro segmentos confirma un bucle de redibujado. Se conserva `NSButton`, sus acciones AX y el hit-testing existente; `drawRect:` dibuja directamente el título atribuido centrado, sin pasar por la celda.
- `scripts/benchmark-status.swift` compara ambos caminos en una aplicación separada, sin activar ni mover ventanas. Tras un segundo de calentamiento, la comparación final consume **42,02 % → 0,040 % de un núcleo** durante unos seis segundos por variante. La prueba exploratoria anterior dio 48,51 % → 0,021 %. Son medidas locales del reproductor AppKit, **no del consumo total de AMSpace ni una garantía en otras versiones de macOS**.
- Claves `NSString` constantes en diccionarios de WindowServer/Spaces y escrituras AX; recorrido de CFArray sin un `Vec` temporal, reteniendo su fuente; resolución de `_AXUIElementGetWindow` una sola vez. Microbenchmark de 640 000 ventanas sintéticas con cuatro atributos: **845,9 ns/ventana antes, 189,1–229,8 ns después** (73–78 % menos tiempo). No mide IPC AX ni latencia de interacción completa. Repetible con `cargo test --locked --release benchmark_window_metadata -- --ignored --nocapture`.
- El temporizador de tiling consulta Accesibilidad una vez por ciclo en lugar de dos y deja de enumerar pantallas para localizar el cursor mientras no hay gesto. La revocación del permiso sigue deteniendo el temporizador.
- Icono ICNS: **992 579 → 634 839 bytes**, sin pérdidas. Comparación de todas sus entradas: mismos píxeles descomprimidos, metadatos PNG y resoluciones; CRC correctos. `iconutil` extrae el resultado correctamente y una segunda optimización es idempotente. La regeneración ejecuta `scripts/optimize-icon.py`.
- Bundle universal, suma de archivos regulares sin duplicar enlaces: **5 097 014 → 4 739 354 bytes**, una reducción del **7,0 %**. El ejecutable universal pasa de 1 373 632 a 1 373 712 bytes; el ahorro de tamaño procede del recurso, no de retirar arquitecturas ni el actualizador.
- Incidente de foco comunicado durante la revisión: los mensajes de autenticación pierden el teclado. El foco bajo el cursor consulta la entrada segura global de Carbon y respeta `AXSecureTextField`, `AXModal`, diálogos y hojas de la aplicación activa. Se comprueba también al completar o reintentar una activación diferida y antes de enviar activación/desactivación nativa. No se consulta el contenido del campo.
- `scripts/check.sh`: **97 pruebas correctas**, formato, Clippy y plist correctos; dos pruebas ignoradas por defecto (diagnóstico interactivo y microbenchmark), este último ejecutado por separado. Nuevas regresiones de tipos/vida del CFArray y roles protegidos. Compilación Apple Silicon e Intel y firma del bundle verificadas.
- Pendiente: prueba de uso de un diálogo real de autenticación, inspección visual de números/iconos y acciones AX en la app completa, y medición antes/después de CPU/RSS y latencia de la app con idéntica carga. No se presenta el reproductor aislado ni las pruebas de roles como validación de esos escenarios completos.

## Validación 0.6.9: pestañas nativas

- Publicada [v0.6.9](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.9) sobre 7d5aba55df4915f08da2cf5cf35b1fe8f933d29f. [CI correcta](https://github.com/amartinrojarvis/amspace/actions/runs/42391615482). ZIP descargado sin autenticación idéntico al paquete firmado.

- Incidente comunicado en Finder y Ghostty: crear o seleccionar una pestaña produce una división temporal del mosaico y su posterior reversión. La reconciliación trataba el nuevo ID seleccionado y el ID inactivo conservado durante la gracia como dos ventanas distintas.
- Metadatos locales: Ghostty publica únicamente el ID 44 en AXWindows y mantiene el ID 100 fuera de pantalla, sin Space nativo. Finder reproduce la sustitución de ID al seleccionar pestañas (262 → 245); el grupo AX retenido antes y después es igual por CFEqual, aunque el AXWindow saliente ya no permite leer sus hijos. Al crear la primera pestaña no existía grupo previo: adopción inicial limitada a un único hueco vivo del mismo proceso, pantalla, Space y frame, ausente de AXWindows.
- Se conserva el hueco del árbol y la distribución ajustada al sustituir el ID seleccionado. Se transfieren restricciones, tamaños mínimos, geometría original y estado flotante. Las pestañas inactivas no se reinsertan por la gracia de pertenencia ni por el asentamiento de un Space. Los grupos conocidos se comparan por identidad AX, sin títulos ni inferirlos por tamaño.
- Versión 0.6.9, build 18: paquete universal compilado, firma estricta del bundle correcta, ZIP y catálogo Sparkle firmados y verificados.
- 95 pruebas correctas, un diagnóstico interactivo ignorado; formato, Clippy y plist correctos. Regresión de alternancia, estado flotante y mínimos, más todas las plantillas con proporciones modificadas en áreas apaisada y vertical.
- Las ventanas temporales de Finder creadas para el diagnóstico se cerraron. La prueba interactiva de Ghostty no estuvo disponible mediante la herramienta de UI; no se presenta la inspección de metadatos como prueba de uso de la corrección. Pendiente de validación de uso del usuario. Sin instalar localmente esta revisión.

## Validación 0.6.8: escritorios independientes

- Los cambios de Space cancelan únicamente las colocaciones pendientes de los escritorios que dejan de estar visibles. La pantalla secundaria conserva sus destinos, restricciones y distribución; se vuelve a comprobar la pertenencia después de la pausa nativa para cubrir notificaciones adelantadas.
- Los cambios de área útil por Dock/barra de menús invalidan solo la pantalla afectada. Las modificaciones físicas de pantallas, resolución, orientación o escala mantienen la invalidación global de coordenadas.
- Las decisiones de colocación y la verificación posterior usan geometría AX, en lugar de interpretar los límites del compositor durante el deslizamiento como desplazamientos que haya que corregir. Se reutiliza la lectura AX de la enumeración y la animación parte de ese mismo frame.
- Regresión automatizada con pantalla apaisada y vertical en Spaces independientes: conserva divisiones ajustadas, colocaciones pendientes, límites estables y destinos ya aplicados ante avisos repetidos, y cancela las operaciones si después cambia también el Space secundario.
- `scripts/check.sh`: 92 pruebas correctas, un diagnóstico interactivo ignorado; formato, Clippy y plist correctos. Pendiente de reproducir y validar físicamente el incidente en el otro Mac con dos pantallas. Versión 0.6.8, build 17; paquete universal compilado, firma estricta del bundle correcta, ZIP y catálogo Sparkle firmados y verificados. Publicada [v0.6.8](https://github.com/amartinrojarvis/amspace/releases/tag/v0.6.8) sobre 84c3993a83052c08097aace0fe340fb6f61f147d, con [CI correcta](https://github.com/amartinrojarvis/amspace/actions/runs/42389580483). ZIP público descargado sin autenticación idéntico al firmado. Sin instalar localmente esta revisión; el usuario probará la configuración multipantalla.

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
