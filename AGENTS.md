# Guía para agentes de desarrollo

## Proyecto y lectura inicial

AMSpace es una aplicación nativa de barra de menús para macOS: indicador y navegación de Spaces, previews de distribución, borde de ventana activa, foco bajo el cursor, mosaico opcional, atajos globales y traslado de ventanas entre escritorios/pantallas. Está escrita en Rust con AppKit; no hay frontend web. Interfaz y documentación de producto en español.

Antes de modificar una zona, lee los archivos correspondientes:

| Necesidad | Referencia |
|---|---|
| Producto, instalación y comportamiento esperado | [README.md](README.md) |
| Entorno, mapa del código, diagnóstico y pruebas | [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) |
| Contratos nativos y regresiones que no deben volver | [docs/NATIVE_INVARIANTS.md](docs/NATIVE_INVARIANTS.md) |
| Detalle de implementación | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Evidencia de pruebas, historial y límites físicos | [docs/VALIDATION.md](docs/VALIDATION.md) |
| Matrices y sesiones reproducibles antes de 0.8 | [docs/ROBUSTNESS.md](docs/ROBUSTNESS.md) |
| Firma, archivos inmutables y publicación | [docs/UPDATES.md](docs/UPDATES.md) |

La versión de código está en `Cargo.toml`/`Cargo.lock`; la del bundle y su build en `assets/Info.plist`. `appcast.xml` describe la actualización publicada, no necesariamente el trabajo en curso. No deduzcas el estado actual de una sección histórica de validación ni de archivos locales en `.cache/`.

## Cómo trabajar

- Comprueba `git status --short` antes de editar; conserva el trabajo ajeno. Separa cambios del modelo de cambios del puente nativo cuando sea posible.
- Reproduce y localiza el fallo antes de cambiar varias rutas a la vez. Usa PID, ID de ventana y pertenencia nativa al Space; títulos o números visibles no son identidades.
- Mantén el cálculo de layouts comprobable en `tiling.rs`. `tile_manager.rs` coordina pertenencia, gestos y colocaciones; `ui.rs` conecta acciones y ciclo de vida.
- Conserva los valores persistidos de plantillas, teclas, modificadores y preferencias históricas. Un índice de selección de la UI no equivale al identificador persistido.
- Las operaciones AppKit y AX pertenecen al hilo principal. El tap prioritario tiene un hilo/run loop propio: su callback solo filtra combinaciones y encola acciones, sin AX ni trabajo de ventanas.
- Las funciones privadas de macOS son opcionales. Comprueba disponibilidad, tipos y resultados, y conserva una salida segura si no están. No sustituyas confirmaciones nativas por un indicador optimista.
- No añadas sondeo permanente para arreglar una transición. Reutiliza observadores, invalidaciones y operaciones temporales con plazo y cancelación.
- Evita activar, elevar o cambiar de escritorio incidentalmente al mostrar una preview o colocar un mosaico oculto. Foco y elevación son funciones independientes.
- Preserva la configuración del usuario en pruebas. Los diagnósticos que crean ventanas usan fixtures desechables; no uses documentos ajenos como fixtures ni reinicies sus preferencias o permisos para obtener un resultado favorable.
- `.cache/`, `dist/` y `target/` están ignorados. No bases funciones o instrucciones reproducibles en probes locales que no están versionados.

## Comprobaciones

Para cambios de código:

```sh
./scripts/check.sh
```

Incluye formato, Clippy sin warnings, pruebas y validación del plist. Añade regresiones que comprueben comportamiento, aislamiento y cancelación cuando cambies geometría, pertenencia o traslados. Para empaquetado o puentes nativos, comprueba también:

```sh
./scripts/bundle.sh --universal
```

Consulta los efectos de cada diagnóstico en [DEVELOPMENT.md](docs/DEVELOPMENT.md) antes de ejecutarlo. No ejecutes indiscriminadamente todas las pruebas ignoradas: varias necesitan escritorio interactivo o condiciones concretas.

Para cambios solo documentales basta comprobar enlaces locales, rutas, comandos contra el código y `git diff --check`; no hace falta relanzar la app ni modificar el bundle.

Al informar resultados, distingue pruebas del modelo, compilación por arquitectura y comprobación física. Compilar Intel o simular varios monitores no verifica ese hardware. Registra los casos realmente ejecutados y sus límites en `docs/VALIDATION.md` cuando cambie el comportamiento.

## Publicación y entrega

Una edición documental se publica como commit en el repositorio; no necesita incrementar la versión de la app ni regenerar su catálogo. Cuando el encargo incluya una versión de la aplicación, sigue [UPDATES.md](docs/UPDATES.md): código revisado y CI, ZIP y catálogo firmados, descarga pública verificada y, al final, anuncio del catálogo.

Los ZIP publicados en `updates/<versión>/` son inmutables. No edites manualmente `appcast.xml`, no reemplaces archivos de una versión ya publicada y no exportes la clave privada del Llavero. El bundle tiene firma ad hoc; Ed25519 de Sparkle no implica Developer ID ni notarización. Incluye las licencias de terceros al empaquetar.

En la entrega indica qué cambió, por qué, cómo se verificó y qué queda sin comprobar. No afirmes prioridad absoluta del teclado, selección instantánea de un Space lejano o compatibilidad física que no se haya demostrado.
