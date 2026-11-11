//! Reproducible, synthetic sessions. Never creates or moves native windows.
use super::*;
use objc2_foundation::NSSize;

fn geometry(desktop: &Desktop, area: NSRect, case: &str) {
    let frames = desktop.frames(area, 10.);
    assert_eq!(frames.len(), desktop.order.len(), "missing leaves: {case}");
    let mut ids: Vec<_> = frames.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), frames.len(), "duplicate leaves: {case}");
    for (index, (_, frame)) in frames.iter().enumerate() {
        assert!(
            [
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height
            ]
            .into_iter()
            .all(f64::is_finite),
            "nonfinite geometry: {case}"
        );
        assert!(
            frame.size.width >= 80. - 0.001 && frame.size.height >= 60. - 0.001,
            "unusable tile {frame:?}: {case}"
        );
        assert!(
            frame.origin.x >= area.origin.x - 0.001
                && frame.origin.y >= area.origin.y - 0.001
                && frame.origin.x + frame.size.width <= area.origin.x + area.size.width + 0.001
                && frame.origin.y + frame.size.height <= area.origin.y + area.size.height + 0.001,
            "out of bounds {frame:?} / {area:?}: {case}"
        );
        for (_, other) in &frames[index + 1..] {
            assert!(
                overlap(*frame, *other) < 0.01,
                "overlap {frame:?} / {other:?}: {case}"
            );
        }
    }
}

#[test]
fn placement_faults_are_bounded_and_repeated_notifications_do_not_restart_limits() {
    let start = Instant::now();
    let target = rect(-1080., -300., 1060., 600.);
    let limited = rect(-1080., -300., 1060., 680.);
    let mut cases = 0;
    for accepted in [false, true] {
        for delay in [0, 1, 59, 60, 61, 600, 5000] {
            let mut verification = Verification {
                target,
                due: start + Duration::from_millis(60),
                retried: false,
                accepted,
            };
            let now = start + Duration::from_millis(delay);
            assert_eq!(
                verification.step(limited, now),
                if delay < 60 {
                    VerificationStep::Wait
                } else {
                    VerificationStep::Retry
                }
            );
            assert_eq!(verification.step(target, now), VerificationStep::Complete);
            let retry = start + Duration::from_millis(delay.max(60));
            verification.step(limited, retry);
            assert_eq!(
                verification.step(limited, retry + Duration::from_millis(59)),
                VerificationStep::Wait
            );
            let final_step = if accepted {
                VerificationStep::Limited
            } else {
                VerificationStep::Rejected
            };
            for later in [60, 61, 1000, 60000] {
                assert_eq!(
                    verification.step(limited, retry + Duration::from_millis(later)),
                    final_step
                );
                cases += 1;
            }
        }
    }
    for _ in 0..10000 {
        assert!(!needs_placement(
            target,
            Some(limited),
            None,
            Some((target, limited))
        ));
        assert!(!needs_placement(target, None, None, None));
        assert!(!needs_placement(target, Some(limited), Some(target), None));
    }
    assert!(needs_placement(
        target,
        Some(rect(-1080., -300., 1060., 750.)),
        None,
        Some((target, limited))
    ));
    assert!(needs_placement(
        rect(-1080., -300., 1060., 400.),
        Some(limited),
        None,
        Some((target, limited))
    ));
    eprintln!("PLACEMENT_FAULTS terminal_checks={cases} repeated_notifications=10000");
}

