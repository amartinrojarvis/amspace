use crate::native::*;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRect, NSString};

pub struct Guide {
    window: Retained<AnyObject>,
    status: Obj,
    tips: Obj,
    tabs: Obj,
}

impl Guide {
    pub fn new(target: Obj, tips_enabled: bool) -> Self {
        unsafe {
            let window = crate::ui::window(rect(0., 0., 680., 570.), 1 | 2);
            let _: () =
                msg_send![&window, setTitle:&*NSString::from_str("AMSpace · Instalación y ayuda")];
            let _: () = msg_send![&window, center];
            let root: Obj = msg_send![&window, contentView];
            let tabs = alloc_view(class!(NSTabView), rect(16., 78., 648., 476.));
            let _: () = msg_send![root, addSubview:&*tabs];
            let mut status = std::ptr::null_mut();
            for (index, title) in [
                "Instalación y permisos",
                "Pestañas y gestos",
                "Privacidad y seguridad",
            ]
            .iter()
            .enumerate()
            {
                let item: Retained<AnyObject> = msg_send![class!(NSTabViewItem), new];
                let _: () = msg_send![&item, setLabel:&*NSString::from_str(title)];
                let page = alloc_view(class!(NSView), rect(0., 0., 620., 430.));
                let page_ptr = Retained::as_ptr(&page) as Obj;
                let sections: [(&str, &str); 3] = match index {
                    0 => [
                        (
                            "1. Copia AMSpace a Aplicaciones",
                            "En el DMG, arrastra AMSpace a Aplicaciones. Expulsa el disco y abre la copia instalada. La app vive en la barra de menús: clic derecho en su indicador para abrir Ajustes.",
                        ),
                        (
                            "2. Si macOS bloquea la primera apertura",
                            "La distribución actual tiene firma ad hoc, sin Developer ID ni notarización. Solo si confías en la descarga oficial: Ajustes del Sistema → Privacidad y seguridad → Abrir igualmente. No ignores una advertencia de malware ni desactives Gatekeeper.",
                        ),
                        (
                            "3. Autoriza Accesibilidad",
                            "Ajustes del Sistema → Privacidad y seguridad → Accesibilidad. Activa AMSpace; si no aparece, usa + y elige Aplicaciones/AMSpace.app. Vuelve aquí y revisa el permiso. Después activa las funciones que quieras en Ajustes.",
                        ),
                    ],
                    1 => [
                        (
                            "Agrupar, seleccionar y cerrar",
                            "Con Mosaico y agrupación activos, arrastra una ventana sobre la zona superior de otra. Pulsa una pestaña para seleccionarla; × solicita el cierre normal de la ventana. Los avisos de guardar siguen perteneciendo a cada aplicación.",
                        ),
                        (
                            "Separar y reorganizar",
                            "Arrastra una pestaña fuera del grupo para independizarla. Clic derecho o Control-clic abre Separar, Hacer flotante, Mover a otro grupo y Cerrar ventana. Los destinos incompatibles indican el motivo: capacidad o tamaño mínimo.",
                        ),
                        (
                            "Atajos y pausa a mano",
                            "En Ajustes → Atajos puedes activar y configurar pestaña anterior, siguiente y separar la activa. Están desactivados inicialmente para no reservar teclas. Pausar Mosaico detiene sus colocaciones sin deshacer la distribución; reanúdalo en la barra lateral de Ajustes.",
                        ),
                    ],
                    _ => [
                        (
                            "Qué permite Accesibilidad",
                            "Es un permiso sensible: permite consultar y manipular ventanas de otras apps, incluidos títulos, posición y tamaño. Concédelo solo si confías en AMSpace. Puedes revocarlo en cualquier momento desde Ajustes del Sistema.",
                        ),
                        (
                            "Qué permanece en tu Mac",
                            "AMSpace no necesita Grabación de pantalla ni incluye telemetría. Las preferencias se guardan localmente. Las comprobaciones de actualización de Sparkle consultan el catálogo y descargan paquetes publicados en GitHub; puedes desactivar la búsqueda automática en Ajustes → General.",
                        ),
                        (
                            "Qué garantiza la distribución actual",
                            "La firma Ed25519 del catálogo de actualizaciones no equivale a una firma Developer ID ni a la notarización de Apple. Una actualización ad hoc puede requerir conceder Accesibilidad de nuevo. El código fuente y las licencias están disponibles para revisión.",
                        ),
                    ],
                };
                for (row, (heading, body)) in sections.iter().enumerate() {
                    let y = 396. - row as f64 * 119.;
                    label(page_ptr, rect(18., y, 584., 24.), heading, 15., 0.3, false);
                    wrapping_label(page_ptr, rect(18., y - 80., 584., 73.), body, 12.);
                }
                if index == 0 {
                    status = label(page_ptr, rect(18., 47., 584., 21.), "", 12., 0.3, false);
                    crate::ui::button(
                        page_ptr,
                        rect(14., 8., 188., 30.),
                        "Abrir Accesibilidad…",
                        target,
                        6,
                    );
                    crate::ui::button(
                        page_ptr,
                        rect(214., 8., 158., 30.),
                        "Revisar permiso",
                        target,
                        115,
                    );
                    crate::ui::button(
                        page_ptr,
                        rect(384., 8., 222., 30.),
                        "Privacidad y seguridad…",
                        target,
                        114,
                    );
                } else if index == 2 {
                    crate::ui::button(
                        page_ptr,
                        rect(14., 12., 220., 30.),
                        "Privacidad y seguridad…",
                        target,
                        114,
                    );
                }
                let _: () = msg_send![&item, setView:&*page];
                let _: () = msg_send![&tabs, addTabViewItem:&*item];
            }
            let tips = crate::ui::button(
                root,
                rect(26., 24., 440., 30.),
                "Mostrar ayudas en los primeros usos",
                target,
                111,
            );
            let _: () = msg_send![tips, setButtonType:3usize];
            let _: () = msg_send![tips, setState:isize::from(tips_enabled)];
            crate::ui::button(root, rect(530., 23., 124., 32.), "Entendido", target, 112);
            Self {
                window,
                status,
                tips,
                tabs: Retained::as_ptr(&tabs) as Obj,
            }
        }
    }

