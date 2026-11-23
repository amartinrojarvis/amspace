use crate::native::*;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

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
                                "Arrastra el asa o el título: en la barra reordenas; fuera eliges un hueco. ⌥ coloca sin apilar. Clic derecho o Control-clic: Colocar en…, Separar, Hacer flotante, Mover a otro grupo y Cerrar. Un destino rechazado muestra el motivo sin cambiar la distribución.",
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

/// Explicit classification for a user-facing notice. Every kind owns one SF Symbol
/// and one tint, so the plate is recognisable without reading the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// A finished operation.
    Success,
    /// An operation still in flight.
    Work,
    /// A neutral state change or proven tip.
    Info,
    /// A rejected or failed action.
    Error,
}

impl NoticeKind {
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            Self::Success => "checkmark.circle.fill",
            Self::Work => "arrow.triangle.2.circlepath",
            Self::Info => "info.circle",
            Self::Error => "exclamationmark.triangle.fill",
        }
    }
    pub(crate) fn rgb(self) -> [f64; 3] {
        match self {
            Self::Success => [0.40, 0.86, 0.55],
            Self::Work => [0.62, 0.79, 1.00],
            Self::Info => [0.86, 0.90, 1.00],
            Self::Error => [1.00, 0.55, 0.48],
        }
    }
}

const NOTICE_WIDTH: f64 = 480.;
const NOTICE_MARGIN: f64 = 18.;
const NOTICE_TEXT_X: f64 = 44.;
const NOTICE_TEXT_WIDTH: f64 = NOTICE_WIDTH - NOTICE_TEXT_X - NOTICE_MARGIN;

/// A layer-backed HUD plate with a per-kind symbol; never becomes key, so the
/// focused window of the user keeps the keyboard.
pub struct Notice {
    window: Retained<AnyObject>,
    icon: Obj,
    label: Obj,
    kind: Option<NoticeKind>,
}

impl Notice {
    pub(crate) fn window(&self) -> &Retained<AnyObject> {
        &self.window
    }

    pub fn new() -> Self {
        unsafe {
            let window = crate::ui::window(rect(0., 0., NOTICE_WIDTH, 74.), 128);
            let clear: Obj = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![&window, setBackgroundColor:clear];
            let _: () = msg_send![&window, setOpaque:false];
            let _: () = msg_send![&window, setHasShadow:true];
            let _: () = msg_send![&window, setIgnoresMouseEvents:true];
            let _: () = msg_send![&window, setLevel:3isize];
            let _: () = msg_send![&window, setHidesOnDeactivate:false];
            let _: () = msg_send![&window, setCollectionBehavior:1usize | 64 | 256];
            let root: Obj = msg_send![&window, contentView];
            let _: () = msg_send![root, setWantsLayer:true];
            crate::native::hud_surface(root, false, 0.95);
            let icon = alloc_view(class!(NSImageView), rect(NOTICE_MARGIN, 41., 18., 18.));
            let _: () = msg_send![&icon, setImageScaling:0usize];
            let _: () = msg_send![root, addSubview:&*icon];
            let label = wrapping_label(
                root,
                rect(NOTICE_TEXT_X, 16., NOTICE_TEXT_WIDTH, 42.),
                "",
                13.,
            );
            // Fixed near-white ink: the plate is always dark, so the muted
            // system color of an inactive appearance would drop the contrast.
            let ink = color(0.96, 0.97, 1., 1.);
            let _: () = msg_send![label, setTextColor:ink];
            Self {
                window,
                icon: Retained::as_ptr(&icon) as Obj,
                label,
                kind: None,
            }
        }
    }

    pub fn show(&mut self, message: &str, kind: NoticeKind) {
        self.kind = Some(kind);
        unsafe {
            text(self.label, message);
            let _: () = msg_send![self.label, setPreferredMaxLayoutWidth:NOTICE_TEXT_WIDTH];
            let intrinsic: NSSize = msg_send![self.label, intrinsicContentSize];
            let body = intrinsic.height.clamp(18., 112.);
            let height = body + 32.;
            let _: () = msg_send![
                self.label,
                setFrame:NSRect::new(NSPoint::new(NOTICE_TEXT_X, 16.), NSSize::new(NOTICE_TEXT_WIDTH, body))
            ];
            let _: () = msg_send![
                self.icon,
                setFrame:NSRect::new(NSPoint::new(NOTICE_MARGIN, 16. + body - 18.), NSSize::new(18., 18.))
            ];
            let symbol = NSString::from_str(kind.symbol());
            let description = NSString::from_str(message);
            let image: Option<Retained<AnyObject>> = msg_send![
                class!(NSImage),
                imageWithSystemSymbolName:&*symbol,
                accessibilityDescription:&*description
            ];
            if let Some(image) = image {
                let _: () = msg_send![self.icon, setImage:&*image];
            }
            let [red, green, blue] = kind.rgb();
            let tint = color(red, green, blue, 1.);
            let _: () = msg_send![self.icon, setContentTintColor:tint];
            let _: () = msg_send![&self.window, setContentSize:NSSize::new(NOTICE_WIDTH, height)];
            let screen: Obj = msg_send![class!(NSScreen), mainScreen];
            if screen.is_null() {
                return;
            }
            let visible: NSRect = msg_send![screen, visibleFrame];
            let x = visible.origin.x + (visible.size.width - NOTICE_WIDTH) / 2.;
            let y = visible.origin.y + visible.size.height - height - 24.;
            let _: () = msg_send![&self.window, setFrameOrigin:NSPoint::new(x, y)];
            let _: () = msg_send![&self.window, orderFrontRegardless];
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _: () = msg_send![&self.window, orderOut:std::ptr::null::<AnyObject>()];
        }
    }

    /// Observed facets for the native inspection mode.
    pub(crate) fn observed(&self) -> NoticeState {
        unsafe {
            let style: isize = msg_send![&self.window, styleMask];
            let ignores_mouse: bool = msg_send![&self.window, ignoresMouseEvents];
            let image: Obj = msg_send![self.icon, image];
            let has_image = !image.is_null();
            let body: *mut NSString = msg_send![self.label, stringValue];
            let message = body
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_default();
            // The image view keeps the exact symbol authored for this kind.
            let symbol = self
                .kind
                .unwrap_or(NoticeKind::Info)
                .symbol()
                .to_string();
            let key: bool = msg_send![&self.window, isKeyWindow];
            let visible: bool = msg_send![&self.window, isVisible];
            NoticeState {
                style,
                ignores_mouse,
                has_image,
                message,
                symbol,
                key,
                visible,
            }
        }
    }
}

/// Everything the inspection mode can verify without executing anything.
pub(crate) struct NoticeState {
    pub(crate) style: isize,
    pub(crate) ignores_mouse: bool,
    pub(crate) has_image: bool,
    pub(crate) message: String,
    pub(crate) symbol: String,
    pub(crate) key: bool,
    pub(crate) visible: bool,
}
