//! Quick scene restoration and optional manual window assignment.
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
pub(super) const OPEN_MISSING: &str = "tile.scenes.openMissing";

pub(super) fn open_missing(defaults: Obj) -> bool {
    crate::tile_manager::pref_or(defaults, OPEN_MISSING, 1) == 1
}

pub(super) struct Restore {
    entry: scene_library::Entry,
    phase: RestorePhase,
}

enum RestorePhase {
    Settling {
        deadline: Instant,
        desktops: Vec<(u32, u64)>,
        screens: Vec<crate::native::ScreenGeometry>,
    },
    Ask(SceneReview, Vec<(String, Retained<AnyObject>)>),
    Waiting(SceneReview, SceneLaunch),
}

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

fn installed_app(id: &str) -> Option<Retained<AnyObject>> {
    unsafe {
        let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        msg_send![workspace, URLForApplicationWithBundleIdentifier:&*NSString::from_str(id)]
    }
}

fn launch_app(url: &AnyObject) -> Receiver<bool> {
    unsafe {
        let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        let config: Retained<AnyObject> =
            msg_send![class!(NSWorkspaceOpenConfiguration), configuration];
        let _: () = msg_send![&config, setActivates:false];
        let (sender, receiver) = mpsc::channel();
        let completion = block2::RcBlock::new(move |app: Obj, error: Obj| {
            let _ = sender.send(!app.is_null() && error.is_null());
        });
        let _: () = msg_send![workspace, openApplicationAtURL: url,
            configuration: &*config, completionHandler: &*completion];
        receiver
    }
}

pub(super) extern "C-unwind" fn launch_progress(target: Obj, _: Sel, _: Obj) {
    let mut prompt = None;
    with_app_deferred(target, sel!(sceneLaunchProgress:), |app| {
        let Some(restore) = app.scene_restore.take() else {
            return;
        };
        match restore.phase {
            RestorePhase::Settling {
                deadline,
                desktops,
                screens,
            } => {
                let defaults = Retained::as_ptr(&app.defaults) as Obj;
                let result = app.scene_access(defaults).and_then(|()| {
                    if scene_desktops() != desktops
                        || crate::native::screen_configuration() != screens
                    {
                        return Err(tr(
                            "La pantalla o el escritorio han cambiado. Vuelve a cargar la escena.",
                        )
                        .into());
                    }
                    if Instant::now() >= deadline {
                        return Err(tr("Espera a que termine el cambio de escritorio.").into());
                    }
                    if app.tile_manager.scene_waiting_for_space() {
                        app.queue_scene(
                            restore.entry,
                            RestorePhase::Settling {
                                deadline,
                                desktops,
                                screens,
                            },
                            0.125,
                        );
                        return Ok(());
                    }
                    app.begin_scene(defaults, restore.entry)
                });
                if let Err(message) = result {
                    app.show_notice(&message);
                }
            }
            RestorePhase::Ask(origin, apps) => {
                app.scene_prompt_open = true;
                prompt = Some((restore.entry, origin, apps));
            }
            RestorePhase::Waiting(origin, launch) => {
                app.update_scene_launch(restore.entry, origin, launch);
            }
        }
    });
    let Some((entry, origin, apps)) = prompt else {
        return;
    };
    // The native modal runs its own loop. Do not hold the App borrow while it is up.
    let open = unsafe {
        let alert: Retained<AnyObject> = msg_send![class!(NSAlert), new];
        let _: () =
            msg_send![&alert, setMessageText:&*NSString::from_str(tr("¿Abrir apps faltantes?"))];
        let _: () = msg_send![&alert, setInformativeText:&*NSString::from_str(tr(
            "Puedes abrirlas o cargar la escena solo con las ventanas disponibles."
        ))];
        let _: Obj =
            msg_send![&alert, addButtonWithTitle:&*NSString::from_str(tr("Cargar sin abrir"))];
        let _: Obj =
            msg_send![&alert, addButtonWithTitle:&*NSString::from_str(tr("Abrir y cargar"))];
        let response: isize = msg_send![&alert, runModal];
        response == 1001
    };
    with_app(|app| {
        app.scene_prompt_open = false;
        let result = app.resume_scene(entry, &origin, open, apps);
        if let Err(message) = result {
            app.show_notice(&message);
        }
    });
}

