//! Explicit work scenes. The regular tiler is never ticked to construct a review.
use super::*;
use crate::i18n::tr;
use std::rc::Rc;

#[derive(Clone)]
pub(crate) struct SceneDisplay {
    pub uuid: String,
    pub name: String,
    pub distribution: String,
    pub bundles: Vec<String>,
}

/// A live match for one ordinal slot; never written to preferences.
pub(crate) struct SceneWindow {
    id: u32,
    pid: i32,
    element: ax::Owned,
}

impl SceneWindow {
    pub(crate) fn matches(&self, candidate: &SceneCandidate) -> bool {
        self.id == candidate.id
            && self.pid == candidate.pid
            && unsafe { ax::CFEqual(self.element.0, candidate.element.0) }
    }
}

pub(crate) struct SceneCapture {
    pub displays: Vec<SceneDisplay>,
    pub windows: Vec<SceneWindow>,
}

pub(crate) struct SceneCandidate {
    pub id: u32,
    pub label: String,
    pub app_name: Option<String>,
    pid: i32,
    source: Workspace,
    element: ax::Owned,
}
pub(crate) struct SceneRow {
    pub display: String,
    pub slot: usize,
    pub bundle: String,
    pub available: bool,
    pub local_candidates: usize,
    pub candidates: Vec<Rc<SceneCandidate>>,
}
pub(crate) struct SceneReview {
    pub rows: Vec<SceneRow>,
    displays: Vec<SceneDisplay>,
    destinations: Vec<Option<(Workspace, NSRect)>>,
}

impl SceneReview {
    pub(crate) fn same_destinations(&self, other: &Self) -> bool {
        self.destinations == other.destinations
    }
}
struct Screen {
    uuid: String,
    name: String,
    visible: Option<(Workspace, NSRect)>,
}
fn screens(defaults: Obj) -> Result<Vec<Screen>, String> {
    let visible = spaces::visible_desktops();
    let mut result = Vec::new();
    unsafe {
        let all: Obj = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![all, count];
        for i in 0..count {
            let screen: Obj = msg_send![all,objectAtIndex:i];
            let desc: Obj = msg_send![screen, deviceDescription];
            let number: Obj =
                msg_send![desc,objectForKey:objc2_foundation::ns_string!("NSScreenNumber")];
            let id: u32 = msg_send![number, unsignedIntValue];
            let uuid = spaces::display_uuid(id).ok_or_else(stale)?;
            if result.iter().any(|s: &Screen| s.uuid == uuid) {
                return Err(stale());
            }
            let name: Retained<NSString> = msg_send![screen, localizedName];
            let visible = visible
                .iter()
                .copied()
                .find(|w| w.0 == id && pref(defaults, &format!("tile.off.{}", w.1)) == 0)
                .and_then(|w| Some((w, area(id)?)));
            result.push(Screen {
                uuid,
                name: name.to_string(),
                visible,
            });
        }
    }
    if result.is_empty() {
        return Err(stale());
    }
    Ok(result)
}
fn stale() -> String {
    tr("La pantalla o sus ventanas han cambiado. Pulsa Revisar.").into()
}
fn bundle(pid: i32) -> Option<String> {
    unsafe {
        let app: Option<Retained<AnyObject>> =
            msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:pid];
        let app = app?;
        if msg_send![&app, isHidden] {
            return None;
        }
        let id: Option<Retained<NSString>> = msg_send![&app, bundleIdentifier];
        id.map(|v| v.to_string()).filter(|v| !v.is_empty())
    }
}
fn live_frame(w: &Managed, id: u32, areas: &[(Workspace, NSRect)]) -> Option<NSRect> {
    if ax::window_id(w.element.0) != Some(id) || ws::ordered_in(id) == Some(false) {
        return None;
    }
    let (p, s) = ax::geometry(w.element.0)?;
    let frame = NSRect::new(p, s);
    (workspace_at_frame(spaces::window_space(id)?, frame, areas) == Some(w.space)).then_some(frame)
}
impl Manager {
    pub(crate) fn scene_waiting_for_space(&mut self) -> bool {
        self.waiting_for_space(Instant::now())
            || self.space_reentry.is_some()
            || self.settling_space()
    }

