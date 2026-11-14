//! Shared status, icon and palette labels; source order and persisted indices stay unchanged.
pub(super) fn english(text: &str) -> Option<&'static str> {
    Some(match text {
        "Escritorios no disponibles en esta versión de macOS" => {
            "Desktops are unavailable on this version of macOS"
        }
        "escritorio" => "desktop",
        "escritorios" => "desktops",
        "Ese escritorio ya no está disponible." => "That desktop is no longer available.",
        "Espera a que termine el cambio de pantalla." => "Wait for the display change to finish.",
        "Concede Accesibilidad para cambiar de escritorio." => {
            "Grant Accessibility permission to switch desktops."
        }
        "El escritorio seleccionado ya no está disponible." => {
            "The selected desktop is no longer available."
        }
        "Ha cambiado la pantalla del escritorio." => "The desktop's display has changed.",
        "macOS no confirmó el cambio de escritorio." => {
            "macOS did not confirm the desktop switch."
        }
        "Cambio cancelado: se ha seleccionado otro escritorio." => {
            "Switch cancelled: another desktop was selected."
        }
        "No se pudo localizar el escritorio actual." => "The current desktop could not be located.",
        "No se pudo localizar el siguiente escritorio." => "The next desktop could not be located.",
        "Ha cambiado el orden de los escritorios." => "The desktop order has changed.",
        "Esta versión de macOS no admite el cambio directo." => {
            "This version of macOS does not support direct switching."
        }
        "Número" => "Number",
        "Trabajo" => "Work",
        "Código" => "Code",
        "Web" => "Web",
        "Mensajes" => "Messages",
        "Música" => "Music",
        "Diseño" => "Design",
        "Casa" => "Home",
        "Juegos" => "Games",
        "Lectura" => "Reading",
        "Estrella" => "Star",
        "Terminal" => "Terminal",
        "Correo" => "Mail",
        "Edición de vídeo" => "Video editing",
        "Edición de foto" => "Photo editing",
        "Fotografía" => "Photography",
        "Comunicaciones" => "Communications",
        "Videollamadas" => "Video calls",
        "Equipo" => "Team",
        "Audio" => "Audio",
        "Calendario" => "Calendar",
        "Tareas" => "Tasks",
        "Escritura" => "Writing",
        "Documentos" => "Documents",
        "Archivos" => "Files",
        "Nube" => "Cloud",
        "Analítica" => "Analytics",
        "Creatividad" => "Creativity",
        "Ilustración" => "Illustration",
        "Herramientas" => "Tools",
        "Redes" => "Networking",
        "Sin color" => "No color",
        "Azul" => "Blue",
        "Salvia" => "Sage",
        "Lavanda" => "Lavender",
        "Arena" => "Sand",
        "Rosa" => "Rose",
        "Gris" => "Gray",
        _ => return None,
    })
}
