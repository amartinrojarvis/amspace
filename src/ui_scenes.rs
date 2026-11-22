//! Manual scene review; selecting a scene or a window never moves anything.
use super::*;
use crate::{i18n::tr, scene_library, tile_manager::scenes::SceneReview};

pub(super) struct Panel {
    pub(super) window: Retained<AnyObject>,
    name: Obj,
    library: Obj,
    delete: Obj,
    scroll: Obj,
    status: Obj,
    apply: Obj,
    entries: Vec<scene_library::Entry>,
    review: Option<SceneReview>,
    controls: Vec<Obj>,
    confirming: bool,
}

/// One normalized rectangle per ordinal slot, in the same order as the saved bundles.
fn slot_previews(entry: &scene_library::Entry) -> Vec<NSRect> {
    let mut slots = Vec::new();
    for display in &entry.displays {
        let base = slots.len();
        slots.resize(base + display.bundles.len(), NSRect::ZERO);
        if let Some(desktop) = scene_library::ordinal_desktop(display) {
            for (id, r) in desktop.frames(rect(0., 0., 1200., 800.), 0.) {
                slots[base + id as usize - 1] = rect(
                    r.origin.x / 1200.,
                    1. - (r.origin.y + r.size.height) / 800.,
                    r.size.width / 1200.,
                    r.size.height / 800.,
                );
            }
        }
    }
    slots
}

unsafe fn slot_preview(root: Obj, y: f64, slot: NSRect, available: bool) {
    unsafe {
        let view = alloc_view(class!(NSView), rect(9., y + 6., 52., 34.));
        let pane = alloc_view(
            class!(NSView),
            rect(
                2. + slot.origin.x * 48.,
                2. + slot.origin.y * 30.,
                (slot.size.width * 48.).max(1.),
                (slot.size.height * 30.).max(1.),
            ),
        );
        let surface: Obj = msg_send![class!(NSColor), quaternaryLabelColor];
        let ink: Obj = if available {
            msg_send![class!(NSColor), controlAccentColor]
        } else {
            msg_send![class!(NSColor), tertiaryLabelColor]
        };
        for (v, c, r) in [(&view, surface, 4.), (&pane, ink, 2.)] {
            let _: () = msg_send![&**v, setWantsLayer:true];
            let layer: Obj = msg_send![&**v, layer];
            let cg: *const CgColor = msg_send![c, CGColor];
            let _: () = msg_send![layer, setBackgroundColor:cg];
            let _: () = msg_send![layer, setCornerRadius:r];
        }
        let _: () = msg_send![&view, addSubview:&*pane];
        let _: () = msg_send![root, addSubview:&*view];
    }
}

pub(super) extern "C-unwind" fn will_close(_: Obj, _: Sel, notice: Obj) {
    let closed: Obj = unsafe { msg_send![notice, object] };
    with_app(|app| {
        if let Some(panel) = app
            .scenes
            .as_mut()
            .filter(|p| ptr::eq(Retained::as_ptr(&p.window), closed))
        {
            panel.review = None;
            panel.controls.clear();
            unsafe {
                let _: () = msg_send![panel.scroll, setDocumentView:ptr::null::<AnyObject>()];
            }
        }
    });
}

impl Panel {
    fn new(target: Obj) -> Self {
        unsafe {
            let window = window(rect(0., 0., 620., 478.), 3);
            let _: () = msg_send![&window, setDelegate:target];
            let root: Obj = msg_send![&window, contentView];
            let title = tr("Escenas de trabajo");
            let _: () = msg_send![&window, setTitle:&*NSString::from_str(title)];
            label(root, rect(20., 428., 580., 30.), title, 23., 0.55, false);
            label(
                root,
                rect(21., 402., 578., 23.),
                tr(
                    "Guarda el mosaico visible de cada pantalla y elige las ventanas al recuperarlo.",
                ),
                13.,
                0.,
                true,
            );
            let name = alloc_view(class!(NSTextField), rect(20., 365., 395., 26.));
            let placeholder = NSString::from_str(tr("Nombre de la nueva escena"));
            let _: () = msg_send![&name, setPlaceholderString:&*placeholder];
            let _: () = msg_send![&name, setAccessibilityLabel:&*placeholder];
            let _: () = msg_send![root, addSubview:&*name];
            button(
                root,
                rect(421., 362., 179., 31.),
                tr("Guardar escena actual"),
                target,
                131,
            );
            let library = popup(root, rect(17., 315., 365., 28.), &[], target, 132);
            let _: () = msg_send![library, setAccessibilityLabel:&*NSString::from_str(tr("Escenas guardadas"))];
            button(
                root,
                rect(381., 314., 118., 30.),
                tr("Revisar"),
                target,
                134,
            );
            let delete = button(
                root,
                rect(497., 314., 105., 30.),
                tr("Eliminar"),
                target,
                133,
            );
            let scroll = alloc_view(class!(NSScrollView), rect(20., 91., 580., 212.));
            let _: () = msg_send![&scroll, setHasVerticalScroller:true];
            let _: () = msg_send![&scroll, setBorderType:1usize];
            let _: () = msg_send![root, addSubview:&*scroll];
            let status = wrapping_label(root, rect(20., 9., 408., 70.), "", 12.);
            let apply = button(
                root,
                rect(438., 27., 163., 32.),
                tr("Aplicar escena"),
                target,
                135,
            );
            let _: () = msg_send![&window, center];
            Self {
                window,
                name: Retained::as_ptr(&name) as Obj,
                library,
                delete,
                scroll: Retained::as_ptr(&scroll) as Obj,
                status,
                apply,
                entries: Vec::new(),
                review: None,
                controls: Vec::new(),
                confirming: false,
            }
        }
    }

