pub(super) fn english(text: &str) -> Option<&'static str> {
    Some(match text {
        "Espiral" => "Spiral",
        "Se ha revocado Accesibilidad; el traslado queda cancelado." => {
            "Accessibility permission was revoked; the move was cancelled."
        }
        "Cuadrícula" => "Grid",
        "Principal" => "Main",
        "División en 3" => "Split in 3",
        "División en 4" => "Split in 4",
        "Principal centrada" => "Centered main",
        "Centrada XL · 3 por lado" => "Centered XL · 3 per side",
        "Principal vertical" => "Vertical main",
        "Centrada" => "Centered",
        "Centrada XL" => "Centered XL",
        "3 columnas" => "3 columns",
        "3 filas" => "3 rows",
        "4 columnas" => "4 columns",
        "4 filas" => "4 rows",
        "Espacio" => "Space",
        "Retorno" => "Return",
        "vertical" => "portrait",
        "horizontal" => "landscape",
        "hueco" => "slot",
        "huecos" => "slots",
        "Distribuciones de esta pantalla" => "Layouts for this display",
        "Cancelar selección" => "Cancel selection",
        "Ratón o teclas · Flechas · Intro: aplicar · Esc: cancelar" => {
            "Mouse or keyboard · Arrows · Return: apply · Esc: cancel"
        }
        "Esta distribución no cabe. Elige otra plantilla." => {
            "This layout does not fit. Choose another layout."
        }
        "Volver al mosaico" => "Return to tiling",
        "Hacer flotante" => "Make floating",
        "Suelta aquí para confirmar" => "Drop here to confirm",
        "Apilar como pestaña" => "Stack as a tab",
        "Pestañas" => "Tabs",
        "Intercambiar posiciones" => "Swap positions",
        "Intercambiar" => "Swap",
        "Posición" => "Position",
        "Colocar aquí" => "Place here",
        "No disponible" => "Unavailable",
        "No cabe" => "Does not fit",
        "La aplicación no ha aceptado cerrar esta ventana." => {
            "The application did not accept closing this window."
        }
        "La pestaña cambió; vuelve a abrir el menú." => "The tab changed; reopen the menu.",
        "Concede Accesibilidad a AMSpace para gestionar las pestañas." => {
            "Grant AMSpace Accessibility permission to manage tabs."
        }
        "Hay una entrada segura activa; termina ese cuadro de diálogo." => {
            "Secure input is active; finish that dialog first."
        }
        "Mosaico está en pausa o el Escritorio está activo." => {
            "Tiling is paused or Show Desktop is active."
        }
        "Espera a que termine el cambio de escritorio." => "Wait for the desktop switch to finish.",
        "Termina el arrastre en curso antes de usar el menú de pestañas." => {
            "Finish dragging before using the tab menu."
        }
        "Esta ventana ya no está gestionada por Mosaico." => {
            "This window is no longer managed by Tiling."
        }
        "Este escritorio ya no está visible." => "This desktop is no longer visible.",
        "Este escritorio ya no está gestionado." => "This desktop is no longer managed.",
        "Esta ventana ya no pertenece a un grupo de pestañas." => {
            "This window no longer belongs to a tab group."
        }
        "La ventana cambió de escritorio o de área; vuelve a abrir el menú." => {
            "The window changed desktop or area; reopen the menu."
        }
        "La identidad de la ventana cambió; vuelve a abrir el menú." => {
            "The window identity changed; reopen the menu."
        }
        "No se pudo comprobar la lista de ventanas." => "The window list could not be verified.",
        "Las ventanas del escritorio cambiaron; vuelve a abrir el menú." => {
            "The desktop's windows changed; reopen the menu."
        }
        "Un cuadro de diálogo de la aplicación tiene el foco." => {
            "An application dialog has focus."
        }
        "La ventana no está lista para cerrarse; inténtalo de nuevo." => {
            "The window is not ready to close; try again."
        }
        "No se pudo leer la geometría de la ventana." => "The window geometry could not be read.",
        "La geometría de la ventana o pantalla no es válida." => {
            "The window or display geometry is invalid."
        }
        "La pestaña ya pertenece a ese grupo." => "The tab already belongs to that group.",
        "La ventana ya no está gestionada." => "The window is no longer managed.",
        "La identidad de la ventana cambió; vuelve a intentarlo." => {
            "The window identity changed; try again."
        }
        "La aplicación no ha aceptado activar esta ventana." => {
            "The application did not accept activating this window."
        }
        "Esta ventana no forma un grupo de pestañas." => "This window is not in a tab group.",
        "El grupo de pestañas cambió." => "The tab group changed.",
        "Sin sesión de espacio de trabajo." => "No workspace session is available.",
        "Ninguna aplicación está al frente." => "No application is in front.",
        "La aplicación del frente no tiene ventana normal." => {
            "The front application has no standard window."
        }
        "Esa ventana no está gestionada por Mosaico." => "That window is not managed by Tiling.",
        "El identificador de ventana cambió de aplicación." => {
            "The window identifier changed application."
        }
        "La pantalla de origen ya no está disponible." => {
            "The source display is no longer available."
        }
        "La pantalla de destino ya no está disponible." => {
            "The destination display is no longer available."
        }
        "No se puede comprobar el escritorio de destino." => {
            "The destination desktop cannot be verified."
        }
        "La ventana no cabe en la distribución de destino; no se ha movido nada." => {
            "The window does not fit the destination layout; nothing was moved."
        }
        "La ventana ya no está disponible." => "The window is no longer available.",
        "El escritorio ya no está disponible." => "The desktop is no longer available.",
        "No queda espacio para esta ventana en la plantilla; sigue flotante." => {
            "There is no room for this window in the layout; it remains floating."
        }
        "El escritorio de origen ya no está disponible." => {
            "The source desktop is no longer available."
        }
        "La zona de soltado ya no está disponible." => "The drop zone is no longer available.",
        "La ventana o su escritorio cambiaron; se ha cancelado la acción." => {
            "The window or its desktop changed; the action was cancelled."
        }
        "La ventana ya no admite alternar entre flotante y mosaico." => {
            "This window no longer supports switching between floating and tiling."
        }
        "No se pudo confirmar la geometría de la ventana." => {
            "The window geometry could not be confirmed."
        }
        "No se pudo confirmar el escritorio de la ventana." => {
            "The window's desktop could not be confirmed."
        }
        "La ventana todavía no pertenece a la pantalla de destino; acción cancelada." => {
            "The window does not yet belong to the destination display; action cancelled."
        }
        "Una ventana limita su tamaño o posición; prueba otra plantilla o déjala flotante." => {
            "A window limits its size or position; try another layout or make it floating."
        }
        "Activa Mosaico y Accesibilidad para guardar o aplicar plantillas." => {
            "Enable Tiling and Accessibility to save or apply layouts."
        }
        "La pantalla no está disponible." => "The display is unavailable.",
        "Activa el mosaico en el escritorio de esta pantalla." => {
            "Enable tiling on this display's desktop."
        }
        "Abre ventanas en mosaico en esta pantalla primero." => {
            "Open tiled windows on this display first."
        }
        "Esta plantilla no admite los tamaños mínimos de estas ventanas; prueba otra o deja una ventana flotante." => {
            "This layout cannot fit these windows' minimum sizes; choose another or make a window floating."
        }
        "Demasiadas ventanas para esta plantilla; usa Cuadrícula." => {
            "Too many windows for this layout; use Grid."
        }
        "No hay espacio suficiente para separar esta pestaña respetando los tamaños mínimos." => {
            "There is not enough room to detach this tab while respecting minimum sizes."
        }
        "La distribución no permite respetar los tamaños mínimos de estas ventanas." => {
            "The layout cannot fit these windows' minimum sizes."
        }
        "El grupo ya no está en este escritorio." => "The group is no longer on this desktop.",
        "Grupo lleno: alcanza su máximo de pestañas." => "The group has reached its tab limit.",
        "El máximo de pestañas por grupo es demasiado bajo." => {
            "The tab limit per group is too low."
        }
        "Es la propia pestaña." => "This is the same tab.",
        "Escribe un nombre de entre 1 y 60 caracteres." => {
            "Enter a name between 1 and 60 characters."
        }
        "Ya existe una plantilla con ese nombre. Elige otro nombre." => {
            "A layout with that name already exists. Choose another name."
        }
        "Puedes guardar hasta 64 plantillas. Elimina una antes de añadir otra." => {
            "You can save up to 64 layouts. Delete one before adding another."
        }
        "No se pudo crear la plantilla." => "The layout could not be created.",
        "Esta distribución no se puede guardar." => "This layout cannot be saved.",
        "La ventana ya está en ese destino." => "The window is already at that destination.",
        "Concede Accesibilidad para usar los atajos de ventanas." => {
            "Grant Accessibility permission to use window shortcuts."
        }
        "El traslado está en pausa en este contexto." => {
            "Window moves are paused in this context."
        }
        "Suelta el ratón antes de trasladar una ventana." => {
            "Release the mouse before moving a window."
        }
        "Selecciona una ventana normal de otra aplicación." => {
            "Select a standard window from another application."
        }
        "No hay una ventana activa." => "There is no active window.",
        "La ventana activa ya no está disponible." => "The active window is no longer available.",
        "Esta ventana no admite traslados." => "This window cannot be moved.",
        "No se puede leer la geometría de la ventana activa." => {
            "The active window geometry cannot be read."
        }
        "WindowServer no puede confirmar la ventana activa." => {
            "WindowServer cannot confirm the active window."
        }
        "Espera a que la ventana termine de moverse antes de trasladarla." => {
            "Wait for the window to stop moving before transferring it."
        }
        "La ventana está fijada a varios escritorios o no tiene un destino inequívoco." => {
            "The window is assigned to several desktops or has no unambiguous destination."
        }
        "La pantalla de origen no está disponible." => "The source display is unavailable.",
        "La ubicación de la ventana está cambiando; espera un instante." => {
            "The window's location is changing; wait a moment."
        }
        "Ese escritorio no existe en la pantalla de la ventana activa." => {
            "That desktop does not exist on the active window's display."
        }
        "No hay otra pantalla en esa dirección." => "There is no other display in that direction.",
        "La pantalla de destino no muestra un escritorio normal." => {
            "The destination display is not showing a standard desktop."
        }
        "Esta versión de macOS no ofrece el traslado seguro entre escritorios." => {
            "This version of macOS does not support safe transfers between desktops."
        }
        "La configuración de pantallas cambió; se comprobará la ubicación real sin aplicar coordenadas antiguas." => {
            "The display configuration changed; the actual location will be checked without applying old coordinates."
        }
        "macOS no confirmó el traslado a tiempo; se comprobará la ubicación real." => {
            "macOS did not confirm the move in time; the actual location will be checked."
        }
        "La ventana se cerró o cambió de identidad durante el traslado." => {
            "The window closed or changed identity during the move."
        }
        "Traslado interrumpido por un gesto de ratón." => {
            "The move was interrupted by a mouse gesture."
        }
        "El destino ya no está disponible." => "The destination is no longer available.",
        "La ventana ha cambiado durante el traslado." => "The window changed during the move.",
        "macOS no confirmó el traslado; no se ha aplicado una distribución nueva." => {
            "macOS did not confirm the move; no new layout was applied."
        }
        "La pantalla de destino se ha desconectado." => "The destination display was disconnected.",
        "La aplicación no confirmó el cambio de pantalla." => {
            "The application did not confirm the display change."
        }
        _ => return None,
    })
}
