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
    pub candidates: Vec<Rc<SceneCandidate>>,
}
pub(crate) struct SceneReview {
    pub rows: Vec<SceneRow>,
    displays: Vec<SceneDisplay>,
    destinations: Vec<Option<(Workspace, NSRect)>>,
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
            let uuid = spaces::display_uuid(id).ok_or(stale())?;
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
    if ax::window_id(w.element.0) != Some(id)
        || ax::flag(w.element.0, "AXMinimized")
        || ax::flag(w.element.0, "AXFullScreen")
        || ws::ordered_in(id) == Some(false)
    {
        return None;
    }
    let (p, s) = ax::geometry(w.element.0)?;
    let frame = NSRect::new(p, s);
    (workspace_at_frame(spaces::window_space(id)?, frame, areas) == Some(w.space)).then_some(frame)
}
impl Manager {
    fn scene_guard(&mut self, defaults: Obj) -> Result<(), String> {
        if pref(defaults, "tile.enabled") == 0
            || !ax::trusted()
            || self.presentation_blocks_input()
            || self.space_reentry.is_some()
            || self.space_quiet_until.is_some()
            || self.settling_space()
            || self.drag.is_some()
            || self.tab_drag.is_some()
            || self.pending_drop.is_some()
            || !self.handoffs.is_empty()
            || crate::focus::secure_input()
        {
            return Err(tr(
                "Activa Mosaico y Accesibilidad y termina cualquier gesto antes de revisar.",
            )
            .into());
        }
        Ok(())
    }
    pub(crate) fn capture_scene(&mut self, defaults: Obj) -> Result<Vec<SceneDisplay>, String> {
        self.scene_guard(defaults)?;
        let screens = screens(defaults)?;
        let areas: Vec<_> = screens.iter().filter_map(|s| s.visible).collect();
        let identities = ws::all_window_ids().ok_or_else(stale)?;
        let mut result = Vec::new();
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
            }
            result.push(SceneDisplay {
                uuid: s.uuid,
                name: s.name,
                distribution: d.save_distribution(area).ok_or_else(stale)?,
                bundles,
            });
        }
        if result.is_empty() {
            return Err(tr("No hay ventanas en mosaico para guardar.").into());
        }
        Ok(result)
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
            let Some(desktop) = self.desktops.get(&key) else {
                continue;
            };
            for &id in &desktop.order {
                let Some(w) = self.windows.get(&id) else {
                    continue;
                };
                let Some(app) = bundle(w.pid) else { continue };
                if !needed.contains(&app)
                    || w.space != key
                    || !identities.contains(&(w.pid, id))
                    || live_frame(w, id, &areas).is_none()
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
                    desktop.tab_strip(id).is_some_and(|(m, _)| m.len() > 1),
                ));
            }
        }
        let mut rows = Vec::new();
        let mut destinations = Vec::new();
        for display in displays {
            let screen = screens.iter().find(|s| s.uuid == display.uuid);
            let destination = screen.map(|s| s.visible.ok_or_else(stale)).transpose()?;
            destinations.push(destination);
            for (index, bundle) in display.bundles.iter().enumerate() {
                let choices = destination.map_or_else(Vec::new, |(key, _)| {
                    candidates
                        .get(bundle)
                        .into_iter()
                        .flatten()
                        .filter(|(c, group)| !*group || c.source == key)
                        .map(|(c, _)| c.clone())
                        .collect()
                });
                rows.push(SceneRow {
                    display: screen.map_or(&display.name, |s| &s.name).clone(),
                    slot: index + 1,
                    bundle: bundle.clone(),
                    available: destination.is_some(),
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
            proposals.insert(key, self.desktops.get(&key).cloned().unwrap_or_default());
            for i in range {
                let id = choices[i].ok_or_else(stale)?;
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
                    return Err(tr(
                        "Elige una ventana distinta para cada puesto de las pantallas disponibles.",
                    )
                    .into());
                }
            }
        }
        if chosen.is_empty() {
            return Err(stale());
        }
        for (&id, &(from, to)) in &chosen {
            if from != to {
                proposals
                    .entry(from)
                    .or_insert_with(|| self.desktops[&from].clone())
                    .remove(id);
            }
        }
        let areas: Vec<_> = screens(defaults)?
            .into_iter()
            .filter_map(|s| s.visible)
            .collect();
        let mut writes = Vec::with_capacity(chosen.len());
        offset = 0;
        for (d, destination) in review.displays.iter().zip(&review.destinations) {
            let range = offset..offset + d.bundles.len();
            offset = range.end;
            let Some((key, area)) = *destination else {
                continue;
            };
            if !areas.contains(&(key, area)) {
                return Err(stale());
            }
            let model = proposals.get_mut(&key).unwrap();
            if model.order.iter().any(|id| !chosen.contains_key(id)) {
                return Err(tr("Sobran ventanas en el mosaico de destino. Déjalas flotantes y vuelve a revisar.").into());
            }
            model.order = choices[range].iter().flatten().copied().collect();
            let grid = model.grid;
            if !model.restore_distribution(&d.distribution) {
                return Err(stale());
            }
            model.grid = grid;
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
            let w = &self.windows[&id];
            let ok = live_frame(w, id, &areas).is_some_and(|current| near(current, old))
                && spaces::visible_desktops().contains(&key)
                && (near(old, target) || {
                    // AX may resize successfully before refusing the position.
                    touched.push((id, old, key));
                    ax::set_frame(w.element.0, target)
                })
                && ax::geometry(w.element.0).is_some_and(|(p, s)| {
                    let frame = NSRect::new(p, s);
                    near(frame, target)
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
                return Err(tr("macOS no confirmó la colocación. Se intentó recuperar la posición anterior; revisa las ventanas.").into());
            }
        }
        for (id, (source, dest)) in chosen {
            if source != dest {
                let w = self.windows.get_mut(&id).unwrap();
                let area = areas.iter().find(|(k, _)| *k == dest).unwrap().1;
                w.original = restoration_frame(w.original, w.original_area, area);
                w.original_area = area;
                w.space = dest;
            }
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