    fn selected(&self) -> Option<&scene_library::Entry> {
        let index: isize = unsafe { msg_send![self.library, indexOfSelectedItem] };
        self.entries.get(usize::try_from(index).ok()?)
    }
    fn reload(&mut self, defaults: Obj, id: Option<&str>) {
        self.entries = scene_library::entries(defaults);
        unsafe {
            let _: () = msg_send![self.library, removeAllItems];
            for (i, entry) in self.entries.iter().enumerate() {
                let _: () =
                    msg_send![self.library, addItemWithTitle:&*NSString::from_str(&entry.name)];
                if Some(entry.id.as_str()) == id {
                    let _: () = msg_send![self.library, selectItemAtIndex:i as isize];
                }
            }
            let _: () = msg_send![self.delete, setEnabled:!self.entries.is_empty()];
        }
    }
    fn reset_delete(&mut self) {
        self.confirming = false;
        unsafe {
            let _: () = msg_send![self.delete, setTitle:&*NSString::from_str(tr("Eliminar"))];
        }
    }
    fn populate(&mut self, review: Option<SceneReview>, target: Obj) {
        self.review = review;
        self.controls.clear();
        self.reset_delete();
        unsafe {
            let count = self.review.as_ref().map_or(0, |r| r.rows.len());
            let height = (count as f64 * 53. + 12.).max(208.);
            let doc = alloc_view(class!(NSView), rect(0., 0., 556., height));
            let root = Retained::as_ptr(&doc) as Obj;
            if let Some(review) = &self.review {
                let slots = self.selected().map(slot_previews).unwrap_or_default();
                for (index, row) in review.rows.iter().enumerate() {
                    let y = height - 52. - index as f64 * 53.;
                    if let Some(slot) = slots.get(index) {
                        slot_preview(root, y, *slot, row.available);
                    }
                    let heading = format!("{} · {}", row.slot, row.display);
                    label(
                        root,
                        rect(70., y + 23., 164., 19.),
                        &heading,
                        13.,
                        0.45,
                        false,
                    );
                    let app = label(
                        root,
                        rect(70., y + 3., 164., 19.),
                        row.candidates
                            .first()
                            .and_then(|c| c.app_name.as_deref())
                            .unwrap_or(&row.bundle),
                        11.,
                        0.,
                        true,
                    );
                    let _: () = msg_send![app, setLineBreakMode:4isize];
                    let _: () = msg_send![app, setToolTip:&*NSString::from_str(&row.bundle)];
                    let first = if !row.available {
                        tr("Pantalla desconectada")
                    } else if row.candidates.is_empty() {
                        tr("Sin ventana compatible")
                    } else {
                        tr("Elige una ventana…")
                    };
                    let menu = popup(root, rect(236., y + 6., 315., 29.), &[first], target, 136);
                    let _: () = msg_send![menu, setAccessibilityLabel:&*NSString::from_str(&format!("{heading} · {}",row.bundle))];
                    for candidate in &row.candidates {
                        // NSMenuItem, not addItemWithTitle:, preserves equal window titles.
                        let native_menu: Obj = msg_send![menu, menu];
                        let item: Retained<AnyObject> = msg_send![class!(NSMenuItem), new];
                        let _: () =
                            msg_send![&item,setTitle:&*NSString::from_str(&candidate.label)];
                        let _: () = msg_send![native_menu,addItem:&*item];
                    }
                    if row.available && row.candidates.len() == 1 {
                        let _: () = msg_send![menu, selectItemAtIndex:1isize];
                    }
                    let _: () =
                        msg_send![menu, setEnabled:row.available && !row.candidates.is_empty()];
                    self.controls.push(menu);
                }
            } else {
                label(
                    root,
                    rect(14., height - 40., 525., 26.),
                    tr("La revisión de ventanas aparecerá aquí."),
                    14.,
                    0.,
                    true,
                );
            }
            let _: () = msg_send![self.scroll, setDocumentView:&*doc];
            let _: () = msg_send![&doc, scrollPoint:NSPoint::new(0.,height)];
        }
        self.update_status();
    }
    fn choices(&self) -> Option<Vec<Option<u32>>> {
        let review = self.review.as_ref()?;
        let mut seen = std::collections::HashSet::new();
        let mut choices = Vec::with_capacity(review.rows.len());
        for (row, menu) in review.rows.iter().zip(&self.controls) {
            if !row.available {
                choices.push(None);
                continue;
            }
            let index: isize = unsafe { msg_send![*menu, indexOfSelectedItem] };
            let candidate = row.candidates.get(usize::try_from(index - 1).ok()?)?;
            if !seen.insert(candidate.id) {
                return None;
            }
            choices.push(Some(candidate.id));
        }
        (!seen.is_empty()).then_some(choices)
    }
    fn update_status(&self) {
        let ready = self.choices().is_some();
        let message = if self.entries.is_empty() {
            tr("Guarda una escena para empezar.")
        } else if ready {
            tr("Revisa los destinos. Las pantallas desconectadas se omiten.")
        } else {
            tr("Elige una ventana distinta para cada puesto de las pantallas disponibles.")
        };
        unsafe {
            let _: () = msg_send![self.apply,setEnabled:ready];
            text(self.status, message);
        }
    }
}