    fn scene_guard(&mut self, defaults: Obj) -> Result<(), String> {
        if pref(defaults, "tile.enabled") == 0 {
            return Err(tr("Activa Mosaico en Ajustes para usar escenas de trabajo.").into());
        }
        if !ax::trusted() {
            return Err(tr("Concede Accesibilidad para usar escenas de trabajo.").into());
        }
        let now = Instant::now();
        if self.presentation_blocks_input() || self.waiting_for_space(now) {
            return Err(tr("Espera a que termine el cambio de escritorio.").into());
        }
        if self.drag.is_some()
            || self.tab_drag.is_some()
            || self
                .pending_drop
                .as_ref()
                .is_some_and(|drop| now < drop.expires)
            || self.handoffs.values().any(|handoff| now < handoff.expires)
        {
            return Err(tr("Termina el gesto de mosaico antes de continuar.").into());
        }
        if crate::focus::secure_input() {
            return Err(tr("Hay una entrada segura activa; termina ese cuadro de diálogo.").into());
        }
        Ok(())
    }
    pub(crate) fn capture_scene(&mut self, defaults: Obj) -> Result<SceneCapture, String> {
        // Saving reads the committed layout; it does not place windows. Verify
        // every member live instead of waiting for the global placement barrier.
        self.scene_guard(defaults)?;
        let screens = screens(defaults)?;
        let areas: Vec<_> = screens.iter().filter_map(|s| s.visible).collect();
        let identities = ws::all_window_ids().ok_or_else(stale)?;
        let mut capture = SceneCapture {
            displays: Vec::new(),
            windows: Vec::new(),
        };
        for s in screens {
            let Some((key, area)) = s.visible else {
                continue;
            };
            let Some(d) = self.desktops.get(&key).filter(|d| !d.order.is_empty()) else {
                continue;
            };
            let mut bundles = Vec::with_capacity(d.order.len());
            for &id in &d.order {
                let w = self.windows.get(&id).ok_or_else(stale)?;
                if w.space != key
                    || !identities.contains(&(w.pid, id))
                    || live_frame(w, id, &areas).is_none()
                {
                    return Err(stale());
                }
                bundles.push(bundle(w.pid).ok_or_else(stale)?);
                capture.windows.push(SceneWindow {
                    id,
                    pid: w.pid,
                    element: w.element.clone(),
                });
            }
            capture.displays.push(SceneDisplay {
                uuid: s.uuid,
                name: s.name,
                distribution: d.save_scene_distribution(area).ok_or_else(stale)?,
                bundles,
            });
        }
        if capture.displays.is_empty() {
            return Err(tr("No hay ventanas en mosaico para guardar.").into());
        }
        Ok(capture)
    }
    pub(crate) fn review_scene(
        &mut self,
        defaults: Obj,
        displays: &[SceneDisplay],
    ) -> Result<SceneReview, String> {
        self.scene_guard(defaults)?;
        let screens = screens(defaults)?;
        let areas: Vec<_> = screens.iter().filter_map(|s| s.visible).collect();
        let identities = ws::all_window_ids().ok_or_else(stale)?;
        let needed: HashSet<_> = displays.iter().flat_map(|d| d.bundles.iter()).collect();
        let mut candidates: HashMap<String, Vec<(Rc<SceneCandidate>, bool)>> = HashMap::new();
        for screen in &screens {
            let Some((key, _)) = screen.visible else {
                continue;
            };
            let tiled = self.desktops.get(&key).into_iter().flat_map(|d| &d.order);
            let mut floating: Vec<_> = self
                .floating
                .iter()
                .filter(|id| self.windows.get(id).is_some_and(|w| w.space == key))
                .collect();
            floating.sort_unstable();
            for &id in tiled.chain(floating) {
                let Some(w) = self.windows.get(&id) else {
                    continue;
                };
                let Some(app) = bundle(w.pid) else { continue };
                if !needed.contains(&app)
                    || w.space != key
                    || !identities.contains(&(w.pid, id))
                    || live_frame(w, id, &areas).is_none()
                    || (self.floating.contains(&id) && !ax::tileable(w.element.0))
                {
                    continue;
                }
                let title = window_label(w.element.0 as Obj, w.pid);
                let label = format!("{title} · {} · #{id}", screen.name);
                candidates.entry(app).or_default().push((
                    Rc::new(SceneCandidate {
                        app_name: app_name(w.pid),
                        id,
                        pid: w.pid,
                        source: key,
                        element: w.element.clone(),
                        label,
                    }),
                    self.desktops
                        .get(&key)
                        .and_then(|d| d.tab_strip(id))
                        .is_some_and(|(m, _)| m.len() > 1),
                ));
            }
        }
        let mut rows = Vec::new();
        let mut destinations = Vec::new();
        for display in displays {
            let screen = screens.iter().find(|s| s.uuid == display.uuid);
            let destination = screen.and_then(|s| s.visible);
            destinations.push(destination);
            for (index, bundle) in display.bundles.iter().enumerate() {
                let mut choices = destination.map_or_else(Vec::new, |(key, _)| {
                    candidates
                        .get(bundle)
                        .into_iter()
                        .flatten()
                        .filter(|(c, group)| !*group || c.source == key)
                        .map(|(c, _)| c.clone())
                        .collect()
                });
                let local_candidates = destination.map_or(0, |(key, _)| {
                    choices.sort_by_key(|c| (c.source != key, self.floating.contains(&c.id)));
                    choices.partition_point(|c| c.source == key)
                });
                rows.push(SceneRow {
                    display: screen.map_or(&display.name, |s| &s.name).clone(),
                    slot: index + 1,
                    bundle: bundle.clone(),
                    available: destination.is_some(),
                    local_candidates,
                    candidates: choices,
                });
            }
        }
        Ok(SceneReview {
            rows,
            displays: displays.to_vec(),
            destinations,
        })
    }
    pub(crate) fn apply_scene(
        &mut self,
        defaults: Obj,
        review: &SceneReview,
        choices: &[Option<u32>],
    ) -> Result<(), String> {
        if self.space_reentry.is_some() || self.settling_space() {
            return Err(tr("Espera a que termine el cambio de escritorio.").into());
        }
        let fresh = self.review_scene(defaults, &review.displays)?;
        if review.destinations != fresh.destinations || choices.len() != review.rows.len() {
            return Err(stale());
        }
        let mut chosen = HashMap::new();
        let mut proposals: HashMap<Workspace, Desktop> = HashMap::new();
        let mut offset = 0;
        for (d, destination) in review.displays.iter().zip(&review.destinations) {
            let range = offset..offset + d.bundles.len();
            offset = range.end;
            let Some((key, _)) = *destination else {
                if choices[range].iter().any(Option::is_some) {
                    return Err(stale());
                }
                continue;
            };
            for i in range {
                let Some(id) = choices[i] else { continue };
                let before = review.rows[i]
                    .candidates
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or_else(stale)?;
                let now = fresh.rows[i]
                    .candidates
                    .iter()
                    .find(|c| {
                        c.id == id
                            && c.pid == before.pid
                            && c.source == before.source
                            && unsafe { ax::CFEqual(c.element.0, before.element.0) }
                    })
                    .ok_or_else(stale)?;
                if chosen.insert(id, (now.source, key)).is_some() {
                    return Err(tr("No uses la misma ventana en dos puestos.").into());
                }
                proposals
                    .entry(key)
                    .or_insert_with(|| self.desktops.get(&key).cloned().unwrap_or_default());
            }
        }
        if chosen.is_empty() {
            return Err(tr("Asigna al menos una ventana para aplicar la escena.").into());
        }
        for (&id, &(from, to)) in &chosen {
            if from != to {
                proposals
                    .entry(from)
                    .or_insert_with(|| self.desktops.get(&from).cloned().unwrap_or_default())
                    .remove(id);
            }
        }
        let areas: Vec<_> = screens(defaults)?
            .into_iter()
            .filter_map(|s| s.visible)
            .collect();
        let mut writes = Vec::with_capacity(chosen.len());
        let mut unmatched = Vec::new();
        offset = 0;
        for (d, destination) in review.displays.iter().zip(&review.destinations) {
            let range = offset..offset + d.bundles.len();
            offset = range.end;
            let Some((key, area)) = *destination else {
                continue;
            };
            if choices[range.clone()].iter().all(Option::is_none) {
                continue;
            }
            if !areas.contains(&(key, area)) {
                return Err(stale());
            }
            let model = proposals.get_mut(&key).unwrap();
            let extras: Vec<_> = model
                .order
                .iter()
                .copied()
                .filter(|id| !chosen.contains_key(id))
                .collect();
            for id in extras {
                if model
                    .tab_strip(id)
                    .is_some_and(|(members, _)| members.len() > 1)
                {
                    return Err(tr("Hay un grupo de pestañas que no pertenece a la escena. Sepáralo antes de cargar.").into());
                }
                let w = self.windows.get(&id).ok_or_else(stale)?;
                let frame = live_frame(w, id, &areas).ok_or_else(stale)?;
                unmatched.push((id, frame, area));
                model.remove(id);
            }
            if !model.restore_scene_distribution(&d.distribution, &choices[range]) {
                return Err(stale());
            }
            model.set_minimum_sizes(&self.minimum_sizes);
            let frames = model.window_frames(area, 10.);
            if frames.len() != model.order.len() {
                return Err(stale());
            }
            for (id, target) in frames {
                let w = &self.windows[&id];
                let old = live_frame(w, id, &areas).ok_or_else(stale)?;
                if !ax::tileable(w.element.0)
                    || ![
                        target.origin.x,
                        target.origin.y,
                        target.size.width,
                        target.size.height,
                    ]
                    .iter()
                    .all(|n| n.is_finite())
                    || target.size.width < 80.
                    || target.size.height < 60.
                    || target.origin.x < area.origin.x - 0.001
                    || target.origin.y < area.origin.y - 0.001
                    || target.origin.x + target.size.width > area.origin.x + area.size.width + 0.001
                    || target.origin.y + target.size.height
                        > area.origin.y + area.size.height + 0.001
                    || self.minimum_sizes.get(&id).is_some_and(|min| {
                        target.size.width + 1. < min.width || target.size.height + 1. < min.height
                    })
                {
                    return Err(tr("Esta distribución no cabe. Elige otra plantilla.").into());
                }
                writes.push((id, old, target, key));
            }
        }
        let mut touched = Vec::with_capacity(writes.len());
        for &(id, old, target, key) in &writes {
            let target = ax::integral_frame(target);
            let w = &self.windows[&id];
            let ok = live_frame(w, id, &areas) == Some(old)
                && spaces::visible_desktops().contains(&key)
                && (old == target || {
                    // AX may resize successfully before refusing the position.
                    touched.push((id, old, key));
                    ax::set_frame(w.element.0, target)
                })
                && ax::geometry(w.element.0).is_some_and(|(p, s)| {
                    let frame = NSRect::new(p, s);
                    frame == target
                        && spaces::window_space(id)
                            .and_then(|space| workspace_at_frame(space, frame, &areas))
                            == Some(key)
                });
            if !ok {
                let visible = spaces::visible_desktops();
                for &(id, frame, dest) in touched.iter().rev() {
                    let window = &self.windows[&id];
                    if visible.contains(&window.space)
                        && visible.contains(&dest)
                        && ax::window_id(window.element.0) == Some(id)
                        && spaces::window_space(id)
                            .is_some_and(|s| s == window.space.1 || s == dest.1)
                    {
                        ax::set_frame(window.element.0, frame);
                    }
                }
                self.invalidate();
                let app = app_name(self.windows[&id].pid).unwrap_or_else(|| format!("#{id}"));
                return Err(format!(
                    "{app}: {}",
                    tr(
                        "macOS no confirmó la colocación. Se intentó recuperar la posición anterior; revisa las ventanas."
                    )
                ));
            }
        }
        for (id, (source, dest)) in chosen {
            self.floating.remove(&id);
            if source != dest {
                let w = self.windows.get_mut(&id).unwrap();
                let area = areas.iter().find(|(k, _)| *k == dest).unwrap().1;
                w.original = restoration_frame(w.original, w.original_area, area);
                w.original_area = area;
                w.space = dest;
            }
        }
        // Keep unrelated windows exactly where they were, outside the restored
        // tree. Commit only after every AX write has been confirmed.
        for (id, frame, area) in unmatched {
            self.cancel_window_placement(id);
            self.floating.insert(id);
            self.minimum_sizes.remove(&id);
            let w = self.windows.get_mut(&id).unwrap();
            w.original = frame;
            w.original_area = area;
        }
        for (key, model) in proposals {
            self.cancel_desktop_placements(&model);
            save(defaults, &layout_key(key), model.layout.index());
            self.desktops.insert(key, model);
        }
        self.invalidate();
        Ok(())
    }
}
