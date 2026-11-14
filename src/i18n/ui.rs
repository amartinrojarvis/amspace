//! Authored settings, sidebar, menu, tooltip, accessibility and feedback strings.
pub(super) fn english(text: &str) -> Option<&'static str> {
    Some(match text {
        // Sidebar and page titles.
        "Accesibilidad…" => "Accessibility…",
        "Navegación y traslado de ventanas" => "Navigation and window transfer",
        "Activar atajos de navegación y traslado" => "Enable navigation and transfer shortcuts",
        "Dar prioridad a los atajos de AMSpace" => "Prioritize AMSpace shortcuts",
        "Enviar a pantalla derecha" => "Send to right display",
        "Ventanas" => "Windows",
        "Barra de menús" => "Menu bar",
        "Mosaico" => "Tiling",
        "Atajos" => "Shortcuts",
        "General" => "General",
        "Acerca de" => "About",
        "Salir" => "Quit",
        "Secciones de ajustes" => "Settings sections",
        "Aplicación" => "Application",
        "AMSpace · abrir ajustes" => "AMSpace · open settings",
        "Ajustes de AMSpace" => "AMSpace Settings",
        "Ajustes…" => "Settings…",
        // Status item and desktop menu.
        "Elegir escritorio" => "Choose a desktop",
        ", activo" => ", active",
        // Window page.
        "Borde de la ventana activa" => "Active window border",
        "Color" => "Color",
        "Color del borde" => "Border color",
        "Índigo" => "Indigo",
        "Menta" => "Mint",
        "Ámbar" => "Amber",
        "Rosa" => "Rose",
        "Grosor" => "Thickness",
        "Grosor del borde en puntos" => "Border thickness in points",
        "Enfocar al pasar el cursor" => "Focus on hover",
        "Traer al primer plano" => "Bring to front",
        "Mantén ⌥ para pausar el foco automático." => "Hold ⌥ to pause automatic focus.",
        // Shortcuts page.
        "Los atajos cambian los escritorios de la pantalla principal." => {
            "Shortcuts switch the desktops of the main display."
        }
        "macOS, otras herramientas de teclado y la entrada segura pueden limitarla." => {
            "macOS, other keyboard tools and secure input may limit it."
        }
        "Sugerencias…" => "Suggestions…",
        "Cambiar a escritorio" => "Switch to desktop",
        "Enviar ventana a escritorio" => "Send window to desktop",
        "Enviar a pantalla izquierda" => "Send to left display",
        "Ventana flotante / mosaico" => "Floating window / tiling",
        "Selector de plantillas" => "Layout picker",
        "Pestaña anterior" => "Previous tab",
        "Pestaña siguiente" => "Next tab",
        "Separar pestaña activa" => "Detach active tab",
        "Activar atajos de pestañas (requiere Mosaico)" => {
            "Enable tab shortcuts (requires tiling)"
        }
        "Al enviar a otro escritorio, permanecer en el origen" => {
            "Stay on the source when sending to another desktop"
        }
        "Los escritorios se numeran por pantalla. Flotante y selector requieren Mosaico activo." => {
            "Desktops are numbered per display. Floating and the picker require active tiling."
        }
        "Prioridad activa" => "Priority active",
        "Prioridad suspendida; vuelve a activar el check para reintentar" => {
            "Priority suspended; re-enable the check to retry"
        }
        "Prioridad no disponible; se usan reservas normales" => {
            "Priority unavailable; normal reservations are used"
        }
        "Prioridad desactivada" => "Priority disabled",
        " · navegación desactivada" => " · navigation disabled",
        "\nEl envío entre escritorios no está disponible en este macOS." => {
            "\nDesktop sending is unavailable on this macOS."
        }
        "Ese escritorio no existe en la pantalla principal." => {
            "That desktop does not exist on the main display."
        }
        "Esa combinación coincide con otro atajo de AMSpace. Se conservan los ajustes anteriores." => {
            "That combination matches another AMSpace shortcut. The previous settings are kept."
        }
        // Tiling page.
        "Activar mosaico" => "Enable tiling",
        "Mosaico en este escritorio" => "Tiling on this desktop",
        "Selecciona un escritorio" => "Select a desktop",
        "Configura los atajos en la sección Atajos." => {
            "Configure shortcuts in the Shortcuts section."
        }
        "Animar colocaciones" => "Animate placements",
        "Zona superior: flotante / mosaico" => "Top zone: floating / tiling",
        "Al mover una ventana aparece un recuadro arriba, en el centro de la pantalla. Suelta dentro para hacerla flotante o devolverla al mosaico; salir cancela." => {
            "Moving a window shows a box at the top center of the display. Drop inside to make it floating or return it to tiling; leaving cancels."
        }
        "Colocación inteligente" => "Smart placement",
        "Previsualizar al redimensionar" => "Preview while resizing",
        "Agrupar como pestañas" => "Group as tabs",
        "Máximo de pestañas (2–20)" => "Maximum tabs (2–20)",
        "Máximo de pestañas" => "Maximum tabs",
        "Alineación de pestañas" => "Tab alignment",
        "Izquierda" => "Left",
        "Centro" => "Center",
        "Rejilla y plantillas…" => "Grid and layouts…",
        "Apps flotantes…" => "Floating apps…",
        "El máximo de pestañas admite valores enteros entre 2 y 20." => {
            "The maximum tabs accepts whole numbers between 2 and 20."
        }
        "Espiral" => "Spiral",
        "Cuadrícula" => "Grid",
        "Principal" => "Main",
        "Principal centrada" => "Centered main",
        "Principal vertical" => "Vertical main",
        "3 columnas" => "3 columns",
        "3 filas" => "3 rows",
        "4 columnas" => "4 columns",
        "4 filas" => "4 rows",
        "Centrada" => "Centered",
        // Layout picker settings panel.
        "Selector de distribuciones" => "Layout picker",
        "Plantillas en la pantalla del cursor" => "Layouts on the cursor display",
        "Previsualiza con el ratón o las flechas. Intro aplica; Esc cancela." => {
            "Preview with the mouse or arrow keys. Return applies; Escape cancels."
        }
        "Abrir selector" => "Open picker",
        "Buscar atajos libres" => "Find free shortcuts",
        "Busca sugerencias en este Mac" => "Search suggestions on this Mac",
        "Usar sugerido" => "Use suggested",
        "Se comprueban reservas globales y atajos del sistema.\nOtras apps pueden tener atajos propios no detectables." => {
            "Global reservations and system shortcuts are checked.\nOther apps may have their own undetectable shortcuts."
        }
        "Mostrar plantillas" => "Show layouts",
        "Atajo listo · abre el selector en la pantalla del cursor." => {
            "Shortcut ready · opens the picker on the cursor display."
        }
        "Atajo no disponible: revisa tiling, Accesibilidad o la combinación." => {
            "Shortcut unavailable: check tiling, Accessibility or the combination."
        }
        "No se pudieron consultar los atajos del sistema." => {
            "System shortcuts could not be queried."
        }
        "No se encontraron combinaciones libres. Prueba otra tecla." => {
            "No free combinations found. Try another key."
        }
        "Sugerencias comprobadas en este Mac. Elige una y pulsa Usar sugerido." => {
            "Suggestions verified on this Mac. Pick one and press Use suggested."
        }
        "Reanuda Mosaico antes de cambiar la plantilla." => {
            "Resume tiling before changing the layout."
        }
        "Activa tiling y Accesibilidad para usar las plantillas." => {
            "Enable tiling and Accessibility to use layouts."
        }
        "Activa el mosaico en el escritorio de esta pantalla." => {
            "Enable tiling on this display's desktop."
        }
        "La pantalla o sus ventanas han cambiado. Abre de nuevo el selector." => {
            "The display or its windows changed. Open the picker again."
        }
        // Grid and custom layouts panel.
        "Rejilla y plantillas" => "Grid and layouts",
        "Ajustar tamaños a una rejilla virtual" => "Fit sizes to a virtual grid",
        "Activar rejilla virtual" => "Enable virtual grid",
        "Las divisiones se alinean sin dejar huecos entre ventanas." => {
            "Divisions align without gaps between windows."
        }
        "Columnas (2–240)" => "Columns (2–240)",
        "Filas (2–60)" => "Rows (2–60)",
        "Aplicar rejilla" => "Apply grid",
        "Plantillas personalizadas" => "Custom layouts",
        "Nombre de la plantilla" => "Layout name",
        "Guardar actual" => "Save current",
        "Plantillas guardadas" => "Saved layouts",
        "Eliminar" => "Delete",
        "Aplicar plantilla" => "Apply layout",
        "Copia rápida del escritorio actual" => "Quick copy of the current desktop",
        "Guardar copia rápida" => "Save quick copy",
        "Restaurar copia rápida" => "Restore quick copy",
        "Se guardan divisiones y proporciones; no se abren aplicaciones." => {
            "Divisions and proportions are saved; no apps are opened."
        }
        "Ninguna plantilla guardada" => "No saved layout",
        "ventana" => "window",
        "ventanas" => "windows",
        "Guarda el mosaico actual.\nReutilízalo con otras ventanas." => {
            "Save the current tiling.\nReuse it with other windows."
        }
        "Espera a que termine la operación actual antes de cambiar una plantilla." => {
            "Wait for the current operation to finish before changing a layout."
        }
        "Espera a que termine el traslado antes de cambiar Mosaico." => {
            "Wait for the transfer to finish before changing tiling."
        }
        "Espera a que termine el traslado o cambio de escritorio antes de pausar." => {
            "Wait for the transfer or desktop switch to finish before pausing."
        }
        "Espera a que termine el cambio de escritorio o el traslado anterior." => {
            "Wait for the desktop change or previous transfer to finish."
        }
        "Plantilla eliminada. El mosaico actual se conserva." => {
            "Layout deleted. The current tiling is kept."
        }
        "Plantilla guardada. Puedes aplicarla en otros escritorios con el mismo número de ventanas." => {
            "Layout saved. You can apply it on other desktops with the same number of windows."
        }
        "Plantilla aplicada conservando sus divisiones y proporciones." => {
            "Layout applied keeping its divisions and proportions."
        }
        "Las ventanas o la pantalla han cambiado, o sus tamaños mínimos no caben en esta distribución." => {
            "The windows or the display changed, or their minimum sizes do not fit this distribution."
        }
        "Introduce columnas entre 2 y 240 y filas entre 2 y 60." => {
            "Enter columns between 2 and 240 and rows between 2 and 60."
        }
        "Rejilla actualizada. Activa la casilla para aplicarla." => {
            "Grid updated. Enable the checkbox to apply it."
        }
        "Distribución guardada para este escritorio." => "Distribution saved for this desktop.",
        "Activa tiling y abre ventanas en este escritorio primero." => {
            "Enable tiling and open windows on this desktop first."
        }
        "Distribución restaurada." => "Distribution restored.",
        "No hay una distribución compatible con estas ventanas y pantalla." => {
            "No distribution matches these windows and display."
        }
        "Reanuda Mosaico antes de trasladar una ventana." => {
            "Resume tiling before moving a window."
        }
        "Preparando traslado de ventana…" => "Preparing window transfer…",
        "Ventana enviada al destino." => "Window sent to the destination.",
        "La ventana cambió de ubicación antes de confirmar el traslado." => {
            "The window moved before the transfer was confirmed."
        }
        // Floating apps panel.
        "Aplicaciones flotantes" => "Floating apps",
        "Abrir como flotantes" => "Open as floating",
        "Se aplica a las nuevas ventanas de estas aplicaciones." => {
            "It applies to new windows of these apps."
        }
        "Puedes devolver cada ventana al tile con tu atajo." => {
            "You can return each window to the tile with your shortcut."
        }
        "Añadir aplicación…" => "Add app…",
        "Cerrar" => "Close",
        "Ninguna aplicación: las ventanas usarán mosaico." => "No apps: windows will use tiling.",
        "Quitar" => "Remove",
        "Selecciona una aplicación" => "Select an app",
        "La aplicación seleccionada no tiene identificador de bundle." => {
            "The selected app has no bundle identifier."
        }
        // General page.
        "Abrir al iniciar sesión" => "Open at login",
        "Actualizaciones" => "Updates",
        "Buscar ahora" => "Check now",
        "Buscar automáticamente" => "Check automatically",
        "Buscar actualizaciones automáticamente" => "Check for updates automatically",
        "Historial de versiones" => "Version history",
        "Copiar diagnóstico" => "Copy diagnostics",
        "Instalación y ayuda…" => "Installation and help…",
        "Enviar feedback…" => "Send feedback…",
        "Idioma" => "Language",
        "Sistema" => "System",
        "Vuelve a abrir AMSpace para aplicar el idioma." => "Reopen AMSpace to apply the language.",
        "El idioma del sistema elige español o inglés. El cambio se aplica al volver a abrir AMSpace." => {
            "System language chooses Spanish or English. Changes apply when you reopen AMSpace."
        }
        "Creado por Alberto Martín" => "Created by Alberto Martín",
        "Tu espacio. Tu foco." => "Your space. Your focus.",
        "© 2026 Alberto Martín · Licencia MIT" => "© 2026 Alberto Martín · MIT License",
        // Menu bar widget page.
        "Fondo del widget" => "Widget background",
        "Seleccionado" => "Selected",
        "Escritorios" => "Desktops",
        "Puntos" => "Dots",
        "Números" => "Numbers",
        "Iconos" => "Icons",
        "Preview al pasar por un escritorio" => "Preview on desktop hover",
        "Borde al reelegir el escritorio" => "Border when reselecting the desktop",
        "No hay escritorios disponibles en esta pantalla." => {
            "No desktops are available on this display."
        }
        "Actualizaciones disponibles en el paquete instalado." => {
            "Updates are available in the installed package."
        }
        // Permission, pause, tips and status feedback.
        "● Accesibilidad concedida" => "● Accessibility granted",
        "○ Borde, foco y tiling requieren Accesibilidad" => {
            "○ Border, focus and tiling require Accessibility"
        }
        "Concede el permiso y vuelve a esta ventana para activarlo." => {
            "Grant the permission and return to this window to enable it."
        }
        "Concede Accesibilidad a AMSpace para gestionar las ventanas." => {
            "Grant Accessibility to AMSpace to manage windows."
        }
        "Autoriza AMSpace en Ajustes del Sistema → Ítems de inicio." => {
            "Allow AMSpace in System Settings → Login Items."
        }
        "No se pudo cambiar. Instala AMSpace.app en Aplicaciones." => {
            "Could not change it. Install AMSpace.app in Applications."
        }
        "Diagnóstico copiado. Incluye estado y geometría, sin títulos ni contenido de ventanas." => {
            "Diagnostics copied. Includes state and geometry, without window titles or content."
        }
        "No se pudo copiar el diagnóstico." => "The diagnostics could not be copied.",
        "Pausar Mosaico" => "Pause tiling",
        "Reanudar Mosaico" => "Resume tiling",
        "Mosaico está en pausa. Reanúdalo en Ajustes." => {
            "Tiling is paused. Resume it in Settings."
        }
        "Mosaico está en pausa. Pulsa Reanudar Mosaico antes de cambiar la distribución." => {
            "Tiling is paused. Press Resume tiling before changing the layout."
        }
        "Mosaico está desactivado. Puedes activarlo en Ajustes → Mosaico." => {
            "Tiling is disabled. You can enable it in Settings → Tiling."
        }
        "Mosaico en pausa. Conserva la distribución y deja de colocar ventanas. Reanúdalo en la barra lateral de Ajustes." => {
            "Tiling paused. The layout is kept and windows stop being placed. Resume it in the Settings sidebar."
        }
        "Mosaico reanudado." => "Tiling resumed.",
        "Activa Mosaico en Ajustes para gestionar las pestañas." => {
            "Enable tiling in Settings to manage tabs."
        }
        "Espera a que termine la transición o cierra el selector de plantillas." => {
            "Wait for the transition to finish or close the layout picker."
        }
        "Mostrar escritorio: el mosaico está en pausa." => "Show desktop: tiling is paused.",
        "La plantilla no admite estas ventanas o el escritorio ha cambiado." => {
            "The layout does not fit these windows or the desktop changed."
        }
        "Agrupa soltando una ventana en la parte superior de otra. Arrastra una pestaña para separarla; clic derecho para ver sus acciones." => {
            "Group by dropping a window on top of another. Drag a tab to detach it; right-click to see its actions."
        }
        "Arrastra una pestaña para separarla. Clic derecho o Control-clic abre las acciones; × solicita el cierre normal de la ventana." => {
            "Drag a tab to detach it. Right-click or Control-click opens the actions; × requests the normal window close."
        }
        // Tab context menu and main menu.
        "Separar del grupo" => "Detach from group",
        "Hacer flotante" => "Make floating",
        "Mover a otro grupo" => "Move to another group",
        "No hay otros grupos en este escritorio y pantalla" => {
            "There are no other groups on this desktop and display"
        }
        "Cerrar ventana" => "Close window",
        "Ayuda de pestañas…" => "Tabs help…",
        "Ir al escritorio…" => "Go to desktop…",
        "Acerca de AMSpace" => "About AMSpace",
        "Buscar actualizaciones…" => "Check for updates…",
        "Pausar / reanudar Mosaico" => "Pause / resume tiling",
        "Salir de AMSpace" => "Quit AMSpace",
        _ => return None,
    })
}
