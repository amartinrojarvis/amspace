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
    pub(crate) fn window(&self) -> &Retained<AnyObject> {
        &self.window
    }

    pub fn new(target: Obj, tips_enabled: bool) -> Self {
        unsafe {
            let window = crate::ui::window(rect(0., 0., 680., 570.), 1 | 2);
            let _: () = msg_send![&window, setTitle:&*NSString::from_str(crate::i18n::tr("AMSpace · Instalación y ayuda"))];
            let _: () = msg_send![&window, center];
            let root: Obj = msg_send![&window, contentView];
            let tabs = alloc_view(class!(NSTabView), rect(16., 78., 648., 476.));
            let _: () = msg_send![root, addSubview:&*tabs];
            let mut status = std::ptr::null_mut();
            for (index, title) in [
                crate::i18n::tr("Instalación y permisos"),
                crate::i18n::tr("Pestañas y gestos"),
                crate::i18n::tr("Privacidad y seguridad"),
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
                            crate::i18n::tr("1. Copia AMSpace a Aplicaciones"),
                            crate::i18n::tr(
                                "En el DMG, arrastra AMSpace a Aplicaciones. Expulsa el disco y abre la copia instalada. La app vive en la barra de menús: clic derecho en su indicador para abrir Ajustes.",
                            ),
                        ),
                        (
                            crate::i18n::tr("2. Si macOS bloquea la primera apertura"),
                            crate::i18n::tr(
                                "La distribución actual tiene firma ad hoc, sin Developer ID ni notarización. Solo si confías en la descarga oficial: Ajustes del Sistema → Privacidad y seguridad → Abrir igualmente. No ignores una advertencia de malware ni desactives Gatekeeper.",
                            ),
                        ),
                        (
                            crate::i18n::tr("3. Autoriza Accesibilidad"),
                            crate::i18n::tr(
                                "Ajustes del Sistema → Privacidad y seguridad → Accesibilidad. Activa AMSpace; si no aparece, usa + y elige Aplicaciones/AMSpace.app. Vuelve aquí y revisa el permiso. Después activa las funciones que quieras en Ajustes.",
                            ),
                        ),
                    ],
                    1 => [
                        (
                            crate::i18n::tr("Agrupar, seleccionar y cerrar"),
                            crate::i18n::tr(
                                "Con Mosaico y agrupación activos, arrastra una ventana sobre la zona superior de otra. Pulsa una pestaña para seleccionarla; × solicita el cierre normal de la ventana. Los avisos de guardar siguen perteneciendo a cada aplicación.",
                            ),
                        ),
                        (
                            crate::i18n::tr("Separar y reorganizar"),
                            crate::i18n::tr(
                                "Arrastra dentro de la barra para reordenar; fuera de ella para separar la ventana. La marca indica dónde quedará. Clic derecho o Control-clic: Separar, Hacer flotante, Mover a otro grupo y Cerrar. Si no cabe, la distribución no cambia.",
                            ),
                        ),
                        (
                            crate::i18n::tr("Atajos y pausa a mano"),
                            crate::i18n::tr(
                                "En Ajustes → Atajos puedes activar y configurar pestaña anterior, siguiente y separar la activa. Están desactivados inicialmente para no reservar teclas. Pausar Mosaico detiene sus colocaciones sin deshacer la distribución; reanúdalo en la barra lateral de Ajustes.",
                            ),
                        ),
                    ],
                    _ => [
                        (
                            crate::i18n::tr("Qué permite Accesibilidad"),
                            crate::i18n::tr(
                                "Es un permiso sensible: permite consultar y manipular ventanas de otras apps, incluidos títulos, posición y tamaño. Concédelo solo si confías en AMSpace. Puedes revocarlo en cualquier momento desde Ajustes del Sistema.",
                            ),
                        ),
                        (
                            crate::i18n::tr("Qué permanece en tu Mac"),
                            crate::i18n::tr(
                                "AMSpace no necesita Grabación de pantalla ni incluye telemetría. Las preferencias se guardan localmente. Las comprobaciones de actualización de Sparkle consultan el catálogo y descargan paquetes publicados en GitHub; puedes desactivar la búsqueda automática en Ajustes → General.",
                            ),
                        ),
                        (
                            crate::i18n::tr("Qué garantiza la distribución actual"),
                            crate::i18n::tr(
                                "La firma Ed25519 del catálogo de actualizaciones no equivale a una firma Developer ID ni a la notarización de Apple. Una actualización ad hoc puede requerir conceder Accesibilidad de nuevo. El código fuente y las licencias están disponibles para revisión.",
                            ),
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
                        crate::i18n::tr("Abrir Accesibilidad…"),
                        target,
                        6,
                    );
                    crate::ui::button(
                        page_ptr,
                        rect(214., 8., 158., 30.),
                        crate::i18n::tr("Revisar permiso"),
                        target,
                        115,
                    );
                    crate::ui::button(
                        page_ptr,
                        rect(384., 8., 222., 30.),
                        crate::i18n::tr("Privacidad y seguridad…"),
                        target,
                        114,
                    );
                } else if index == 2 {
                    crate::ui::button(
                        page_ptr,
                        rect(14., 12., 220., 30.),
                        crate::i18n::tr("Privacidad y seguridad…"),
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
                crate::i18n::tr("Mostrar ayudas en los primeros usos"),
                target,
                111,
            );
            let _: () = msg_send![tips, setButtonType:3usize];
            let _: () = msg_send![tips, setState:isize::from(tips_enabled)];
            crate::ui::button(
                root,
                rect(530., 23., 124., 32.),
                crate::i18n::tr("Entendido"),
                target,
                112,
            );
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
                    crate::i18n::tr(
                        "Accesibilidad concedida. Puedes activar Borde, Foco y Mosaico.",
                    )
                } else {
                    crate::i18n::tr(
                        "Accesibilidad pendiente. Concede el permiso a la copia instalada.",
                    )
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

pub struct Feedback {
    window: Retained<AnyObject>,
    status: Obj,
}

impl Feedback {
    pub(crate) fn window(&self) -> &Retained<AnyObject> {
        &self.window
    }

    pub fn new(target: Obj) -> Self {
        unsafe {
            let window = crate::ui::window(rect(0., 0., 520., 300.), 1 | 2);
            let _: () = msg_send![&window, setTitle:&*NSString::from_str("AMSpace · Feedback")];
            let _: () = msg_send![&window, center];
            let root: Obj = msg_send![&window, contentView];
            label(
                root,
                rect(18., 250., 484., 28.),
                crate::i18n::tr("Tu experiencia ayuda a mejorar AMSpace"),
                18.,
                0.4,
                false,
            );
            wrapping_label(
                root,
                rect(18., 205., 484., 36.),
                crate::i18n::tr(
                    "Cuéntame un error, una sugerencia o cómo utilizas la app en tu equipo.",
                ),
                13.,
            );
            let address = label(
                root,
                rect(18., 170., 484., 26.),
                "a.martinro@gmail.com",
                15.,
                0.3,
                false,
            );
            let _: () = msg_send![address, setSelectable:true];
            wrapping_label(
                root,
                rect(18., 105., 484., 56.),
                crate::i18n::tr(
                    "Preparar correo abre un borrador en tu aplicación de correo. Tú decides qué incluir y cuándo enviarlo. No se adjuntan diagnósticos ni datos de tus ventanas.",
                ),
                12.,
            );
            crate::ui::button(
                root,
                rect(14., 62., 194., 30.),
                crate::i18n::tr("Preparar correo"),
                target,
                117,
            );
            crate::ui::button(
                root,
                rect(218., 62., 174., 30.),
                crate::i18n::tr("Copiar dirección"),
                target,
                118,
            );
            let status = wrapping_label(
                root,
                rect(18., 12., 484., 42.),
                crate::i18n::tr(
                    "Si no tienes un cliente de correo configurado, utiliza esta dirección desde tu webmail.",
                ),
                12.,
            );
            Self { window, status }
        }
    }

    pub fn show(&self) {
        unsafe {
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps:true];
            let _: () = msg_send![&self.window, makeKeyAndOrderFront:std::ptr::null::<AnyObject>()];
        }
    }

    fn email_url() -> Option<Retained<AnyObject>> {
        unsafe {
            let components: Retained<AnyObject> = msg_send![class!(NSURLComponents), new];
            let _: () = msg_send![&components, setScheme:&*NSString::from_str("mailto")];
            let _: () =
                msg_send![&components, setPath:&*NSString::from_str("a.martinro@gmail.com")];
            let subject: Retained<AnyObject> = msg_send![class!(NSURLQueryItem), queryItemWithName:&*NSString::from_str("subject"), value:&*NSString::from_str(concat!("AMSpace ", env!("CARGO_PKG_VERSION"), " · Feedback"))];
            let body: Retained<AnyObject> = msg_send![class!(NSURLQueryItem), queryItemWithName:&*NSString::from_str("body"), value:&*NSString::from_str(crate::i18n::tr("Hola Alberto:\r\n\r\nMi comentario o sugerencia:\r\n\r\nSi se trata de un error:\r\n- Pasos para reproducirlo:\r\n- Qué esperaba y qué ocurrió:\r\n- Versión de macOS y configuración de pantallas (opcional):\r\n\r\nRevisaré cualquier diagnóstico antes de adjuntarlo; no incluiré datos privados.\r\n"))];
            let items = [Retained::as_ptr(&subject), Retained::as_ptr(&body)];
            let query: Obj =
                msg_send![class!(NSArray), arrayWithObjects:items.as_ptr(), count:items.len()];
            let _: () = msg_send![&components, setQueryItems:query];
            msg_send![&components, URL]
        }
    }

    pub fn prepare_email(&self) {
        unsafe {
            let opened = Self::email_url().is_some_and(|url| {
                let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
                msg_send![workspace, openURL:&*url]
            });
            text(
                self.status,
                if opened {
                    crate::i18n::tr(
                        "Se ha solicitado abrir el borrador, no enviar el mensaje. Si tu cliente no está configurado, escribe a la dirección desde tu webmail.",
                    )
                } else {
                    crate::i18n::tr(
                        "No se pudo abrir el correo. Copia la dirección y escribe desde tu cliente o webmail.",
                    )
                },
            );
        }
    }

    pub fn copy_address(&self) {
        unsafe {
            let pasteboard: Obj = msg_send![class!(NSPasteboard), generalPasteboard];
            let _: isize = msg_send![pasteboard, clearContents];
            let copied: bool = msg_send![pasteboard, setString:&*NSString::from_str("a.martinro@gmail.com"), forType:&*NSString::from_str("public.utf8-plain-text")];
            text(
                self.status,
                if copied {
                    crate::i18n::tr("Dirección copiada. No se ha enviado ningún mensaje.")
                } else {
                    crate::i18n::tr(
                        "No se pudo copiar. Selecciona la dirección de arriba y cópiala manualmente.",
                    )
                },
            );
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