#[test]
fn template_mutation_corpus_is_atomic_and_accepted_trees_round_trip() {
    let area = rect(10., 41., 2540., 1870.);
    let mut checked = 0;
    let mut refused = 0;
    let mut accepted = 0;
    for layout in Layout::ALL {
        for count in 1..=8 {
            for grid in [None, Some((12, 8))] {
                let mut desktop = Desktop::configured(layout, grid);
                let ids: Vec<_> = (400..400 + count).collect();
                desktop.reconcile_at(&ids, area, 10., None);
                edit(&mut desktop, area);
                let original = desktop.save_distribution(area).unwrap();
                let tokens: Vec<_> = original.split_whitespace().collect();
                let mut corpus = vec![String::new(), format!("{original} w999"), original.clone()];
                for index in 0..tokens.len() {
                    let mut truncated = tokens.clone();
                    truncated.truncate(index);
                    corpus.push(truncated.join(" "));
                    for replacement in [
                        "NaN",
                        "inf",
                        "-inf",
                        "-1",
                        "0",
                        "999999999999999999999999",
                        "w999",
                        "w0",
                        "é",
                        "v0.9999999999999999",
                        "v0.00000000000001",
                    ] {
                        let mut altered = tokens.clone();
                        altered[index] = replacement;
                        corpus.push(altered.join(" "));
                    }
                }
                for data in corpus {
                    checked += 1;
                    let mut changed = desktop.clone();
                    if changed.restore_distribution(&data) {
                        accepted += 1;
                        // Syntax acceptance is distinct from feasibility on a
                        // particular monitor. Serialization must stay finite.
                        if let Some(encoded) = changed.save_distribution(area) {
                            let mut copy = desktop.clone();
                            assert!(copy.restore_distribution(&encoded));
                            assert_eq!(copy.save_distribution(area), Some(encoded));
                            geometry(&copy, area, &format!("mutation={checked}"));
                        }
                    } else {
                        refused += 1;
                        assert!(
                            changed == desktop,
                            "partial mutation: case={checked} text={data}"
                        );
                    }
                }
            }
        }
    }
    assert!(accepted > 0 && refused > 0);
    eprintln!("TEMPLATE_CORPUS cases={checked} accepted={accepted} rejected_atomically={refused}");
}

fn edit(desktop: &mut Desktop, area: NSRect) {
    if let Some((id, from)) = desktop.frames(area, 10.).first().copied() {
        let mut to = from;
        to.size.width *= 0.87;
        to.size.height *= 0.93;
        desktop.resize(id, from, to, area, 10.);
    }
    if desktop.order.len() > 1 {
        desktop.place(
            desktop.order[0],
            *desktop.order.last().unwrap(),
            Zone::Center,
        );
    }
}

fn destination_areas(source: NSRect) -> [NSRect; 6] {
    [
        source,
        rect(-1080., -300., 1060., 1870.),
        rect(3000., -187., 1060., 1870.),
        rect(0., -1100., 1900., 1080.),
        rect(300., 2200., 1900., 1050.),
        rect(-1300., 20., 1280., 1280.),
    ]
}