fn scene_desktops() -> Vec<(u32, u64)> {
    let mut desktops = crate::spaces::visible_desktops();
    desktops.sort_unstable();
    desktops
}

pub(super) struct Panel {
    pub(super) window: Retained<AnyObject>,
    name: Obj,
    library: Obj,
    delete: Obj,
    scroll: Obj,
    status: Obj,
    apply: Obj,
    load: Obj,
    entries: Vec<scene_library::Entry>,
    captured_windows: HashMap<String, Vec<SceneWindow>>,
    review: Option<SceneReview>,
    controls: Vec<Obj>,
    confirming: bool,
}

fn remembered_slots(
    review: &SceneReview,
    remembered: Option<&[SceneWindow]>,
) -> Vec<Option<usize>> {
    let mut used = HashSet::new();
    review
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let window = remembered?.get(index)?;
            row.candidates
                .iter()
                .position(|c| window.matches(c) && used.insert(c.id))
        })
        .collect()
}

pub(crate) fn quick_choices(
    review: &SceneReview,
    remembered: Option<&[SceneWindow]>,
) -> Vec<Option<u32>> {
    let mut choices = remembered_slots(review, remembered);
    let mut used: HashSet<_> = review
        .rows
        .iter()
        .zip(&choices)
        .filter_map(|(row, choice)| choice.map(|i| row.candidates[i].id))
        .collect();
    // Reserve windows already on their saved monitor before filling gaps
    // elsewhere: an early missing slot must not steal a later local match.
    for local_only in [true, false] {
        for (row, choice) in review.rows.iter().zip(&mut choices) {
            if choice.is_none() {
                let candidates = if local_only {
                    &row.candidates[..row.local_candidates]
                } else {
                    &row.candidates
                };
                *choice = candidates.iter().position(|c| used.insert(c.id));
            }
        }
    }
    review
        .rows
        .iter()
        .zip(choices)
        .map(|(row, choice)| choice.map(|i| row.candidates[i].id))
        .collect()
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
                    "Guarda el mosaico visible de cada pantalla y recupéralo desde el selector de plantillas.",
                ),
                13.,
                0.,
                true,
            );
            let name = alloc_view(class!(NSTextField), rect(20., 365., 395., 26.));
            let placeholder = NSString::from_str(tr("Nombre de la nueva escena"));
            let _: () = msg_send![&name, setPlaceholderString:&*placeholder];
            let _: () = msg_send![&name, setAccessibilityLabel:&*placeholder];
            let _: () = msg_send![&name, setTarget:target];
            let _: () = msg_send![&name, setAction:sel!(change:)];
            let _: () = msg_send![&name, setTag:131isize];
            let _: () = msg_send![root, addSubview:&*name];
            button(
                root,
                rect(421., 362., 179., 31.),
                tr("Guardar escena actual"),
                target,
                131,
            );
            let library = popup(root, rect(17., 315., 296., 28.), &[], target, 132);
            let _: () = msg_send![library, setAccessibilityLabel:&*NSString::from_str(tr("Escenas guardadas"))];
            let load = button(root, rect(311., 314., 92., 30.), tr("Cargar"), target, 138);
            button(
                root,
                rect(401., 314., 100., 30.),
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
                load,
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
            let _: () = msg_send![self.load, setEnabled:!self.entries.is_empty()];
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
                tr("Escena guardada. Está disponible en el selector de plantillas."),
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
                let selected = remembered_slots(
                    review,
                    self.selected()
                        .and_then(|entry| self.captured_windows.get(&entry.id))
                        .map(Vec::as_slice),
                );
                let reserved: HashSet<_> = review
                    .rows
                    .iter()
                    .zip(&selected)
                    .filter_map(|(row, choice)| choice.map(|i| row.candidates[i].id))
                    .collect();
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
                        tr("Pantalla sin mosaico disponible")
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
        let ready = choices.is_some();
        let message = if self.entries.is_empty() {
            tr("Guarda una escena para empezar.")
        } else if self.review.is_none() {
            tr("Selecciona una escena para cargarla o pulsa Revisar para elegir ventanas.")
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
            text(self.status, message);
        }
    }
}