impl App {
    pub(super) fn scene_action(&mut self, tag: isize) {
        let target = Retained::as_ptr(&self.target) as Obj;
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let mut panel = self.scenes.take().unwrap_or_else(|| Panel::new(target));
        let result = (|| -> Result<(), String> {
            if tag == 130 {
                let selected = panel.selected().map(|e| e.id.clone());
                panel.reload(defaults, selected.as_deref());
                unsafe {
                    let _: () =
                        msg_send![&panel.window,makeKeyAndOrderFront:ptr::null::<AnyObject>()];
                }
            }
            if tag == 133 {
                if !panel.confirming {
                    panel.confirming = true;
                    unsafe {
                        let _: () =
                            msg_send![panel.delete,setTitle:&*NSString::from_str(tr("Confirmar"))];
                        text(
                            panel.status,
                            tr("Pulsa Confirmar para eliminar esta escena."),
                        );
                    }
                    return Ok(());
                }
                if let Some(entry) = panel.selected() {
                    scene_library::remove(defaults, &entry.id);
                }
                panel.reload(defaults, None);
            }
            panel.reset_delete();
            if tag == 136 {
                panel.update_status();
                return Ok(());
            }
            if tag != 135 {
                panel.populate(None, target);
            }
            if self.sleeping
                || self.tiling_paused
                || crate::desktop_mode::active()
                || self.transfer.is_some()
                || self.switching.is_some()
                || crate::tile_manager::pref(defaults, "tile.enabled") == 0
            {
                return Err(tr("Activa el mosaico y espera a que termine la transición.").into());
            }
            if tag == 131 {
                let value: Retained<NSString> = unsafe { msg_send![panel.name, stringValue] };
                let scene = self.tile_manager.capture_scene(defaults)?;
                let id = scene_library::save(defaults, &value.to_string(), &scene)?;
                panel.reload(defaults, Some(&id));
                unsafe {
                    text(panel.name, "");
                }
            }
            if tag == 135 {
                if let (Some(review), Some(choices)) = (&panel.review, panel.choices()) {
                    let outcome = self.tile_manager.apply_scene(defaults, review, &choices);
                    panel.populate(None, target);
                    outcome?;
                    unsafe {
                        text(panel.status, tr("Escena aplicada."));
                    }
                    self.refresh_tile_controls();
                }
                return Ok(());
            }
            if let Some(entry) = panel.selected() {
                let review = self.tile_manager.review_scene(defaults, &entry.displays)?;
                panel.populate(Some(review), target);
            }
            Ok(())
        })();
        if let Err(message) = result {
            unsafe {
                text(panel.status, &message);
            }
        }
        self.scenes = Some(panel);
    }
}