#[test]
fn exhaustive_edited_template_transfer_matrix() {
    let element = ax::application(std::process::id() as i32).unwrap();
    let mut cases = 0;
    let mut accepted = 0;
    let mut refused = 0;
    for source_area in [rect(10., 41., 2540., 1327.), rect(10., 41., 1060., 1870.)] {
        for (screen, destination_area) in destination_areas(source_area).into_iter().enumerate() {
            for from_layout in Layout::ALL {
                for to_layout in Layout::ALL {
                    for source_count in 1..=6 {
                        for destination_count in 0..=6 {
                            for source_grid in [None, Some((12, 8))] {
                                for target_grid in [None, Some((120, 6))] {
                                    cases += 1;
                                    let label = format!(
                                        "case={cases} source={from_layout:?}/{source_count}/{source_grid:?} target={to_layout:?}/{destination_count}/{target_grid:?} screen={screen}"
                                    );
                                    let source = (1, 7);
                                    let destination = if screen == 0 { (1, 9) } else { (2, 7) };
                                    let untouched = (3, 7);
                                    let source_ids: Vec<_> = (1..=source_count).collect();
                                    let target_ids: Vec<_> =
                                        (1001..1001 + destination_count).collect();
                                    let mut from = Desktop::configured(from_layout, source_grid);
                                    from.reconcile_at(&source_ids, source_area, 10., None);
                                    edit(&mut from, source_area);
                                    geometry(&from, source_area, &label);
                                    let mut to = Desktop::configured(to_layout, target_grid);
                                    to.reconcile_at(&target_ids, destination_area, 10., None);
                                    edit(&mut to, destination_area);
                                    geometry(&to, destination_area, &label);
                                    let id = from.order[0];
                                    let original = from
                                        .frames(source_area, 10.)
                                        .into_iter()
                                        .find(|(window, _)| *window == id)
                                        .unwrap()
                                        .1;
                                    let mut other = Desktop::configured(Layout::Grid, None);
                                    other.reconcile_at(&[2001, 2002], source_area, 10., None);
                                    let mut manager = Manager::default();
                                    manager.desktops.insert(source, from.clone());
                                    manager.desktops.insert(destination, to.clone());
                                    manager.desktops.insert(untouched, other.clone());
                                    manager.windows.insert(
                                        id,
                                        Managed {
                                            pid: 42,
                                            space: source,
                                            original,
                                            original_area: source_area,
                                            element: element.clone(),
                                        },
                                    );
                                    let minimums = HashMap::from([(
                                        id,
                                        if cases % 3 == 0 {
                                            NSSize::new(640., 360.)
                                        } else {
                                            NSSize::new(120., 90.)
                                        },
                                    )]);
                                    let proposal = arrival_proposal(
                                        &to,
                                        id,
                                        None,
                                        destination_area,
                                        Some(&minimums),
                                    );
                                    // A preflight cannot change either committed tree, including on refusal.
                                    assert!(
                                        manager.desktops[&source] == from,
                                        "source mutation: {label}"
                                    );
                                    assert!(
                                        manager.desktops[&destination] == to,
                                        "target mutation: {label}"
                                    );
                                    let Some(proposal) = proposal else {
                                        refused += 1;
                                        continue;
                                    };
                                    geometry(&proposal, destination_area, &label);
                                    let target = proposal
                                        .frames(destination_area, 10.)
                                        .into_iter()
                                        .find(|(window, _)| *window == id)
                                        .unwrap()
                                        .1;
                                    manager.commit_keyboard_transfer_model(
                                        KeyboardTransfer {
                                            source,
                                            destination,
                                            target,
                                            source_area,
                                            destination_area,
                                            desktop: Some(proposal),
                                            tracked: true,
                                            floating: false,
                                        },
                                        42,
                                        id,
                                        element.clone(),
                                        original,
                                    );
                                    accepted += 1;
                                    assert!(
                                        manager.desktops[&untouched] == other,
                                        "third workspace mutation: {label}"
                                    );
                                    assert_eq!(
                                        manager.desktops[&source].layout, from_layout,
                                        "{label}"
                                    );
                                    assert_eq!(
                                        manager.desktops[&destination].layout, to_layout,
                                        "{label}"
                                    );
                                    assert_eq!(manager.windows[&id].space, destination, "{label}");
                                    assert_eq!(
                                        manager
                                            .desktops
                                            .values()
                                            .filter(|desktop| desktop.order.contains(&id))
                                            .count(),
                                        1,
                                        "duplicate owner: {label}"
                                    );
                                    let mut before = source_ids;
                                    before.extend(target_ids);
                                    before.sort_unstable();
                                    let mut after = manager.desktops[&source].order.clone();
                                    after.extend(&manager.desktops[&destination].order);
                                    after.sort_unstable();
                                    assert_eq!(after, before, "lost or duplicated window: {label}");
                                    geometry(&manager.desktops[&source], source_area, &label);
                                    geometry(
                                        &manager.desktops[&destination],
                                        destination_area,
                                        &label,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 129_024);
    assert!(accepted > 0 && refused > 0);
    eprintln!("TRANSFER_MATRIX cases={cases} accepted={accepted} refused_safely={refused}");
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Tiled,
    Floating,
    Hidden,
}
#[derive(Clone, Copy)]
struct Owner {
    workspace: Workspace,
    slot: Slot,
    pid: i32,
}
struct Session {
    manager: Manager,
    areas: Vec<(Workspace, NSRect)>,
    owners: HashMap<u32, Owner>,
    element: ax::Owned,
    serial: u32,
    recycled: Vec<(u32, i32)>,
    saved: Vec<String>,
    clock: Instant,
    rng: u64,
}
impl Session {
    fn new(seed: u64) -> Self {
        let element = ax::application(std::process::id() as i32).unwrap();
        let areas = vec![
            ((1, 7), rect(10., 41., 2540., 1327.)),
            ((1, 10), rect(10., 41., 2540., 1327.)),
            ((2, 8), rect(2570., -187., 1060., 1870.)),
            ((2, 11), rect(2570., -187., 1060., 1870.)),
            ((3, 9), rect(-1920., -800., 1900., 1080.)),
            ((3, 12), rect(-1920., -800., 1900., 1080.)),
        ];
        let mut manager = Manager::default();
        let mut owners = HashMap::new();
        let mut serial = 1;
        for (index, (key, area)) in areas.iter().enumerate() {
            let count = 2 + (seed as usize + index) % 3;
            let ids: Vec<_> = (serial..serial + count as u32).collect();
            serial += count as u32;
            let mut desktop = Desktop::configured(
                Layout::ALL[(seed as usize + index) % 8],
                if index % 2 == 0 { Some((12, 8)) } else { None },
            );
            desktop.reconcile_at(&ids, *area, 10., None);
            edit(&mut desktop, *area);
            for (id, frame) in desktop.frames(*area, 10.) {
                owners.insert(
                    id,
                    Owner {
                        workspace: *key,
                        slot: Slot::Tiled,
                        pid: 42,
                    },
                );
                manager.windows.insert(
                    id,
                    Managed {
                        pid: 42,
                        space: *key,
                        original: frame,
                        original_area: *area,
                        element: element.clone(),
                    },
                );
            }
            manager.desktops.insert(*key, desktop);
        }
        manager.active_areas = vec![areas[0], areas[2], areas[4]];
        Self {
            manager,
            areas,
            owners,
            element,
            serial,
            recycled: vec![],
            saved: vec![],
            clock: Instant::now(),
            rng: seed,
        }
    }
    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
    fn pick_owner(&mut self, tiled: bool) -> Option<u32> {
        let mut ids: Vec<_> = self
            .owners
            .iter()
            .filter(|(_, owner)| {
                self.manager
                    .active_areas
                    .iter()
                    .any(|(key, _)| *key == owner.workspace)
                    && (!tiled || owner.slot == Slot::Tiled)
            })
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        if ids.is_empty() {
            None
        } else {
            Some(ids[self.next() as usize % ids.len()])
        }
    }
    fn active(&mut self) -> (Workspace, NSRect) {
        {
            let index = self.next() as usize % self.manager.active_areas.len();
            self.manager.active_areas[index]
        }
    }
    fn area(&self, key: Workspace) -> NSRect {
        self.areas.iter().find(|(k, _)| *k == key).unwrap().1
    }
    fn settle(&mut self) {
        if self.manager.space_reentry.is_none() {
            return;
        }
        let mut windows = vec![];
        for (key, area) in &self.manager.active_areas {
            for (id, frame) in self.manager.desktops[key].frames(*area, 10.) {
                windows.push((self.owners[&id].pid, id, key.1, frame, Some(frame)));
            }
        }
        let sample = ReentrySample {
            areas: self.manager.active_areas.clone(),
            windows,
            absent: vec![],
        };
        assert!(
            !self
                .manager
                .observe_reentry(None, self.clock + Duration::from_millis(400))
        );
        assert!(!self.manager.observe_reentry(
            Some(sample.clone()),
            self.clock + Duration::from_millis(600)
        ));
        assert!(
            self.manager
                .observe_reentry(Some(sample), self.clock + Duration::from_millis(900))
        );
        self.clock += Duration::from_millis(1000);
        assert!(!self.manager.waiting_for_space(self.clock));
    }
    fn check(&self, seed: u64, step: usize, op: usize) {
        let label = format!("seed={seed:#x} step={step} op={op}");
        let mut membership = HashMap::new();
        for (key, area) in &self.areas {
            let desktop = &self.manager.desktops[key];
            let detail = format!(
                "{label} workspace={key:?} layout={:?} grid={:?} order={:?} minima={:?}",
                desktop.layout,
                desktop.grid,
                desktop.order,
                desktop
                    .order
                    .iter()
                    .map(|id| (*id, self.manager.minimum_sizes.get(id)))
                    .collect::<Vec<_>>()
            );
            if !desktop.order.is_empty() && desktop.frames(*area, 10.).is_empty() {
                // Membership changes can make a symmetric centered layout
                // incompatible with application minima. This is a safe blocked
                // layout, not permission to replace the user's template.
                assert!(
                    desktop
                        .order
                        .iter()
                        .any(|id| self.manager.minimum_sizes.contains_key(id)),
                    "unexplained refusal: {detail}"
                );
                let mut unconstrained = desktop.clone();
                unconstrained.set_minimum_sizes(&HashMap::new());
                geometry(&unconstrained, *area, &detail);
                assert!(
                    desktop.save_distribution(*area).is_none(),
                    "infeasible template was serializable: {detail}"
                );
            } else {
                geometry(desktop, *area, &detail);
            }
            for id in &desktop.order {
                assert!(
                    membership.insert(*id, *key).is_none(),
                    "duplicate ownership {id}: {label}"
                );
                let owner = self.owners.get(id).expect(&label);
                assert_eq!(owner.workspace, *key, "{label}");
                assert_eq!(owner.slot, Slot::Tiled, "{label}");
            }
        }
        assert_eq!(self.manager.windows.len(), self.owners.len(), "{label}");
        for (id, owner) in &self.owners {
            let window = self.manager.windows.get(id).expect(&label);
            assert_eq!(window.pid, owner.pid, "PID reuse: {label}");
            assert_eq!(window.space, owner.workspace, "{label}");
            assert_eq!(
                membership.contains_key(id),
                owner.slot == Slot::Tiled,
                "missing membership {id}: {label}"
            );
            assert_eq!(
                self.manager.floating.contains(id),
                owner.slot == Slot::Floating,
                "floating state {id}: {label}"
            );
        }
        for id in self
            .manager
            .applied
            .keys()
            .chain(self.manager.transitions.keys())
            .chain(self.manager.verifications.keys())
            .chain(self.manager.constrained.keys())
            .chain(self.manager.minimum_sizes.keys())
            .chain(self.manager.handoffs.keys())
        {
            assert!(
                self.owners.contains_key(id),
                "deferred state survived closing {id}: {label}"
            );
        }
    }
}

#[test]
fn seeded_sessions_mix_manual_edits_transfers_templates_and_native_faults() {
    use objc2::{
        class, msg_send,
        rc::{Allocated, Retained},
        runtime::AnyObject,
    };
    objc2::rc::autoreleasepool(|_| unsafe {
        let suite = NSString::from_str(&format!(
            "amspace.robustness.sessions.{}",
            std::process::id()
        ));
        let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
        let raw = Retained::as_ptr(&defaults) as Obj;
        let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        let mut operations = [0u64; 12];
        let mut moved = 0;
        let mut refused = 0;
        for number in 1..=64u64 {
            let seed = number.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let mut session = Session::new(seed);
            for step in 0..500 {
                let op = session.next() as usize % 12;
                operations[op] += 1;
                let (key, area) = session.active();
                match op {
                    0 => {
                        if let Some(id) = session.pick_owner(true) {
                            let owner = session.owners[&id];
                            let area = session.area(owner.workspace);
                            let Some((_, frame)) = session.manager.desktops[&owner.workspace]
                                .frames(area, 10.)
                                .into_iter()
                                .find(|(w, _)| *w == id)
                            else {
                                session.check(seed, step, op);
                                continue;
                            };
                            let delta = (session.next() % 161) as f64 - 80.;
                            let mut target = frame;
                            match session.next() % 4 {
                                0 => {
                                    target.origin.x += delta;
                                    target.size.width -= delta;
                                }
                                1 => target.size.width += delta,
                                2 => {
                                    target.origin.y += delta;
                                    target.size.height -= delta;
                                }
                                _ => target.size.height += delta,
                            }
                            session
                                .manager
                                .desktops
                                .get_mut(&owner.workspace)
                                .unwrap()
                                .resize(id, frame, target, area, 10.);
                        }
                    }
                    1 => {
                        let ids = session.manager.desktops[&key].order.clone();
                        if ids.len() > 1 {
                            let id = ids[session.next() as usize % ids.len()];
                            let target = ids[session.next() as usize % ids.len()];
                            let intent = DropIntent::Window(target, Zone::Center);
                            if let Some(proposed) = arrival_proposal(
                                &session.manager.desktops[&key],
                                id,
                                Some(intent),
                                area,
                                Some(&session.manager.minimum_sizes),
                            ) {
                                session.manager.desktops.insert(key, proposed);
                            }
                        }
                    }
                    2 => {
                        if let Some(id) = session.pick_owner(false) {
                            let owner = session.owners[&id];
                            if owner.slot != Slot::Hidden && owner.workspace != key {
                                let before = session.manager.desktops.clone();
                                let desktop = if owner.slot == Slot::Floating {
                                    None
                                } else {
                                    let proposal = arrival_proposal(
                                        &session.manager.desktops[&key],
                                        id,
                                        None,
                                        area,
                                        Some(&session.manager.minimum_sizes),
                                    );
                                    if proposal.is_none() {
                                        refused += 1;
                                        session.check(seed, step, op);
                                        continue;
                                    }
                                    proposal
                                };
                                let original = session.manager.windows[&id].original;
                                let target = desktop
                                    .as_ref()
                                    .and_then(|d| {
                                        d.frames(area, 10.)
                                            .into_iter()
                                            .find(|(w, _)| *w == id)
                                            .map(|(_, r)| r)
                                    })
                                    .unwrap_or_else(|| {
                                        restoration_frame(
                                            original,
                                            session.area(owner.workspace),
                                            area,
                                        )
                                    });
                                let plan = KeyboardTransfer {
                                    source: owner.workspace,
                                    destination: key,
                                    target,
                                    source_area: session.area(owner.workspace),
                                    destination_area: area,
                                    desktop,
                                    tracked: true,
                                    floating: owner.slot == Slot::Floating,
                                };
                                session.manager.commit_keyboard_transfer_model(
                                    plan,
                                    owner.pid,
                                    id,
                                    session.element.clone(),
                                    original,
                                );
                                session.owners.get_mut(&id).unwrap().workspace = key;
                                moved += 1;
                                for (other, desktop) in &before {
                                    if *other != owner.workspace && *other != key {
                                        assert!(
                                            session.manager.desktops[other] == *desktop,
                                            "unrelated workspace: seed={seed:#x} step={step}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    3 => {
                        if session.manager.desktops[&key].order.len() < 8 {
                            let reuse = session.next() % 2 == 0 && !session.recycled.is_empty();
                            let (id, pid) = if reuse {
                                let (id, pid) = session.recycled.pop().unwrap();
                                (id, pid + 1)
                            } else {
                                let id = session.serial;
                                session.serial += 1;
                                (id, 42)
                            };
                            if let Some(proposed) = arrival_proposal(
                                &session.manager.desktops[&key],
                                id,
                                None,
                                area,
                                Some(&session.manager.minimum_sizes),
                            ) {
                                let frame = proposed
                                    .frames(area, 10.)
                                    .into_iter()
                                    .find(|(w, _)| *w == id)
                                    .unwrap()
                                    .1;
                                session.manager.desktops.insert(key, proposed);
                                session.manager.windows.insert(
                                    id,
                                    Managed {
                                        pid,
                                        space: key,
                                        original: frame,
                                        original_area: area,
                                        element: session.element.clone(),
                                    },
                                );
                                session.owners.insert(
                                    id,
                                    Owner {
                                        workspace: key,
                                        slot: Slot::Tiled,
                                        pid,
                                    },
                                );
                            }
                        }
                    }
                    4 => {
                        if let Some(id) = session.pick_owner(false) {
                            let owner = session.owners.remove(&id).unwrap();
                            session.manager.forget_window(id);
                            session.recycled.push((id, owner.pid));
                        }
                    }
                    5 => {
                        if let Some(id) = session.pick_owner(false) {
                            let owner = session.owners[&id];
                            if owner.slot == Slot::Tiled {
                                session
                                    .manager
                                    .desktops
                                    .get_mut(&owner.workspace)
                                    .unwrap()
                                    .remove(id);
                                session.manager.cancel_window_placement(id);
                                session.manager.floating.insert(id);
                                session.owners.get_mut(&id).unwrap().slot = Slot::Floating;
                            } else if owner.slot == Slot::Floating {
                                let area = session.area(owner.workspace);
                                if let Some(proposed) = arrival_proposal(
                                    &session.manager.desktops[&owner.workspace],
                                    id,
                                    None,
                                    area,
                                    Some(&session.manager.minimum_sizes),
                                ) {
                                    session.manager.desktops.insert(owner.workspace, proposed);
                                    session.manager.floating.remove(&id);
                                    session.owners.get_mut(&id).unwrap().slot = Slot::Tiled;
                                }
                            }
                        }
                    }
                    6 => {
                        let mut visible: Vec<_> = session
                            .manager
                            .active_areas
                            .iter()
                            .map(|(key, _)| *key)
                            .collect();
                        let display = session.next() as usize % 3;
                        let alternatives: Vec<_> = session
                            .areas
                            .iter()
                            .filter(|(key, _)| {
                                key.0 == visible[display].0 && *key != visible[display]
                            })
                            .map(|(key, _)| *key)
                            .collect();
                        visible[display] = alternatives[0];
                        let before = session.manager.desktops.clone();
                        session
                            .manager
                            .space_changed_with_visible_at(session.clock, &visible);
                        session.manager.active_areas = visible
                            .iter()
                            .map(|key| (*key, session.area(*key)))
                            .collect();
                        session.settle();
                        assert!(
                            session.manager.desktops == before,
                            "Space reset a tree: seed={seed:#x} step={step}"
                        );
                    }
                    7 => {
                        if let Some(id) = session.pick_owner(true) {
                            let owner = session.owners[&id];
                            let area = session.area(owner.workspace);
                            let before =
                                session.manager.desktops[&owner.workspace].save_distribution(area);
                            let uncertain = HashSet::from([id]);
                            let confirmed = HashSet::from([id]);
                            for delay in [0, 649, 650, 5000] {
                                let kept = session.manager.membership_grace.retained(
                                    &uncertain,
                                    &confirmed,
                                    session.clock + Duration::from_millis(delay),
                                );
                                let mut members =
                                    session.manager.desktops[&owner.workspace].order.clone();
                                members.retain(|window| *window != id);
                                members.extend(kept);
                                session
                                    .manager
                                    .desktops
                                    .get_mut(&owner.workspace)
                                    .unwrap()
                                    .reconcile_at(&members, area, 10., None);
                                assert_eq!(
                                    session.manager.desktops[&owner.workspace]
                                        .save_distribution(area),
                                    before,
                                    "AX timeout reset: seed={seed:#x} step={step}"
                                );
                            }
                        }
                    }
                    8 => {
                        if let Some(id) = session.pick_owner(true) {
                            let owner = session.owners[&id];
                            let area = session.area(owner.workspace);
                            let Some((_, frame)) = session.manager.desktops[&owner.workspace]
                                .frames(area, 10.)
                                .into_iter()
                                .find(|(w, _)| *w == id)
                            else {
                                session.check(seed, step, op);
                                continue;
                            };
                            session.manager.minimum_sizes.insert(
                                id,
                                NSSize::new(
                                    (frame.size.width * 0.7).max(120.),
                                    (frame.size.height * 0.7).max(90.),
                                ),
                            );
                            let desktop =
                                session.manager.desktops.get_mut(&owner.workspace).unwrap();
                            desktop.set_minimum_sizes(&session.manager.minimum_sizes);
                            desktop.fit(area, 10.);
                        }
                    }
                    9 => {
                        if session.next() % 2 == 0 {
                            if let Some(data) =
                                session.manager.desktops[&key].save_distribution(area)
                            {
                                if session.saved.len() == 16 {
                                    session.saved.remove(0);
                                }
                                session.saved.push(data);
                            }
                        } else if !session.saved.is_empty() {
                            let index = session.next() as usize % session.saved.len();
                            let data = session.saved[index].clone();
                            let before = session.manager.desktops.clone();
                            let grid = (
                                pref(raw, "tile.grid.enabled"),
                                pref(raw, "tile.grid.columns"),
                            );
                            let ok = session
                                .manager
                                .restore_distribution_with_grid(raw, key, &data, area, true);
                            if !ok {
                                assert!(
                                    session.manager.desktops == before,
                                    "failed template mutated session: seed={seed:#x} step={step}"
                                );
                            }
                            for (other, desktop) in before {
                                if other != key {
                                    assert!(session.manager.desktops[&other] == desktop);
                                }
                            }
                            assert_eq!(
                                (
                                    pref(raw, "tile.grid.enabled"),
                                    pref(raw, "tile.grid.columns")
                                ),
                                grid
                            );
                        }
                    }
                    10 => {
                        if let Some(id) = session.pick_owner(false) {
                            let owner = session.owners[&id];
                            if owner.slot == Slot::Tiled {
                                session
                                    .manager
                                    .desktops
                                    .get_mut(&owner.workspace)
                                    .unwrap()
                                    .remove(id);
                                session.manager.cancel_window_placement(id);
                                session.owners.get_mut(&id).unwrap().slot = Slot::Hidden;
                            } else if owner.slot == Slot::Hidden {
                                let area = session.area(owner.workspace);
                                if let Some(proposed) = arrival_proposal(
                                    &session.manager.desktops[&owner.workspace],
                                    id,
                                    None,
                                    area,
                                    Some(&session.manager.minimum_sizes),
                                ) {
                                    session.manager.desktops.insert(owner.workspace, proposed);
                                    session.owners.get_mut(&id).unwrap().slot = Slot::Tiled;
                                }
                            }
                        }
                    }
                    _ => {
                        let before = session.manager.desktops.clone();
                        let windows: HashSet<_> = session
                            .owners
                            .iter()
                            .map(|(id, owner)| (owner.pid, *id))
                            .collect();
                        session.manager.set_presentation_at(
                            crate::desktop_mode::Mode::Desktop,
                            session.clock,
                            Some(windows.clone()),
                        );
                        session.manager.set_presentation_at(
                            crate::desktop_mode::Mode::Normal,
                            session.clock + Duration::from_millis(100),
                            Some(windows),
                        );
                        session.settle();
                        assert!(
                            session.manager.desktops == before,
                            "Show Desktop reset: seed={seed:#x} step={step}"
                        );
                    }
                }
                session.settle();
                session.check(seed, step, op);
            }
        }
        assert!(operations.iter().all(|n| *n > 0));
        assert!(moved > 0 && refused > 0);
        eprintln!(
            "SEEDED_SESSIONS seeds=64 steps_per_seed=500 operations={operations:?} transfers={moved} refused_safely={refused}"
        );
        let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
    });
}
