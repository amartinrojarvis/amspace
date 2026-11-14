use super::*;

fn gesture(now: Instant) -> Drag {
    let area = rect(0., 30., 1600., 900.);
    let mut baseline = Desktop::default();
    baseline.reconcile_at(&[1, 2], area, 10., None);
    let start = rect(10., 40., 1500., 800.);
    Drag {
        id: 1,
        pid: std::process::id() as i32,
        was_floating: false,
        space: (1, 7),
        start,
        point: NSPoint::new(200., 45.),
        drop: Some(((1, 7), DropIntent::Group(2))),
        candidate: Some(((1, 7), DropIntent::Group(2))),
        candidate_since: now,
        moved: false,
        resized: false,
        resize_gesture: true,
        baseline,
        area,
        previews: Default::default(),
        shake: Some(crate::shake::Shake::new(start.origin, now)),
        shake_armed: false,
        pulse_until: None,
    }
}

fn agitate(drag: &mut Drag, now: Instant) {
    for (index, dx) in [20., -20., 20., -20., 20., -20.].into_iter().enumerate() {
        let mut frame = drag.start;
        frame.origin.x += dx;
        drag.observe_shake(
            frame,
            NSPoint::new(drag.point.x + dx, drag.point.y),
            Some(drag.space),
            true,
            now + Duration::from_millis((index as u64 + 1) * 40),
        );
    }
}

fn manager_for(drag: &Drag) -> Manager {
    let mut manager = Manager::default();
    manager.windows.insert(
        drag.id,
        Managed {
            pid: drag.pid,
            space: drag.space,
            original: drag.start,
            original_area: drag.area,
            element: ax::application(std::process::id() as i32).unwrap(),
        },
    );
    manager.desktops.insert(drag.space, drag.baseline.clone());
    manager.active_areas.push((drag.space, drag.area));
    manager
}

#[test]
fn recognition_arms_one_release_without_changing_layout_or_extending_pulse() {
    let now = Instant::now();
    let mut drag = gesture(now);
    let baseline = drag.baseline.clone();
    agitate(&mut drag, now);
    assert!(drag.shake_armed);
    assert!(drag.drop.is_none() && drag.candidate.is_none());
    assert!(drag.baseline == baseline);
    assert!(!drag.shake_released(false, Some(drag.space)));
    assert!(!drag.shake_released(true, Some((2, 7))));
    assert!(drag.shake_released(true, Some(drag.space)));
    let until = drag.pulse_until.unwrap();
    agitate(&mut drag, now + Duration::from_millis(300));
    assert_eq!(drag.pulse_until, Some(until));
    assert!(until < now + Duration::from_millis(600));
}

#[test]
fn content_motion_resize_and_missing_native_down_do_not_arm() {
    let now = Instant::now();
    for mode in 0..3 {
        let mut drag = gesture(now);
        if mode == 2 {
            drag.shake = None;
        }
        for index in 1..=1000 {
            let dx = if index % 2 == 0 { 20. } else { -20. };
            let mut frame = drag.start;
            match mode {
                1 => frame.size.width += dx,
                2 => frame.origin.x += dx,
                _ => {}
            }
            drag.observe_shake(
                frame,
                NSPoint::new(drag.point.x + dx, drag.point.y),
                Some(drag.space),
                true,
                now + Duration::from_millis(index * 40),
            );
            assert!(!drag.shake_armed);
        }
        assert!(!drag.shake_released(true, Some(drag.space)));
    }
}

#[test]
fn disabling_changing_space_or_resizing_cancels_without_rearming_same_press() {
    let now = Instant::now();
    for reason in 0..3 {
        let mut drag = gesture(now);
        agitate(&mut drag, now);
        assert!(drag.shake_armed);
        let mut frame = drag.start;
        if reason == 2 {
            frame.size.height += 20.;
        }
        let space = if reason == 1 { (2, 7) } else { drag.space };
        drag.observe_shake(
            frame,
            drag.point,
            Some(space),
            reason != 0,
            now + Duration::from_millis(300),
        );
        assert!(drag.shake.is_none() && drag.pulse_until.is_none());
        assert!(!drag.shake_released(true, Some(drag.space)));
        agitate(&mut drag, now + Duration::from_millis(350));
        assert!(!drag.shake_armed);
    }
}

#[test]
fn release_rejects_reused_identity_changed_membership_area_and_model() {
    let mut drag = gesture(Instant::now());
    agitate(&mut drag, Instant::now());
    let mut manager = manager_for(&drag);
    assert!(manager.shake_matches(&drag));
    manager.windows.get_mut(&drag.id).unwrap().pid += 1;
    assert!(!manager.shake_matches(&drag));
    manager.windows.get_mut(&drag.id).unwrap().pid = drag.pid;
    manager.windows.get_mut(&drag.id).unwrap().space = (2, 7);
    assert!(!manager.shake_matches(&drag));
    manager.windows.get_mut(&drag.id).unwrap().space = drag.space;
    manager.floating.insert(drag.id);
    assert!(!manager.shake_matches(&drag));
    drag.was_floating = true;
    assert!(manager.shake_matches(&drag));
    manager.active_areas[0].1.size.width -= 20.;
    assert!(!manager.shake_matches(&drag));
    manager.active_areas[0].1 = drag.area;
    manager
        .desktops
        .get_mut(&drag.space)
        .unwrap()
        .order
        .reverse();
    assert!(!manager.shake_matches(&drag));
    manager.desktops.insert(drag.space, drag.baseline.clone());
    assert!(manager.shake_matches(&drag));
}

#[test]
fn float_and_return_preserve_identity_and_reject_an_overfull_tile_atomically() {
    let drag = gesture(Instant::now());
    let mut manager = manager_for(&drag);
    // Already at the comfortable size: no AX writes or geometry session is
    // necessary to prove the classification/model transaction in either direction.
    let comfortable = rect(350., 130., 900., 700.);
    manager
        .toggle_float_window(drag.id, comfortable, drag.area)
        .unwrap();
    assert!(manager.floating.contains(&drag.id));
    assert_eq!(manager.desktops[&drag.space].order, vec![2]);
    assert_eq!(manager.windows[&drag.id].original, comfortable);
    assert!(manager.transitions.is_empty());
    manager
        .minimum_sizes
        .insert(2, objc2_foundation::NSSize::new(2000., 1000.));
    let before = manager.desktops[&drag.space].clone();
    let released = rect(80., 90., 900., 700.);
    assert!(
        manager
            .toggle_float_window(drag.id, released, drag.area)
            .is_err()
    );
    assert!(manager.floating.contains(&drag.id));
    assert!(manager.desktops[&drag.space] == before);
    assert_eq!(manager.windows[&drag.id].original, comfortable);
    manager.minimum_sizes.clear();
    manager
        .toggle_float_window(drag.id, released, drag.area)
        .unwrap();
    assert!(!manager.floating.contains(&drag.id));
    assert_eq!(manager.desktops[&drag.space].order, vec![2, 1]);
    assert_eq!(manager.windows[&drag.id].original, released);
    assert!(manager.dirty);
}

#[test]
fn active_gesture_keeps_frame_cadence_even_when_reconciliation_is_dirty() {
    let mut manager = Manager::default();
    assert_eq!(manager.tick_interval(), 0.25);
    manager.drag = Some(gesture(Instant::now()));
    manager.dirty = true;
    assert_eq!(manager.tick_interval(), 1. / 60.);
    manager.drag = None;
    manager.dirty = false;
    assert_eq!(manager.tick_interval(), 0.25);
}
