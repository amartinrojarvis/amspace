pub(super) fn english(text: &str) -> Option<&'static str> {
    Some(match text {
        "AMSpace · Instalación y ayuda" => "AMSpace · Installation and help",
        "Instalación y permisos" => "Installation and permissions",
        "Pestañas y gestos" => "Tabs and gestures",
        "Privacidad y seguridad" => "Privacy and security",
        "1. Copia AMSpace a Aplicaciones" => "1. Copy AMSpace to Applications",
        "En el DMG, arrastra AMSpace a Aplicaciones. Expulsa el disco y abre la copia instalada. La app vive en la barra de menús: clic derecho en su indicador para abrir Ajustes." => {
            "In the DMG, drag AMSpace to Applications. Eject the disk and open the installed copy. The app lives in the menu bar: right-click its indicator to open Settings."
        }
        "2. Si macOS bloquea la primera apertura" => "2. If macOS blocks the first launch",
        "La distribución actual tiene firma ad hoc, sin Developer ID ni notarización. Solo si confías en la descarga oficial: Ajustes del Sistema → Privacidad y seguridad → Abrir igualmente. No ignores una advertencia de malware ni desactives Gatekeeper." => {
            "The current distribution is ad hoc signed, without Developer ID or notarization. Only if you trust the official download: System Settings → Privacy & Security → Open Anyway. Never ignore a malware warning or disable Gatekeeper."
        }
        "3. Autoriza Accesibilidad" => "3. Grant Accessibility permission",
        "Ajustes del Sistema → Privacidad y seguridad → Accesibilidad. Activa AMSpace; si no aparece, usa + y elige Aplicaciones/AMSpace.app. Vuelve aquí y revisa el permiso. Después activa las funciones que quieras en Ajustes." => {
            "System Settings → Privacy & Security → Accessibility. Enable AMSpace; if it is missing, use + and select Applications/AMSpace.app. Return here and check the permission. Then enable the features you want in Settings."
        }
        "Agrupar, seleccionar y cerrar" => "Group, select and close",
        "Con Mosaico y agrupación activos, arrastra una ventana sobre la zona superior de otra. Pulsa una pestaña para seleccionarla; × solicita el cierre normal de la ventana. Los avisos de guardar siguen perteneciendo a cada aplicación." => {
            "With Tiling and grouping enabled, drag a window onto another window's upper zone. Click a tab to select it; × requests the window's normal close. Each application still handles its own save prompts."
        }
        "Separar y reorganizar" => "Detach and reorder",
        "Arrastra dentro de la barra para reordenar; fuera de ella para separar la ventana. La marca indica dónde quedará. Clic derecho o Control-clic: Separar, Hacer flotante, Mover a otro grupo y Cerrar. Si no cabe, la distribución no cambia." => {
            "Drag within the strip to reorder; outside it to detach the window. The marker shows its destination. Right-click or Control-click: Detach, Make floating, Move to another group and Close. If it does not fit, the layout stays unchanged."
        }
        "Atajos y pausa a mano" => "Shortcuts and pause at hand",
        "En Ajustes → Atajos puedes activar y configurar pestaña anterior, siguiente y separar la activa. Están desactivados inicialmente para no reservar teclas. Pausar Mosaico detiene sus colocaciones sin deshacer la distribución; reanúdalo en la barra lateral de Ajustes." => {
            "In Settings → Shortcuts, enable and configure previous tab, next tab and detach active tab. They are initially off to avoid reserving keys. Pause Tiling stops placements without undoing the layout; resume it in the Settings sidebar."
        }
        "Qué permite Accesibilidad" => "What Accessibility allows",
        "Es un permiso sensible: permite consultar y manipular ventanas de otras apps, incluidos títulos, posición y tamaño. Concédelo solo si confías en AMSpace. Puedes revocarlo en cualquier momento desde Ajustes del Sistema." => {
            "This is a sensitive permission: it allows reading and manipulating other apps' windows, including titles, position and size. Grant it only if you trust AMSpace. You can revoke it at any time in System Settings."
        }
        "Qué permanece en tu Mac" => "What stays on your Mac",
        "AMSpace no necesita Grabación de pantalla ni incluye telemetría. Las preferencias se guardan localmente. Las comprobaciones de actualización de Sparkle consultan el catálogo y descargan paquetes publicados en GitHub; puedes desactivar la búsqueda automática en Ajustes → General." => {
            "AMSpace does not need Screen Recording and includes no telemetry. Preferences stay local. Sparkle update checks fetch the feed and download packages published on GitHub; disable automatic checking in Settings → General."
        }
        "Qué garantiza la distribución actual" => "What the current distribution guarantees",
        "La firma Ed25519 del catálogo de actualizaciones no equivale a una firma Developer ID ni a la notarización de Apple. Una actualización ad hoc puede requerir conceder Accesibilidad de nuevo. El código fuente y las licencias están disponibles para revisión." => {
            "The update feed's Ed25519 signature is not a Developer ID signature or Apple notarization. An ad hoc update may require granting Accessibility again. The source code and licenses are available for review."
        }
        "Abrir Accesibilidad…" => "Open Accessibility…",
        "Revisar permiso" => "Check permission",
        "Privacidad y seguridad…" => "Privacy and security…",
        "Mostrar ayudas en los primeros usos" => "Show help on first uses",
        "Entendido" => "Got it",
        "Accesibilidad concedida. Puedes activar Borde, Foco y Mosaico." => {
            "Accessibility granted. You can enable Border, Focus and Tiling."
        }
        "Accesibilidad pendiente. Concede el permiso a la copia instalada." => {
            "Accessibility pending. Grant permission to the installed copy."
        }
        "Tu experiencia ayuda a mejorar AMSpace" => "Your experience helps improve AMSpace",
        "Cuéntame un error, una sugerencia o cómo utilizas la app en tu equipo." => {
            "Tell me about a bug, a suggestion or how you use the app on your Mac."
        }
        "Preparar correo abre un borrador en tu aplicación de correo. Tú decides qué incluir y cuándo enviarlo. No se adjuntan diagnósticos ni datos de tus ventanas." => {
            "Compose email opens a draft in your email app. You decide what to include and when to send it. No diagnostics or window data are attached."
        }
        "Preparar correo" => "Compose email",
        "Copiar dirección" => "Copy address",
        "Si no tienes un cliente de correo configurado, utiliza esta dirección desde tu webmail." => {
            "If no email client is configured, use this address from your webmail."
        }
        "Hola Alberto:\r\n\r\nMi comentario o sugerencia:\r\n\r\nSi se trata de un error:\r\n- Pasos para reproducirlo:\r\n- Qué esperaba y qué ocurrió:\r\n- Versión de macOS y configuración de pantallas (opcional):\r\n\r\nRevisaré cualquier diagnóstico antes de adjuntarlo; no incluiré datos privados.\r\n" => {
            "Hi Alberto:\r\n\r\nMy feedback or suggestion:\r\n\r\nIf this is a bug:\r\n- Steps to reproduce:\r\n- What I expected and what happened:\r\n- macOS version and display setup (optional):\r\n\r\nI will review any diagnostic before attaching it and will not include private data.\r\n"
        }
        "Se ha solicitado abrir el borrador, no enviar el mensaje. Si tu cliente no está configurado, escribe a la dirección desde tu webmail." => {
            "Opening a draft was requested, not sending a message. If your client is not configured, use the address from your webmail."
        }
        "No se pudo abrir el correo. Copia la dirección y escribe desde tu cliente o webmail." => {
            "Email could not be opened. Copy the address and write from your client or webmail."
        }
        "Dirección copiada. No se ha enviado ningún mensaje." => {
            "Address copied. No message was sent."
        }
        "No se pudo copiar. Selecciona la dirección de arriba y cópiala manualmente." => {
            "Copying failed. Select the address above and copy it manually."
        }
        _ => return None,
    })
}
