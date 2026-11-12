# Versiones y actualizaciones

AMSpace usa [Sparkle 2.10.1](https://sparkle-project.org/documentation/) desde Rust con su interfaz nativa. El repositorio y las descargas son públicos. No se necesita token de GitHub en la app.

## Usuario

Ajustes → General → Buscar ahora → revisar novedades → Instalar actualización. Sparkle gestiona descarga, verificación, sustitución y reinicio. Los errores de red y la ausencia de novedades se muestran en diálogos nativos. Las preferencias permanecen en NSUserDefaults.

La búsqueda diaria es opcional. Nunca se instala automáticamente. El feed y el ZIP exigen firma Ed25519, además de HTTPS. La clave privada se guarda en el Llavero, cuenta `io.github.amartinrojarvis.amspace`; solo la clave pública está en Info.plist. No se exporta ni se sube en CI.

## Publicar una versión

1. Incrementar `version` en Cargo.toml y `CFBundleShortVersionString` en assets/Info.plist. Incrementar también `CFBundleVersion`, entero estrictamente creciente. Actualizar Cargo.lock y docs/RELEASE_NOTES.md.
2. Ejecutar `./scripts/check.sh` y `./scripts/bundle.sh --universal`.
3. Ejecutar `./scripts/prepare-release.py` en el Mac que custodia la clave. Genera `dist/AMSpace-universal.zip`, `dist/appcast.xml` firmado y `dist/SHA256SUMS.txt`; verifica ambas firmas y coherencia del paquete.
4. Crear el commit, subirlo y esperar a CI. Crear una release de GitHub con tag `vX.Y.Z`, ese commit y los tres archivos. Comprobar que el ZIP puede descargarse sin autenticación.
5. Publicar una copia idéntica del ZIP firmado en la rama `updates`, ruta `X.Y.Z/AMSpace-universal.zip`, y verificar su descarga pública desde `https://raw.githubusercontent.com/amartinrojarvis/amspace/updates/X.Y.Z/AMSpace-universal.zip`. Mantener esas rutas inmutables. El catálogo utiliza esa URL para evitar depender del host de descargas de releases. Para reparar solo la URL del catálogo de un ZIP existente, usar `prepare-release.py --feed-only`; comprueba versión y ejecutable antes de firmar, sin reemplazar el ZIP.
6. Solo después, copiar `dist/appcast.xml` a `appcast.xml` en la raíz, hacer commit y push. **No editar manualmente el feed firmado.** Verificar la URL exacta de `SUFeedURL` tanto sin compresión como con las cabeceras de Sparkle; comparar los cuerpos descargados con el catálogo firmado local. GitHub usa `Vary: Authorization,Accept-Encoding`: una respuesta sin compresión actualizada no prueba que la variante comprimida ya anuncie la versión. No añadir un parámetro de consulta para esta comprobación: crearía otra clave de caché.
7. Probar Buscar… desde la versión anterior, instalar y verificar versión y preferencias después del reinicio.

Comprobación de la variante de Sparkle (ajustar el User-Agent a la versión instalada):

```sh
curl --fail --silent --show-error --compressed \
  -H 'User-Agent: AMSpace/0.9.6 Sparkle/2.10.1' \
  -H 'Accept: application/rss+xml,*/*;q=0.1' \
  -H 'Accept-Language: es-ES,es;q=0.9' \
  -H 'Accept-Encoding: gzip, deflate, br' \
  "$(/usr/libexec/PlistBuddy -c 'Print SUFeedURL' assets/Info.plist)" \
  -o .cache/appcast-sparkle-public.xml
cmp dist/appcast.xml .cache/appcast-sparkle-public.xml
```

Si la variante aún anuncia la anterior, conservar las cabeceras `Date`, `Expires`, `Source-Age` y el cuerpo para diagnosticar la propagación. No reemplazar el ZIP publicado, desactivar firmas ni borrar preferencias para resolver una caché HTTP transitoria.

La clave compatible con `generate_keys --account io.github.amartinrojarvis.amspace` debe estar en el Llavero con servicio `https://sparkle-project.org` y protocolo SSH, que forman parte de la consulta de Sparkle. `prepare-release.py` usa únicamente `--account` y comprueba con `generate_keys -p` que la clave pública coincide con `SUPublicEDKey` antes de firmar. No crea ni rota claves. No usar archivos de respaldo como fallback ni extraer el secreto con `security -w`. Mantener una copia de seguridad protegida del Llavero; perder esa clave impide firmar actualizaciones compatibles. No incluir claves privadas en el repositorio, argumentos de proceso ni artefactos.

## Firma de Apple

Ed25519 autentica el canal de actualización; no sustituye Developer ID ni notarización. Los paquetes actuales están firmados ad hoc: Gatekeeper puede intervenir en la primera descarga y TCC puede requerir eliminar y volver a añadir AMSpace en Accesibilidad tras sustituir el ejecutable. La app no elude estas decisiones. Para una distribución sin esa renovación recurrente se necesita una identidad Developer ID estable y un proceso de notarización.

## Desarrollo

`fetch-sparkle.sh` fija versión y SHA-256 de la distribución oficial y la guarda en .cache (ignorada por Git). `bundle.sh` integra el framework y sus auxiliares, preserva symlinks, incluye licencias y verifica la firma del bundle. CI empaqueta pero no publica ni dispone de la clave privada.

## Candidatas para pruebas físicas

Cuando el arreglo necesite hardware que no está disponible localmente, publicar una prerelease claramente marcada y mantener el catálogo estable intacto. La candidata 0.8.0 usa build 44 y tag `v0.8.0-rc.1`; su comprobación local no se debe presentar como validación física de varias pantallas.

1. Ejecutar comprobaciones, bundle universal y CI sobre el commit exacto.
2. `python3 scripts/prepare-release.py` verifica y firma el ZIP. Para una candidata manual, no publicar ni copiar el `dist/appcast.xml` generado: su URL apunta a la futura versión estable.
3. Extraer la firma del ZIP de su enclosure a `AMSpace-universal.zip.sig` y generar `SHA256SUMS.txt` para los archivos que se vayan a publicar (ZIP y firma separada). La firma es pública; la clave privada permanece en el Llavero.
4. Crear GitHub Release con `--prerelease`, sin marcarla latest. Conservar una copia idéntica e inmutable del ZIP en `updates/<versión>-rc.<n>/` y comparar su descarga HTTPS sin autenticación con el ZIP firmado.
5. No modificar `appcast.xml` de la raíz. Después de confirmar el hardware, preparar la estable con build estrictamente mayor que la candidata, incluso si conserva `CFBundleShortVersionString` (por ejemplo, 0.8.0 build 45 o superior). Seguir entonces el procedimiento estable completo.