    pub fn show(&self, trusted: bool, tips_enabled: bool, page: usize) {
        self.refresh(trusted);
        unsafe {
            let _: () = msg_send![self.tips, setState:isize::from(tips_enabled)];
            let _: () = msg_send![self.tabs, selectTabViewItemAtIndex:page as isize];
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps:true];
            let _: () = msg_send![&self.window, makeKeyAndOrderFront:std::ptr::null::<AnyObject>()];
        }
    }

    pub fn refresh(&self, trusted: bool) {
        unsafe {
            text(
                self.status,
                if trusted {
                    "Accesibilidad concedida. Puedes activar Borde, Foco y Mosaico."
                } else {
                    "Accesibilidad pendiente. Concede el permiso a la copia instalada."
                },
            );
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _: () = msg_send![&self.window, orderOut:std::ptr::null::<AnyObject>()];
        }
    }
}

/// Allocated on the first user-facing notice; no focus changes or periodic timer.
pub struct Notice {
    window: Retained<AnyObject>,
    label: Obj,
}
impl Notice {
    pub fn new() -> Self {
        unsafe {
            let window = crate::ui::window(rect(0., 0., 480., 88.), 128);
            let clear: Obj = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![&window, setBackgroundColor:clear];
            let _: () = msg_send![&window, setOpaque:false];
            let _: () = msg_send![&window, setHasShadow:true];
            let _: () = msg_send![&window, setIgnoresMouseEvents:true];
            let _: () = msg_send![&window, setLevel:3isize];
            let _: () = msg_send![&window, setHidesOnDeactivate:false];
            let _: () = msg_send![&window, setCollectionBehavior:1usize | 64 | 256];
            let root: Obj = msg_send![&window, contentView];
            crate::ui::card(root, rect(0., 0., 480., 88.));
            let label = wrapping_label(root, rect(18., 14., 444., 58.), "", 13.);
            Self { window, label }
        }
    }
    pub fn show(&self, message: &str) {
        unsafe {
            text(self.label, message);
            let screen: Obj = msg_send![class!(NSScreen), mainScreen];
            if screen.is_null() {
                return;
            }
            let visible: NSRect = msg_send![screen, visibleFrame];
            let _: () = msg_send![&self.window, setFrameOrigin:NSPoint::new(visible.origin.x + (visible.size.width - 480.) / 2., visible.origin.y + visible.size.height - 112.)];
            let _: () = msg_send![&self.window, orderFrontRegardless];
        }
    }
    pub fn hide(&self) {
        unsafe {
            let _: () = msg_send![&self.window, orderOut:std::ptr::null::<AnyObject>()];
        }
    }
}