/// Uses the manager populated exclusively with the three disposable AX windows.
pub(crate) fn check_scene_fixture(
    mut manager: crate::tile_manager::Manager,
    ids: &[u32],
    fixture_pid: i32,
    area: NSRect,
) -> Result<(), String> {
    unsafe {
        let suite = NSString::from_str(&format!("amspace.scene-ui.{}", std::process::id()));
        let alloc: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let prefs: Retained<AnyObject> = msg_send![alloc, initWithSuiteName:&*suite];
        let defaults = Retained::as_ptr(&prefs) as Obj;
        let _: () = msg_send![&prefs, removePersistentDomainForName:&*suite];
        let _: () = msg_send![&prefs, setInteger:1isize, forKey:ns_string!("tile.enabled")];
        let target = controller();
        let target = Retained::as_ptr(&target) as Obj;
        let mut panel = Panel::new(target);
        let result = (|| {
            let geometry = |id| {
                let window =
                    ax::window_for_id(fixture_pid, id).ok_or("Ha desaparecido el fixture.")?;
                let (p, s) = ax::geometry(window.0).ok_or("Sin geometría del fixture.")?;
                Ok::<_, String>(NSRect::new(p, s))
            };
            let before = ids
                .iter()
                .copied()
                .map(geometry)
                .collect::<Result<Vec<_>, _>>()?;
            let mut displays = manager.capture_scene(defaults)?;
            if displays.len() != 1 || displays[0].bundles.len() != ids.len() {
                return Err("La captura no coincide con las ventanas desechables.".into());
            }
            let offline = crate::tile_manager::scenes::SceneDisplay {
                name: "Monitor desconectado".into(),
                uuid: "00000000-0000-0000-0000-000000000000".into(),
                distribution: displays[0].distribution.clone(),
                bundles: displays[0].bundles.clone(),
            };
            displays.push(offline);
            let saved = scene_library::save(defaults, "Estudio", &displays)?;
            panel.reload(defaults, Some(&saved));
            let review = manager.review_scene(
                defaults,
                &panel.selected().ok_or("Escena no guardada.")?.displays,
            )?;
            panel.populate(Some(review), target);
            if panel.choices().is_some() {
                return Err("Se eligieron automáticamente ventanas ambiguas.".into());
            }
            let review = panel.review.as_ref().unwrap();
            let mut occupied = 0;
            let mut skipped = 0;
            for (row, control) in review.rows.iter().zip(&panel.controls) {
                if !row.available {
                    skipped += 1;
                    continue;
                }
                let wanted = ids[(occupied + 1) % ids.len()];
                let index = row
                    .candidates
                    .iter()
                    .position(|c| c.id == wanted)
                    .ok_or("Falta la ventana elegida en el selector nativo.")?;
                let _: () = msg_send![*control, selectItemAtIndex:(index+1) as isize];
                occupied += 1;
            }
            panel.update_status();
            if occupied != ids.len() || skipped != ids.len() {
                return Err("Una pantalla ausente cambió los destinos de revisión.".into());
            }
            let choices = panel
                .choices()
                .ok_or("No se habilitó la asignación completa.")?;
            let mut duplicate = choices.clone();
            let mut active = duplicate.iter_mut().filter(|choice| choice.is_some());
            let first = *active.next().unwrap();
            *active.next().unwrap() = first;
            if manager.apply_scene(defaults, review, &duplicate).is_ok() {
                return Err("El motor aceptó una ventana repetida.".into());
            }
            for (&id, frame) in ids.iter().zip(before) {
                if geometry(id)? != frame {
                    return Err("Revisar o rechazar una asignación movió una ventana.".into());
                }
            }
            let _: () = msg_send![&panel.window, setHidesOnDeactivate:false];
            let _: () = msg_send![&panel.window, setLevel:3isize];
            let _: () = msg_send![&panel.window, orderFrontRegardless];
            super::diagnostics::pump(0.08);
            let _: () = msg_send![&panel.window, displayIfNeeded];
            super::diagnostics::capture_window(&panel.window, ".cache/work-scenes-review.png")?;
            manager.apply_scene(defaults, review, &choices)?;
            let mut expected = crate::tiling::Desktop::configured(Default::default(), None);
            expected.order = choices.iter().flatten().copied().collect();
            if !expected.restore_distribution(&displays[0].distribution) {
                return Err("Sin distribución ordinal para verificar.".into());
            }
            for (id, target) in expected.window_frames(area, 10.) {
                let actual = geometry(id)?;
                if [
                    (actual.origin.x, target.origin.x),
                    (actual.origin.y, target.origin.y),
                    (actual.size.width, target.size.width),
                    (actual.size.height, target.size.height),
                ]
                .iter()
                .any(|(a, b)| (a - b).abs() >= 3.)
                {
                    return Err(format!(
                        "La ventana {id} no ocupa el puesto elegido: {actual:?} ≠ {target:?}"
                    ));
                }
            }
            println!(
                "PASS: escena guardada, revisión con tres ventanas AX de la misma app, rechazo de duplicado y aplicación omitiendo la pantalla ausente."
            );
            Ok(())
        })();
        let _: () = msg_send![&panel.window, orderOut:ptr::null::<AnyObject>()];
        let _: () = msg_send![&prefs, removePersistentDomainForName:&*suite];
        result
    }
}
