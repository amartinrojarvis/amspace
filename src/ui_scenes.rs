//! Manual scene review; selecting a scene or a window never moves anything.
use super::*;
use crate::{
    i18n::tr,
    scene_library,
    tile_manager::scenes::{SceneReview, SceneWindow},
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const SKIP_SLOT: isize = 1;
const FIRST_WINDOW: isize = 2;

struct SceneLaunch {
    deadline: Instant,
    pending: Vec<(String, Receiver<bool>, bool)>,
    failed: Vec<String>,
}

fn stopped_apps(review: &SceneReview) -> impl Iterator<Item = &str> {
    let mut seen = HashSet::new();
    review
        .rows
        .iter()
        .filter(|row| row.available && row.candidates.is_empty())
        .map(|row| row.bundle.as_str())
        .filter(move |&id| seen.insert(id))
        .filter(|id| unsafe {
            let apps: Obj = msg_send![class!(NSRunningApplication),
                runningApplicationsWithBundleIdentifier:&*NSString::from_str(id)];
            let count: usize = msg_send![apps, count];
            count == 0
        })
}

fn launch_app(id: &str) -> Option<Receiver<bool>> {
    unsafe {
        let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        let url: Option<Retained<AnyObject>> = msg_send![workspace,
            URLForApplicationWithBundleIdentifier:&*NSString::from_str(id)];
        let url = url?;
        let config: Retained<AnyObject> =
            msg_send![class!(NSWorkspaceOpenConfiguration), configuration];
        let _: () = msg_send![&config, setActivates:false];
        let (sender, receiver) = mpsc::channel();
        let completion = block2::RcBlock::new(move |app: Obj, error: Obj| {
            let _ = sender.send(!app.is_null() && error.is_null());
        });
        let _: () = msg_send![workspace, openApplicationAtURL: &*url,
            configuration: &*config, completionHandler: &*completion];
        Some(receiver)
    }
}

pub(super) extern "C-unwind" fn launch_progress(target: Obj, _: Sel, _: Obj) {
    with_app_deferred(target, sel!(sceneLaunchProgress:), |app| {
        app.update_scene_launch(target);
    });
}

pub(super) struct Panel {
    pub(super) window: Retained<AnyObject>,
    name: Obj,
    library: Obj,
    delete: Obj,
    scroll: Obj,
    status: Obj,
    apply: Obj,
    open_missing: Obj,
    launch: Option<SceneLaunch>,
    entries: Vec<scene_library::Entry>,
    captured_windows: HashMap<String, Vec<SceneWindow>>,
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
            panel.launch = None;
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
            let scroll = alloc_view(class!(NSScrollView), rect(20., 112., 580., 191.));
            let _: () = msg_send![&scroll, setHasVerticalScroller:true];
            let _: () = msg_send![&scroll, setBorderType:1usize];
            let _: () = msg_send![root, addSubview:&*scroll];
            let status = wrapping_label(root, rect(20., 9., 385., 90.), "", 12.);
            let open_missing = button(
                root,
                rect(413., 67., 188., 32.),
                tr("Abrir apps que faltan"),
                target,
                137,
            );
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
                open_missing,
                launch: None,
                entries: Vec::new(),
                captured_windows: HashMap::new(),
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

    fn save_scene(
        &mut self,
        defaults: Obj,
        manager: &mut crate::tile_manager::Manager,
    ) -> Result<(), String> {
        let value: Retained<NSString> = unsafe { msg_send![self.name, stringValue] };
        let scene = manager.capture_scene(defaults)?;
        let id = scene_library::save(defaults, &value.to_string(), &scene.displays)?;
        self.captured_windows.insert(id.clone(), scene.windows);
        self.reload(defaults, Some(&id));
        unsafe {
            text(self.name, "");
            text(
                self.status,
                tr("Escena guardada. Pulsa Revisar para prepararla."),
            );
        }
        Ok(())
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
                let remembered = self
                    .selected()
                    .and_then(|entry| self.captured_windows.get(&entry.id))
                    .filter(|windows| windows.len() == review.rows.len());
                let mut selected = vec![None; review.rows.len()];
                let mut reserved = HashSet::new();
                if let Some(windows) = remembered {
                    for ((row, window), choice) in
                        review.rows.iter().zip(windows).zip(&mut selected)
                    {
                        *choice = row.candidates.iter().position(|candidate| {
                            window.matches(candidate) && reserved.insert(candidate.id)
                        });
                    }
                }
                let mut occurrences = HashMap::new();
                for (row, choice) in review.rows.iter().zip(&selected) {
                    if choice.is_none() {
                        for candidate in row.candidates.iter().filter(|c| !reserved.contains(&c.id))
                        {
                            *occurrences.entry(candidate.id).or_insert(0usize) += 1;
                        }
                    }
                }
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
                    let items = [first, tr("Omitir puesto")];
                    let items = if row.candidates.is_empty() {
                        &items[..1]
                    } else {
                        &items[..]
                    };
                    let menu = popup(root, rect(236., y + 6., 315., 29.), items, target, 136);
                    let _: () = msg_send![menu, setAccessibilityLabel:&*NSString::from_str(&format!("{heading} · {}",row.bundle))];
                    let native_menu: Obj = msg_send![menu, menu];
                    for candidate in &row.candidates {
                        // NSMenuItem, not addItemWithTitle:, preserves equal window titles.
                        let item: Retained<AnyObject> = msg_send![class!(NSMenuItem), new];
                        let _: () =
                            msg_send![&item,setTitle:&*NSString::from_str(&candidate.label)];
                        let _: () = msg_send![native_menu,addItem:&*item];
                    }
                    let choice = selected[index].or_else(|| {
                        let mut unmatched = row
                            .candidates
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| !reserved.contains(&c.id));
                        let (choice, candidate) = unmatched.next()?;
                        (unmatched.next().is_none() && occurrences.get(&candidate.id) == Some(&1))
                            .then_some(choice)
                    });
                    if let Some(choice) = choice {
                        let _: () =
                            msg_send![menu, selectItemAtIndex:FIRST_WINDOW + choice as isize];
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
        let mut seen = HashSet::new();
        let mut choices = Vec::with_capacity(review.rows.len());
        for (row, menu) in review.rows.iter().zip(&self.controls) {
            if !row.available || row.candidates.is_empty() {
                choices.push(None);
                continue;
            }
            let index: isize = unsafe { msg_send![*menu, indexOfSelectedItem] };
            if index == SKIP_SLOT {
                choices.push(None);
                continue;
            }
            let candidate = row
                .candidates
                .get(usize::try_from(index - FIRST_WINDOW).ok()?)?;
            if !seen.insert(candidate.id) {
                return None;
            }
            choices.push(Some(candidate.id));
        }
        (!seen.is_empty()).then_some(choices)
    }
    fn update_status(&self) {
        let choices = self.choices();
        let ready = choices.is_some() && self.launch.is_none();
        let open = self.launch.is_none()
            && self
                .review
                .as_ref()
                .is_some_and(|review| stopped_apps(review).next().is_some());
        let message = if self.entries.is_empty() {
            tr("Guarda una escena para empezar.")
        } else if self.launch.is_some() {
            tr("Abriendo aplicaciones y esperando sus ventanas…")
        } else if open {
            tr("Faltan aplicaciones. Ábrelas antes de aplicar la escena.")
        } else if choices
            .as_ref()
            .is_some_and(|rows| rows.iter().any(Option::is_none))
        {
            tr("Se aplicarán solo los puestos con ventana asignada.")
        } else if ready {
            tr("Revisa los destinos antes de aplicar la escena.")
        } else if self
            .review
            .as_ref()
            .is_some_and(|review| review.rows.iter().all(|row| row.candidates.is_empty()))
        {
            tr("No hay ventanas compatibles en los mosaicos visibles.")
        } else {
            tr("Asigna ventanas distintas u omite los puestos que no puedas completar.")
        };
        unsafe {
            let _: () = msg_send![self.apply,setEnabled:ready];
            let _: () = msg_send![self.open_missing,setEnabled:open];
            text(self.status, message);
        }
    }
}

impl App {
    fn update_scene_launch(&mut self, target: Obj) {
        let Some(panel) = self.scenes.as_mut() else {
            return;
        };
        let Some(mut launch) = panel.launch.take() else {
            return;
        };
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let review = panel.selected().and_then(|entry| {
            self.tile_manager
                .review_scene(defaults, &entry.displays)
                .ok()
        });
        launch.pending.retain_mut(|(id, response, opened)| {
            if !*opened {
                match response.try_recv() {
                    Ok(true) => *opened = true,
                    Ok(false) | Err(TryRecvError::Disconnected) => {
                        launch.failed.push(id.clone());
                        return false;
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            !*opened
                || !review.as_ref().is_some_and(|review| {
                    review
                        .rows
                        .iter()
                        .any(|row| &row.bundle == id && !row.candidates.is_empty())
                })
        });
        if !launch.pending.is_empty() && Instant::now() < launch.deadline {
            panel.launch = Some(launch);
            schedule_app_callback(target, sel!(sceneLaunchProgress:), 0.4);
            return;
        }
        if let Some(review) = review {
            panel.populate(Some(review), target);
        } else {
            panel.update_status();
        }
        let mut message = String::new();
        if !launch.failed.is_empty() {
            message = format!(
                "{} {}",
                tr("No se pudieron abrir:"),
                launch.failed.join(", ")
            );
        }
        if !launch.pending.is_empty() {
            if !message.is_empty() {
                message.push('\n');
            }
            message.push_str(tr(
                "Si una app no mostró una ventana en mosaico, ábrela y pulsa Revisar.",
            ));
        }
        if !message.is_empty() {
            unsafe { text(panel.status, &message) };
        }
        if panel.review.is_none() && message.is_empty() {
            unsafe {
                text(
                    panel.status,
                    tr("La pantalla o sus ventanas han cambiado. Pulsa Revisar."),
                )
            };
        }
    }

    pub(super) fn scene_action(&mut self, tag: isize, preferred: Option<&str>) {
        let target = Retained::as_ptr(&self.target) as Obj;
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let mut panel = self.scenes.take().unwrap_or_else(|| Panel::new(target));
        if !matches!(tag, 135..=137) {
            panel.launch = None;
        }
        let result = (|| -> Result<(), String> {
            if tag == 130 {
                let selected = panel.selected().map(|e| e.id.clone());
                panel.reload(defaults, preferred.or(selected.as_deref()));
                unsafe {
                    let _: () =
                        msg_send![&panel.window,makeKeyAndOrderFront:ptr::null::<AnyObject>()];
                }
                if let Some(id) = preferred {
                    if panel.selected().is_none_or(|entry| entry.id != id) {
                        panel.populate(None, target);
                        return Err(tr("Esta escena ya no está guardada. Elige otra.").into());
                    }
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
                if let Some(id) = panel.selected().map(|entry| entry.id.clone()) {
                    scene_library::remove(defaults, &id);
                    panel.captured_windows.remove(&id);
                }
                panel.reload(defaults, None);
            }
            panel.reset_delete();
            if tag == 136 {
                panel.update_status();
                return Ok(());
            }
            if !matches!(tag, 135 | 137) {
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
                return panel.save_scene(defaults, &mut self.tile_manager);
            }
            if tag == 137 {
                if panel.launch.is_some() {
                    return Ok(());
                }
                let Some(entry) = panel.selected() else {
                    return Ok(());
                };
                let review = self.tile_manager.review_scene(defaults, &entry.displays)?;
                let mut launch = SceneLaunch {
                    deadline: Instant::now() + Duration::from_secs(10),
                    pending: Vec::new(),
                    failed: Vec::new(),
                };
                for id in stopped_apps(&review) {
                    if let Some(result) = launch_app(id) {
                        launch.pending.push((id.into(), result, false));
                    } else {
                        launch.failed.push(id.into());
                    }
                }
                if launch.pending.is_empty() {
                    panel.populate(Some(review), target);
                    if launch.failed.is_empty() {
                        return Ok(());
                    }
                    return Err(format!(
                        "{} {}",
                        tr("No se pudieron abrir:"),
                        launch.failed.join(", ")
                    ));
                }
                panel.launch = Some(launch);
                panel.update_status();
                schedule_app_callback(target, sel!(sceneLaunchProgress:), 0.4);
                return Ok(());
            }
            if tag == 135 {
                if panel.launch.is_some() {
                    return Ok(());
                }
                if let (Some(review), Some(choices)) = (&panel.review, panel.choices()) {
                    let partial = choices.iter().any(Option::is_none);
                    self.tile_manager.apply_scene(defaults, review, &choices)?;
                    panel.populate(None, target);
                    unsafe {
                        text(
                            panel.status,
                            if partial {
                                tr("Escena aplicada con puestos omitidos.")
                            } else {
                                tr("Escena aplicada.")
                            },
                        );
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
        let controller = controller();
        let target = Retained::as_ptr(&controller) as Obj;
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
            let mut displays = manager.capture_scene(defaults)?.displays;
            if displays.len() != 1 || displays[0].bundles.len() != ids.len() {
                return Err("La captura no coincide con las ventanas desechables.".into());
            }
            let offline = crate::tile_manager::scenes::SceneDisplay {
                name: "Monitor desconectado".into(),
                uuid: "00000000-0000-0000-0000-000000000000".into(),
                distribution: displays[0].distribution.clone(),
                bundles: displays[0].bundles.clone(),
            };
            scene_library::save(defaults, "Antes", &displays)?;
            displays.push(offline);
            let saved = scene_library::save(defaults, "Estudio", &displays)?;
            let mut desktop = crate::scene_library::ordinal_desktop(&displays[0])
                .ok_or("No se pudo preparar la galería del fixture.")?;
            desktop.order = ids.to_vec();
            let context = crate::tile_manager::PickerContext {
                workspace: (0, 0),
                area,
                desktop,
            };
            let mut app = App::build(controller.clone(), prefs.clone());
            app.layout_picker = Some(
                crate::tile_picker::Picker::new(context, target, defaults)
                    .ok_or("No se pudo mostrar el selector de plantillas.")?,
            );
            app.layout_picker.as_ref().unwrap().show();
            super::diagnostics::pump(0.06);
            let native_app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let gallery: Option<Retained<AnyObject>> = msg_send![native_app, keyWindow];
            let gallery = gallery.ok_or("El selector no recibió su ventana nativa.")?;
            let root: Obj = msg_send![&gallery, contentView];
            let tag = (crate::tiling::Layout::ALL.len() + 1) as isize;
            let card: Option<Retained<AnyObject>> = msg_send![root, viewWithTag:tag];
            let card = card.ok_or("Falta la tarjeta gráfica «Estudio».")?;
            let caption: Retained<NSString> = msg_send![&card, accessibilityLabel];
            if !caption.to_string().contains("Estudio") {
                return Err("La tarjeta de la escena no tiene su nombre accesible.".into());
            }
            super::diagnostics::capture_window(&gallery, ".cache/work-scenes-picker.png")?;
            std::mem::swap(&mut app.tile_manager, &mut manager);
            APP.with(|cell| *cell.borrow_mut() = Some(app));
            let _: () = msg_send![&card, performClick:ptr::null::<AnyObject>()];
            let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
            std::mem::swap(&mut app.tile_manager, &mut manager);
            let visible: bool = msg_send![&gallery, isVisible];
            if visible || app.layout_picker.is_some() {
                return Err("Elegir una escena no cerró el selector de plantillas.".into());
            }
            panel = app
                .scenes
                .take()
                .ok_or("No se abrió el panel de escenas.")?;
            let visible: bool = msg_send![&panel.window, isVisible];
            if !visible
                || panel.review.is_none()
                || panel.selected().is_none_or(|entry| entry.id != saved)
            {
                return Err("La galería no abrió la revisión de la escena elegida.".into());
            }
            println!("PASS: la tarjeta gráfica abre la revisión de la escena elegida.");
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
                let _: () = msg_send![*control, selectItemAtIndex:FIRST_WINDOW + index as isize];
                if occupied == 0 && panel.choices().is_some() {
                    return Err("Un puesto pendiente se omitió sin confirmación.".into());
                }
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
            let mut partial = displays[0].clone();
            partial
                .bundles
                .insert(1, "no.amspace.fixture.missing".into());
            // Keep an asymmetric root and let the missing leaf share a split
            // with the third slot. Removing it must not reset the root ratio.
            partial.distribution = "2 1 4 0 0 v0.37 w0 h0.41 v0.47 w1 w2 w3".into();
            let mut expected = scene_library::ordinal_desktop(&partial)
                .ok_or("Distribución parcial de prueba inválida.")?;
            expected.remove(2);
            let partial_id = scene_library::save(defaults, "Parcial", &[partial])?;
            panel.reload(defaults, Some(&partial_id));
            let entry = panel.selected().ok_or("Falta la escena parcial.")?;
            let review = manager.review_scene(defaults, &entry.displays)?;
            panel.populate(Some(review), target);
            let review = panel.review.as_ref().unwrap();
            if !review.rows[1].available || !review.rows[1].candidates.is_empty() {
                return Err(
                    "El puesto sin ventana no se distinguió de la pantalla ausente.".into(),
                );
            }
            for (&id, index) in ids.iter().zip([0, 2, 3]) {
                let item = review.rows[index]
                    .candidates
                    .iter()
                    .position(|c| c.id == id)
                    .ok_or("Falta una ventana abierta en la revisión parcial.")?;
                let _: () = msg_send![panel.controls[index], selectItemAtIndex:FIRST_WINDOW + item as isize];
            }
            panel.update_status();
            let enabled: bool = msg_send![panel.apply, isEnabled];
            let choices = panel
                .choices()
                .filter(|choices| enabled && choices[1].is_none())
                .ok_or("La ventana ausente bloqueó la aplicación parcial.")?;
            manager.apply_scene(defaults, review, &choices)?;
            for (ordinal, target) in expected.window_frames(area, 10.) {
                let id = choices[ordinal as usize - 1].unwrap();
                let actual = geometry(id)?;
                if [
                    actual.origin.x - target.origin.x,
                    actual.origin.y - target.origin.y,
                    actual.size.width - target.size.width,
                    actual.size.height - target.size.height,
                ]
                .iter()
                .any(|delta| delta.abs() >= 3.)
                {
                    return Err(format!(
                        "La escena parcial no recolocó {id}: {actual:?} ≠ {target:?}"
                    ));
                }
            }
            println!("PASS: la escena parcial recoloca tres ventanas y contrae el puesto ausente.");
            // Optional launch fixture: a separate, uniquely registered test app.
            // This exercises the real button and Launch Services without
            // starting or closing any of the user's applications.
            if let Ok(bundle) = std::env::var("AMSPACE_SCENE_LAUNCH_FIXTURE") {
                if !bundle.starts_with("io.github.amartinrojarvis.amspace.fixture.") {
                    return Err("El bundle de lanzamiento debe ser un fixture desechable.".into());
                }
                let mut launch_scene = displays[0].clone();
                launch_scene.bundles.fill(bundle);
                let id = scene_library::save(defaults, "Apertura", &[launch_scene])?;
                let mut app = App::build(controller.clone(), prefs.clone());
                std::mem::swap(&mut app.tile_manager, &mut manager);
                app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
                app.scene_action(130, Some(&id));
                let panel_ref = app.scenes.as_ref().unwrap();
                let enabled: bool = msg_send![panel_ref.open_missing, isEnabled];
                if !enabled {
                    return Err("No se ofreció abrir la app cerrada.".into());
                }
                let open_missing = panel_ref.open_missing;
                APP.with(|cell| *cell.borrow_mut() = Some(app));
                let _: () = msg_send![open_missing, performClick:ptr::null::<AnyObject>()];
                let end = Instant::now() + Duration::from_secs(6);
                let opened = loop {
                    super::diagnostics::pump(0.1);
                    if APP.with(|cell| {
                        let app = cell.borrow();
                        app.as_ref()
                            .and_then(|app| app.scenes.as_ref()?.launch.as_ref())
                            .is_some_and(|launch| {
                                matches!(launch.pending.as_slice(), [(_, _, true)])
                            })
                    }) {
                        break true;
                    }
                    if Instant::now() >= end {
                        break false;
                    }
                };
                let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
                std::mem::swap(&mut app.tile_manager, &mut manager);
                panel = app.scenes.take().unwrap();
                panel.launch = None;
                if !opened {
                    return Err("Launch Services no confirmó la apertura del fixture.".into());
                }
                println!(
                    "PASS: el botón abre una aplicación cerrada una sola vez para tres puestos."
                );
            }

            let saved_frames = ids
                .iter()
                .copied()
                .map(geometry)
                .collect::<Result<Vec<_>, _>>()?;
            text(panel.name, "Ida y vuelta");
            panel.save_scene(defaults, &mut manager)?;
            let saved = panel.selected().ok_or("Falta la escena de ida y vuelta.")?;
            let original = manager.review_scene(defaults, &saved.displays)?;
            let reordered: Vec<_> = ids
                .iter()
                .cycle()
                .skip(1)
                .take(ids.len())
                .copied()
                .map(Some)
                .collect();
            manager.apply_scene(defaults, &original, &reordered)?;
            if geometry(ids[0])? == saved_frames[0] {
                return Err("La distribución intermedia no cambió la posición de prueba.".into());
            }
            let review = manager.review_scene(defaults, &saved.displays)?;
            panel.populate(Some(review), target);
            let choices = panel
                .choices()
                .filter(|choices| choices.iter().copied().eq(ids.iter().copied().map(Some)))
                .ok_or("La revisión no recuperó la identidad de cada ventana guardada.")?;
            manager.apply_scene(defaults, panel.review.as_ref().unwrap(), &choices)?;
            for (&id, frame) in ids.iter().zip(saved_frames) {
                let actual = geometry(id)?;
                if actual != frame {
                    return Err(format!(
                        "La escena no recuperó el marco guardado de {id}: {actual:?} ≠ {frame:?}"
                    ));
                }
            }
            println!(
                "PASS: guardar y recuperar restaura la posición de cada ventana de la misma app tras intercambiar sus puestos."
            );
            let settled = ids
                .iter()
                .copied()
                .map(geometry)
                .collect::<Result<Vec<_>, _>>()?;
            // Topology notification arms the placement barrier without changing
            // the live displays of this fixture.
            manager.screens_changed();
            panel.populate(None, target);
            text(panel.name, "Durante la reentrada");
            panel.save_scene(defaults, &mut manager)?;
            let status: Retained<NSString> = msg_send![panel.status, stringValue];
            if status.to_string() != tr("Escena guardada. Pulsa Revisar para prepararla.")
                || panel.selected().map(|entry| entry.name.as_str()) != Some("Durante la reentrada")
            {
                return Err("El panel no confirmó ni seleccionó la escena recién guardada.".into());
            }
            let pending = manager.review_scene(
                defaults,
                &panel
                    .selected()
                    .ok_or("Escena no guardada durante la reentrada.")?
                    .displays,
            )?;
            let selection: Vec<_> = ids.iter().rev().copied().map(Some).collect();
            if manager
                .apply_scene(defaults, &pending, &selection)
                .err()
                .as_deref()
                != Some(tr("Espera a que termine el cambio de escritorio."))
            {
                return Err("Aplicar no respetó la barrera de colocación.".into());
            }
            for (&id, frame) in ids.iter().zip(settled) {
                if geometry(id)? != frame {
                    return Err("Guardar o revisar durante la reentrada movió una ventana.".into());
                }
            }
            println!(
                "PASS: guardado y revisión durante reentrada, aplicación bloqueada sin mover ventanas."
            );
            Ok(())
        })();
        let _: () = msg_send![&panel.window, orderOut:ptr::null::<AnyObject>()];
        let _: () = msg_send![&prefs, removePersistentDomainForName:&*suite];
        result
    }
}