impl App {
    fn scene_access(&self, defaults: Obj) -> Result<(), String> {
        if self.sleeping
            || self.tiling_paused
            || crate::desktop_mode::active()
            || self.transfer.is_some()
            || self.switching.is_some()
            || crate::tile_manager::pref(defaults, "tile.enabled") == 0
        {
            return Err(tr("Activa el mosaico y espera a que termine la transición.").into());
        }
        Ok(())
    }

    pub(super) fn restore_scene(&mut self, id: &str) {
        if self.scene_prompt_open {
            return;
        }
        self.scene_restore = None;
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let result = (|| {
            self.scene_access(defaults)?;
            let entry = scene_library::entry(defaults, id)
                .ok_or_else(|| tr("Esta escena ya no está guardada. Elige otra.").to_owned())?;
            if self.tile_manager.scene_waiting_for_space() {
                self.queue_scene(
                    entry,
                    RestorePhase::Settling {
                        deadline: Instant::now() + Duration::from_secs(3),
                        desktops: scene_desktops(),
                        screens: crate::native::screen_configuration(),
                    },
                    0.125,
                );
                return Ok(());
            }
            self.begin_scene(defaults, entry)
        })();
        if let Err(message) = result {
            self.show_notice(&message);
        }
    }

    fn begin_scene(&mut self, defaults: Obj, entry: scene_library::Entry) -> Result<(), String> {
        let review = self.tile_manager.review_scene(defaults, &entry.displays)?;
        if open_missing(defaults) {
            let apps: Vec<_> = stopped_apps(&review)
                .filter_map(|id| Some((id.to_owned(), installed_app(id)?)))
                .collect();
            if !apps.is_empty() {
                self.queue_scene(entry, RestorePhase::Ask(review, apps), 0.);
                return Ok(());
            }
        }
        self.apply_quick_scene(defaults, &entry.id, &review)
    }

    fn queue_scene(&mut self, entry: scene_library::Entry, phase: RestorePhase, delay: f64) {
        self.scene_restore = Some(Restore { entry, phase });
        schedule_app_callback(
            Retained::as_ptr(&self.target) as Obj,
            sel!(sceneLaunchProgress:),
            delay,
        );
    }

    fn scene_after_wait(
        &mut self,
        defaults: Obj,
        entry: &scene_library::Entry,
        origin: &SceneReview,
    ) -> Result<SceneReview, String> {
        self.scene_access(defaults)?;
        let review = self.tile_manager.review_scene(defaults, &entry.displays)?;
        if !origin.same_destinations(&review) {
            return Err(
                tr("La pantalla o el escritorio han cambiado. Vuelve a cargar la escena.").into(),
            );
        }
        Ok(review)
    }

    fn resume_scene(
        &mut self,
        entry: scene_library::Entry,
        origin: &SceneReview,
        open: bool,
        apps: Vec<(String, Retained<AnyObject>)>,
    ) -> Result<(), String> {
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let review = self.scene_after_wait(defaults, &entry, origin)?;
        if open && open_missing(defaults) {
            let stopped: HashSet<_> = stopped_apps(&review).collect();
            let pending: Vec<_> = apps
                .into_iter()
                .filter(|(id, _)| stopped.contains(id.as_str()))
                .map(|(id, url)| (id, launch_app(&url), false))
                .collect();
            if !pending.is_empty() {
                self.show_notice(tr("Abriendo aplicaciones y esperando sus ventanas…"));
                self.queue_scene(
                    entry,
                    RestorePhase::Waiting(
                        review,
                        SceneLaunch {
                            deadline: Instant::now() + Duration::from_secs(10),
                            pending,
                            failed: Vec::new(),
                        },
                    ),
                    0.25,
                );
                return Ok(());
            }
        }
        self.apply_quick_scene(defaults, &entry.id, &review)
    }

