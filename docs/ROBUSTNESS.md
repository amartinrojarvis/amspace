# Batería interna de robustez — candidata 0.8.0

Ejecutada el 9 de noviembre de 2026 sobre el comportamiento de RC 1, build 44. Esta revisión añade pruebas y documentación; no cambia el ejecutable publicado ni anuncia una versión estable. Resultado local: **185 pruebas correctas, cero fallos, ocho diagnósticos/benchmarks excluidos**; formato, Clippy con warnings prohibidos y plist correctos mediante `./scripts/check.sh`.

## Alcance y resultados

| Batería | Casos ejecutados | Qué comprueba |
|---|---:|---|
| Traslados con distribuciones editadas | 129.024 | Ocho plantillas de origen × ocho de destino, 1–6 ventanas de origen, 0–6 de destino, rejillas independientes, origen horizontal/vertical y seis geometrías de destino. 123.328 propuestas válidas y 5.696 rechazos sin mutación. |
| Sesiones reproducibles | 64 × 500 acciones | Seis contextos en tres pantallas, dos Spaces por pantalla, coordenadas negativas y verticales. 1.201 traslados confirmados del modelo y 172 rechazados por mínimos. |
| Datos de plantillas alterados | 20.496 | Truncamiento en cada token, índices/huecos incorrectos, duplicados, valores negativos, enormes, no finitos y Unicode; 20.112 rechazos atómicos y 384 aceptaciones sintácticas comprobadas con serialización cuando su geometría es viable. |
| Cambios de topología | 5.832 | Todas las permutaciones de tres pantallas y pares origen/destino, horizontal/vertical, escalas 1/1,5/2, cambios de posición, rotación, área útil, escala, identidad y desconexión. Cambiar un participante invalida el plan; reordenar la enumeración o cambiar un tercero no lo invalida. |
| Fallos de colocación | 56 comprobaciones terminales y 10.000 avisos repetidos | Antes, en y después del plazo; escritura aceptada/rechazada, un único retry, fin como límite/rechazo y ausencia de reinicio mientras destino y geometría observada no cambien. Sin geometría verificable no se propone escribir. |
| Biblioteca de plantillas | Límite completo de 64 entradas | Nombres Unicode de hasta 60 caracteres, duplicados sin sobrescritura, capacidad, eliminación inexistente, eliminación individual, alta posterior, recarga y datos excesivos. Usa un dominio de preferencias aislado. |

Las sesiones mezclan redimensionar desde los cuatro bordes, intercambiar huecos, enviar entre contextos visibles, abrir/cerrar, reutilizar un ID con PID nuevo, alternar flotante, minimizar/restaurar, cambiar un Space independiente, consultas AX inciertas con pertenencia nativa confirmada, nuevos mínimos, guardar/aplicar distribuciones y Mostrar escritorio. El conjunto guardado de distribuciones es sintético; la persistencia de la biblioteca con nombre se prueba por separado.

Después de cada acción se comprueba identidad y propiedad única, pertenencia al contexto correcto, coherencia de flotantes/ocultas, limpieza de estados al cerrar, geometría finita dentro del área, hojas completas y sin solapamientos cuando el diseño es viable. Los cambios de Space y Mostrar escritorio conservan los árboles; los traslados y las aplicaciones de plantillas conservan los contextos ajenos. Las negativas a aplicar una plantilla no cambian modelo ni rejilla global.

## Caso límite analizado

La semilla `0x8ff34785799e5cbd`, acción 410, cierra el hueco principal de una plantilla centrada modificada. El nuevo reparto exige dos laterales simétricos de al menos 1.178,33 puntos y un principal de 292,83, más dos separaciones de 10: supera los 2.540 puntos disponibles. Es una incompatibilidad con los mínimos, no una pérdida de ventana. La prueba distingue este bloqueo de una geometría inválida: mantiene identidades/plantilla, exige que sin mínimos la geometría sea válida y que el diseño inviable no pueda guardarse como plantilla. La ruta de reconciliación existente rechaza el cálculo incompleto antes de crear nuevas colocaciones y muestra el aviso de incompatibilidad; no sustituye silenciosamente el diseño elegido.

## Reproducción

```sh
./scripts/check.sh
cargo test --locked exhaustive_edited_template_transfer_matrix -- --nocapture
cargo test --locked seeded_sessions_mix -- --nocapture
cargo test --locked template_mutation_corpus -- --nocapture
cargo test --locked topology_permutations -- --nocapture
cargo test --locked placement_faults -- --nocapture
cargo test --locked library_capacity_unicode -- --nocapture
```

Las seis pruebas están versionadas en `src/tile_manager/robustness_tests.rs`, `src/window_move.rs` y `src/custom_templates.rs`. Las semillas de las sesiones son `n.wrapping_mul(0x9e3779b97f4a7c15)` para `n=1..64`, con generador xorshift y selección de IDs ordenada. Un fallo indica semilla, paso, operación y contexto; puede reproducirse sin documentos ni preferencias del usuario. No hay dependencia de probes locales de `.cache/`.

## Límites y condición para la estable

Estas son pruebas del modelo y sus guardas reales: no crean ni mueven ventanas nativas, no inyectan fallos en procesos ajenos ni simulan el compositor de macOS. La referencia AX usada por los fixtures sintéticos apunta al propio proceso y no se usa para colocar ventanas. El éxito del modelo no demuestra que DaVinci Resolve o ChatGPT respondan igual en el otro ordenador.

RC 1 ya tiene compilación universal y pruebas nativas locales con ventanas temporales, registradas en [VALIDATION.md](VALIDATION.md). Sigue pendiente comprobar físicamente en el equipo del informe: Principal modificada con Resolve/ChatGPT y tres filas modificadas en pantalla vertical, tanto al trabajar como al cambiar el Space de la otra pantalla. Tampoco se ha probado hardware Intel, desconexiones físicas ni todos los comportamientos de todas las apps. La matriz cubre clases de variaciones y secuencias reproducibles; no constituye una garantía de ausencia de bugs ni una enumeración infinita de casos.

El catálogo estable conserva 0.7.0/build 43. Esta batería no publica la definitiva.