    fn update_scene_launch(
        &mut self,
        entry: scene_library::Entry,
        origin: SceneReview,
        mut launch: SceneLaunch,
    ) {
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let review = match self.scene_after_wait(defaults, &entry, &origin) {
            Ok(review) => review,
            Err(message) => {
                self.show_notice(&message);
                return;
            }
        };
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
                || !review
                    .rows
                    .iter()
                    .any(|row| &row.bundle == id && !row.candidates.is_empty())
        });
        if !launch.pending.is_empty() && Instant::now() < launch.deadline {
            self.queue_scene(entry, RestorePhase::Waiting(origin, launch), 0.25);
            return;
        }
        if let Err(message) = self.apply_quick_scene(defaults, &entry.id, &review) {
            self.show_notice(&message);
        } else if !launch.failed.is_empty() {
            self.show_notice(&format!(
                "{} {}",
                tr("No se pudieron abrir:"),
                launch.failed.join(", ")
            ));
        }
    }

    fn apply_quick_scene(
        &mut self,
        defaults: Obj,
        id: &str,
        review: &SceneReview,
    ) -> Result<(), String> {
        let remembered = self
            .scenes
            .as_ref()
            .and_then(|panel| panel.captured_windows.get(id))
            .map(Vec::as_slice);
        let choices = quick_choices(review, remembered);
        let applied = choices.iter().flatten().count();
        if applied == 0 {
            return Err(tr("No hay ventanas compatibles en los mosaicos visibles.").into());
        }
        self.tile_manager.apply_scene(defaults, review, &choices)?;
        self.refresh_tile_controls();
        let partial;
        let message = if applied == choices.len() {
            tr("Escena aplicada.")
        } else {
            partial = crate::tr_format!(
                "Escena aplicada: {} de {} puestos.",
                "Scene applied: {} of {} slots.",
                applied,
                choices.len()
            );
            &partial
        };
        if !self
            .scenes
            .as_ref()
            .is_some_and(|panel| unsafe { msg_send![&*panel.window, isVisible] })
        {
            self.show_notice(message);
        }
        if let Some(panel) = &mut self.scenes {
            let target = Retained::as_ptr(&self.target) as Obj;
            panel.populate(None, target);
            unsafe { text(panel.status, message) };
        }
        Ok(())
    }

    pub(super) fn scene_action(&mut self, tag: isize) {
        let target = Retained::as_ptr(&self.target) as Obj;
        let defaults = Retained::as_ptr(&self.defaults) as Obj;
        let mut panel = self.scenes.take().unwrap_or_else(|| Panel::new(target));
        if matches!(tag, 132 | 138) {
            let id = panel.selected().map(|entry| entry.id.clone());
            panel.populate(None, target);
            self.scenes = Some(panel);
            if let Some(id) = id {
                self.restore_scene(&id);
            }
            return;
        }
        if matches!(tag, 131 | 133..=135) {
            self.scene_restore = None;
        }
        let result = (|| -> Result<(), String> {
            if tag == 130 {
                let selected = panel.selected().map(|e| e.id.clone());
                panel.reload(defaults, selected.as_deref());
                unsafe {
                    let _: () =
                        msg_send![&panel.window,makeKeyAndOrderFront:ptr::null::<AnyObject>()];
                }
                panel.populate(None, target);
                return Ok(());
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
                panel.populate(None, target);
                return Ok(());
            }
            panel.reset_delete();
            if tag == 136 {
                panel.update_status();
                return Ok(());
            }
            if tag != 135 {
                panel.populate(None, target);
            }
            self.scene_access(defaults)?;
            if tag == 131 {
                return panel.save_scene(defaults, &mut self.tile_manager);
            }
            if tag == 135 {
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
            let capture = manager
                .capture_scene(defaults)
                .map_err(|e| format!("Captura inicial: {e}"))?;
            let mut displays = capture.displays;
            if displays.len() != 1 || displays[0].bundles.len() != ids.len() {
                return Err("La captura no coincide con las ventanas desechables.".into());
            }
            let initial = manager
                .review_scene(defaults, &displays)
                .map_err(|e| format!("Revisión inicial: {e}"))?;
            let choices: Vec<_> = ids.iter().copied().map(Some).collect();
            manager
                .apply_scene(defaults, &initial, &choices)
                .map_err(|e| format!("Colocación inicial: {e}"))?;
            let before = ids
                .iter()
                .copied()
                .map(geometry)
                .collect::<Result<Vec<_>, _>>()?;
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
            let initial = manager
                .review_scene(defaults, &displays)
                .map_err(|e| format!("Preparación de la tarjeta: {e}"))?;
            let reordered: Vec<_> = ids
                .iter()
                .rev()
                .copied()
                .map(Some)
                .chain(std::iter::repeat_n(None, ids.len()))
                .collect();
            manager
                .apply_scene(defaults, &initial, &reordered)
                .map_err(|e| format!("Permutación del fixture: {e}"))?;
            if geometry(ids[0])? == before[0] {
                return Err("La preparación no permutó las ventanas del fixture.".into());
            }
            panel
                .captured_windows
                .insert(saved.clone(), capture.windows);
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
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
            panel = app.scenes.take().unwrap();
            let visible: bool = msg_send![&panel.window, isVisible];
            if visible || panel.review.is_some() || app.scene_restore.is_some() {
                return Err("La tarjeta abrió una revisión o dejó una carga pendiente.".into());
            }
            for (&id, &frame) in ids.iter().zip(&before) {
                if geometry(id)? != frame {
                    return Err("La tarjeta no recuperó por sí sola la geometría guardada.".into());
                }
            }
            println!(
                "PASS: una tarjeta carga la escena directamente, sin revisión ni confirmación."
            );
            panel.captured_windows.remove(&saved);
            panel.reload(defaults, Some(&saved));
            panel.populate(Some(manager.review_scene(defaults, &displays)?), target);
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
            panel
                .choices()
                .filter(|choices| enabled && choices[1].is_none())
                .ok_or("La ventana ausente bloqueó la revisión parcial.")?;
            let choices = quick_choices(review, None);
            let mut app = App::build(controller.clone(), prefs.clone());
            let load = panel.load;
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            APP.with(|cell| *cell.borrow_mut() = Some(app));
            let _: () = msg_send![load, performClick:ptr::null::<AnyObject>()];
            let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            if app.scene_restore.is_some() {
                return Err("Una app no instalada generó una solicitud de apertura.".into());
            }
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
            // A registered disposable app exercises the native prompt and Launch Services.
            if let Ok(bundle) = std::env::var("AMSPACE_SCENE_LAUNCH_FIXTURE") {
                if !bundle.starts_with("io.github.amartinrojarvis.amspace.fixture.") {
                    return Err("El bundle de lanzamiento debe ser un fixture desechable.".into());
                }
                let mut launch_scene = displays[0].clone();
                launch_scene.bundles.fill(bundle.clone());
                let id = scene_library::save(defaults, "Apertura", &[launch_scene])?;
                let mut app = App::build(controller.clone(), prefs.clone());
                std::mem::swap(&mut app.tile_manager, &mut manager);
                let entry = scene_library::entry(defaults, &id).unwrap();
                let review = app.tile_manager.review_scene(defaults, &entry.displays)?;
                if stopped_apps(&review).collect::<Vec<_>>() != [bundle.as_str()]
                    || installed_app(&bundle).is_none()
                {
                    return Err("El fixture de lanzamiento debe estar instalado y cerrado.".into());
                }
                crate::tile_manager::save(defaults, OPEN_MISSING, 0);
                app.restore_scene(&id);
                if app.scene_restore.is_some() || stopped_apps(&review).next().is_none() {
                    return Err("La opción desactivada solicitó abrir una app cerrada.".into());
                }
                let _: () =
                    msg_send![&prefs, removeObjectForKey:&*NSString::from_str(OPEN_MISSING)];
                APP.with(|cell| *cell.borrow_mut() = Some(app));
                let answer = |response: isize| {
                    with_app(|app| app.restore_scene(&id));
                    let answered = std::rc::Rc::new(std::cell::Cell::new(false));
                    let flag = answered.clone();
                    let complete = block2::RcBlock::new(move |_: Obj| {
                        let modal: Obj = msg_send![native_app, modalWindow];
                        if !modal.is_null() {
                            flag.set(true);
                            let _: () = msg_send![native_app, stopModalWithCode:response];
                        }
                    });
                    let timer: Retained<AnyObject> = msg_send![class!(NSTimer),
                        timerWithTimeInterval:0.1f64, repeats:false, block:&*complete];
                    let run_loop: Obj = msg_send![class!(NSRunLoop), mainRunLoop];
                    let _: () = msg_send![run_loop, addTimer:&*timer, forMode:ns_string!("NSModalPanelRunLoopMode")];
                    super::diagnostics::pump(0.15);
                    answered.get()
                };
                let declined = answer(1000) && stopped_apps(&review).next().is_some();
                let accepted = answer(1001);
                let end = Instant::now() + Duration::from_secs(6);
                let opened = loop {
                    if APP.with(|cell| {
                        let app = cell.borrow();
                        matches!(
                            app.as_ref().and_then(|app| app.scene_restore.as_ref()).map(|r| &r.phase),
                            Some(RestorePhase::Waiting(_, launch)) if matches!(launch.pending.as_slice(), [(_, _, true)])
                        )
                    }) {
                        break true;
                    }
                    if Instant::now() >= end {
                        break false;
                    }
                    super::diagnostics::pump(0.1);
                };
                let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
                std::mem::swap(&mut app.tile_manager, &mut manager);
                if !open_missing(defaults) {
                    return Err("La apertura de apps no está habilitada por defecto.".into());
                }
                if !declined || !accepted || !opened {
                    return Err(
                        "El aviso nativo no respetó el rechazo o la apertura confirmada.".into(),
                    );
                }
                println!(
                    "PASS: la opción desactivada no abre; por defecto pregunta; confirmar abre una sola app para tres puestos."
                );
            }

            let saved_frames = ids
                .iter()
                .copied()
                .map(geometry)
                .collect::<Result<Vec<_>, _>>()?;
            text(panel.name, "Ida y vuelta");
            let name = panel.name;
            let mut app = App::build(controller.clone(), prefs.clone());
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            APP.with(|cell| *cell.borrow_mut() = Some(app));
            let sent: bool = msg_send![name, sendAction:sel!(change:), to:target];
            let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            let saved = panel
                .selected()
                .filter(|entry| sent && entry.name == "Ida y vuelta")
                .ok_or("La acción nativa no guardó ni seleccionó la escena de ida y vuelta.")?;
            let original = manager.review_scene(defaults, &saved.displays)?;
            let mut reordered = quick_choices(
                &original,
                panel.captured_windows.get(&saved.id).map(Vec::as_slice),
            );
            let extra = reordered[2].ok_or("Falta el tercer miembro del mosaico inicial.")?;
            reordered.rotate_left(1);
            manager.apply_scene(defaults, &original, &reordered)?;
            if geometry(ids[0])? == saved_frames[0] {
                return Err("La distribución intermedia no cambió la posición de prueba.".into());
            }
            let saved_id = saved.id.clone();
            let mut subset = saved.displays[0].clone();
            let mut app = App::build(controller.clone(), prefs.clone());
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            app.restore_scene(&saved_id);
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            for (&id, &frame) in ids.iter().zip(&saved_frames) {
                if geometry(id)? != frame {
                    return Err("La escena no recuperó la distribución guardada.".into());
                }
            }
            // Offsets inside the general layout tolerance must still be fixed.
            let window = ax::window_for_id(fixture_pid, ids[0]).ok_or("Sin ventana AX")?;
            let frame = saved_frames[0];
            let drifted = rect(
                frame.origin.x + 1.,
                frame.origin.y + 1.,
                frame.size.width - 2.,
                frame.size.height - 2.,
            );
            if !ax::set_frame(window.0, drifted) || geometry(ids[0])? != drifted {
                return Err("No se pudo preparar la deriva de posición y tamaño.".into());
            }
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            app.restore_scene(&saved_id);
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            for (&id, frame) in ids.iter().zip(saved_frames) {
                let actual = geometry(id)?;
                if actual != frame {
                    return Err(format!(
                        "La escena no recuperó el marco guardado de {id}: {actual:?} ≠ {frame:?}"
                    ));
                }
            }
            println!(
                "PASS: la escena recupera posición y tamaño exactos, incluso con uno o dos puntos de deriva."
            );
            subset.bundles.pop();
            subset.distribution = "2 1 2 0 0 v0.37 w0 w1".into();
            let subset_id = scene_library::save(defaults, "Dos puestos", &[subset])?;
            let extra_frame = geometry(extra)?;
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            app.restore_scene(&subset_id);
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            if geometry(extra)? != extra_frame
                || manager.capture_scene(defaults)?.windows.len() != 2
            {
                return Err(
                    "La escena no dejó la ventana ajena flotante en su posición original.".into(),
                );
            }
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            app.restore_scene(&saved_id);
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            if manager.capture_scene(defaults)?.windows.len() != ids.len() {
                return Err("La escena completa no recuperó la ventana flotante.".into());
            }
            println!("PASS: la ventana ajena no se mueve y otra escena recupera esa flotante.");
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
            if status.to_string()
                != tr("Escena guardada. Está disponible en el selector de plantillas.")
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
            app.scenes = Some(std::mem::replace(&mut panel, Panel::new(target)));
            std::mem::swap(&mut app.tile_manager, &mut manager);
            app.restore_scene(&subset_id);
            APP.with(|cell| *cell.borrow_mut() = Some(app));
            super::diagnostics::pump(0.2);
            let pending = APP.with(|cell| {
                let app = cell.borrow();
                matches!(
                    app.as_ref()
                        .and_then(|a| a.scene_restore.as_ref())
                        .map(|r| &r.phase),
                    Some(RestorePhase::Settling { .. })
                )
            });
            if !pending {
                return Err("La carga rápida se perdió durante la reentrada.".into());
            }
            crate::tile_manager::save(defaults, "tile.enabled", 0);
            super::diagnostics::pump(0.2);
            let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
            if app.scene_restore.is_some() {
                return Err("Desactivar el mosaico no canceló la escena pendiente.".into());
            }
            for (&id, frame) in ids.iter().zip(settled) {
                if geometry(id)? != frame {
                    return Err("Guardar o revisar durante la reentrada movió una ventana.".into());
                }
            }
            crate::tile_manager::save(defaults, "tile.enabled", 1);
            app.restore_scene(&subset_id);
            APP.with(|cell| *cell.borrow_mut() = Some(app));
            let end = Instant::now() + Duration::from_secs(4);
            while Instant::now() < end {
                with_app(|a| {
                    crate::tile_manager::close_diagnostics::observe_scene_reentry(
                        &mut a.tile_manager,
                        defaults,
                    );
                });
                super::diagnostics::pump(0.09);
                if APP.with(|cell| cell.borrow().as_ref().unwrap().scene_restore.is_none()) {
                    break;
                }
            }
            let mut app = APP.with(|cell| cell.borrow_mut().take()).unwrap();
            std::mem::swap(&mut app.tile_manager, &mut manager);
            panel = app.scenes.take().unwrap();
            if app.scene_restore.is_some() || manager.capture_scene(defaults)?.windows.len() != 2 {
                return Err(
                    "La carga pendiente no se aplicó al estabilizarse el escritorio.".into(),
                );
            }
            println!(
                "PASS: carga aplazada durante la reentrada, cancelación sin escrituras y aplicación al estabilizarse el escritorio."
            );
            Ok(())
        })();
        let _: () = msg_send![&panel.window, orderOut:ptr::null::<AnyObject>()];
        let _: () = msg_send![&prefs, removePersistentDomainForName:&*suite];
        result
    }
}
