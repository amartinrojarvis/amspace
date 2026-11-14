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
        float_drop_enabled: true,
        float_target: None,
        float_hover: false,
    }
}

fn enter_zone(drag: &mut Drag) -> NSPoint {
    let zone = float_drop::frame(drag.area).unwrap();
    let point = NSPoint::new(zone.origin.x + zone.size.width / 2., zone.origin.y + 20.);
    let mut moved = drag.start;
    moved.origin.x += 50.;
    drag.observe(moved, point, Some(drag.space));
    assert_eq!(
        drag.update_float_zone(point, Some(drag.space), Some((drag.space, drag.area)), true),
        Some((zone, true))
    );
    point
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
fn hover_reserves_the_zone_but_commits_only_on_explicit_release() {
    let mut drag = gesture(Instant::now());
    let baseline = drag.baseline.clone();
    enter_zone(&mut drag);
    assert!(drag.drop.is_none() && drag.candidate.is_none());
    assert!(drag.baseline == baseline);
    assert!(!drag.float_released(false));
    assert!(drag.float_released(true));
    let outside = NSPoint::new(drag.area.origin.x + 10., drag.area.origin.y + 200.);
    assert_eq!(
        drag.update_float_zone(
            outside,
            Some(drag.space),
            Some((drag.space, drag.area)),
            true
        )
        .map(|(_, highlighted)| highlighted),
        Some(false)
    );
    assert!(
        !drag.float_released(true),
        "Leaving must not leave a sticky toggle"
    );
    enter_zone(&mut drag);
    assert!(
        drag.float_released(true),
        "Re-entering may accept a new release"
    );
}

#[test]
fn content_motion_resize_or_missing_native_down_cannot_show_a_zone() {
    for mode in 0..3 {
        let mut drag = gesture(Instant::now());
        let mut frame = drag.start;
        let point = NSPoint::new(drag.point.x + 30., drag.point.y);
        match mode {
            1 => frame.size.width += 30.,
            2 => {
                frame.origin.x += 30.;
                drag.float_drop_enabled = false;
            }
            _ => {}
        }
        drag.observe(frame, point, Some(drag.space));
        assert!(
            drag.update_float_zone(point, Some(drag.space), Some((drag.space, drag.area)), true)
                .is_none()
        );
        assert!(!drag.float_released(true));
    }
}

#[test]
fn disabled_zone_cannot_rearm_until_a_new_mouse_down() {
    let mut drag = gesture(Instant::now());
    let point = enter_zone(&mut drag);
    assert!(
        drag.update_float_zone(
            point,
            Some(drag.space),
            Some((drag.space, drag.area)),
            false
        )
        .is_none()
    );
    assert!(!drag.float_released(true));
    assert!(
        drag.update_float_zone(point, Some(drag.space), Some((drag.space, drag.area)), true)
            .is_none()
    );
}

#[test]
fn zone_follows_the_pointer_display_and_rejects_missing_or_mismatched_targets() {
    let mut drag = gesture(Instant::now());
    enter_zone(&mut drag);
    let target = ((2, 9), rect(-1000., -500., 1000., 1500.));
    let zone = float_drop::frame(target.1).unwrap();
    let point = NSPoint::new(zone.origin.x + 30., zone.origin.y + 30.);
    assert_eq!(
        drag.update_float_zone(point, Some(target.0), Some(target), true),
        Some((zone, true))
    );
    assert_eq!(drag.float_target, Some(target));
    assert!(drag.float_released(true));
    assert!(
        drag.update_float_zone(point, Some(drag.space), Some(target), true)
            .is_none()
    );
    assert!(!drag.float_released(true));
    assert!(drag.update_float_zone(point, None, None, true).is_none());
    assert!(!drag.float_released(true));
}

#[test]
fn release_rejects_reused_identity_changed_membership_area_and_model() {
    let mut drag = gesture(Instant::now());
    enter_zone(&mut drag);
    let mut manager = manager_for(&drag);
    assert!(manager.float_drag_matches(&drag));
    manager.windows.get_mut(&drag.id).unwrap().pid += 1;
    assert!(!manager.float_drag_matches(&drag));
    manager.windows.get_mut(&drag.id).unwrap().pid = drag.pid;
    manager.windows.get_mut(&drag.id).unwrap().space = (2, 7);
    assert!(!manager.float_drag_matches(&drag));
    manager.windows.get_mut(&drag.id).unwrap().space = drag.space;
    manager.floating.insert(drag.id);
    assert!(!manager.float_drag_matches(&drag));
    drag.was_floating = true;
    assert!(manager.float_drag_matches(&drag));
    manager.active_areas[0].1.size.width -= 20.;
    assert!(!manager.float_drag_matches(&drag));
    manager.active_areas[0].1 = drag.area;
    manager
        .desktops
        .get_mut(&drag.space)
        .unwrap()
        .order
        .reverse();
    assert!(!manager.float_drag_matches(&drag));
    manager.desktops.insert(drag.space, drag.baseline.clone());
    assert!(manager.float_drag_matches(&drag));
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

#[test]
fn cross_display_float_preserves_destination_and_returns_only_to_the_chosen_layout() {
    let drag = gesture(Instant::now());
    let mut manager = manager_for(&drag);
    let destination = (2, 9);
    let target_area = rect(-1100., -400., 1000., 1500.);
    let mut target = Desktop::default();
    target.reconcile_at(&[8, 9], target_area, 10., None);
    manager.desktops.insert(destination, target.clone());
    manager.active_areas.push((destination, target_area));
    let third = (3, 10);
    manager.desktops.insert(third, target.clone());
    let comfortable =
        crate::tiling::floating_frame(rect(-1000., -200., 900., 900.), target_area).unwrap();
    manager
        .toggle_float_at(drag.id, comfortable, destination, target_area)
        .unwrap();
    assert!(manager.floating.contains(&drag.id));
    assert_eq!(manager.windows[&drag.id].space, destination);
    assert_eq!(manager.windows[&drag.id].original_area, target_area);
    assert_eq!(manager.desktops[&drag.space].order, vec![2]);
    assert!(manager.desktops[&destination] == target);
    assert!(manager.desktops[&third] == target);
    assert!(manager.transitions.is_empty());
    manager
        .minimum_sizes
        .insert(8, objc2_foundation::NSSize::new(2000., 2000.));
    let before = manager.desktops[&destination].clone();
    assert!(
        manager
            .toggle_float_at(drag.id, comfortable, destination, target_area)
            .is_err()
    );
    assert!(manager.floating.contains(&drag.id));
    assert!(manager.desktops[&destination] == before);
    assert_eq!(manager.windows[&drag.id].space, destination);
    manager.minimum_sizes.clear();
    manager
        .toggle_float_at(drag.id, comfortable, destination, target_area)
        .unwrap();
    assert!(!manager.floating.contains(&drag.id));
    assert!(manager.desktops[&destination].order.contains(&drag.id));
    assert_eq!(manager.desktops[&drag.space].order, vec![2]);
    assert!(manager.desktops[&third] == target);
}

#[test]
fn floating_a_group_member_promotes_its_sibling_without_losing_other_windows() {
    let drag = gesture(Instant::now());
    let mut manager = manager_for(&drag);
    let grouped = arrival_proposal(
        &manager.desktops[&drag.space],
        drag.id,
        Some(DropIntent::Group(2)),
        drag.area,
        Some(&manager.minimum_sizes),
    )
    .unwrap();
    manager.desktops.insert(drag.space, grouped);
    let comfortable = rect(350., 130., 900., 700.);
    manager
        .toggle_float_window(drag.id, comfortable, drag.area)
        .unwrap();
    assert!(manager.floating.contains(&drag.id));
    assert_eq!(manager.desktops[&drag.space].order, vec![2]);
    let desktop = &manager.desktops[&drag.space];
    assert_eq!(
        desktop.window_frames(drag.area, 10.),
        desktop.frames(drag.area, 10.),
        "The survivor must reclaim the tab header and have no hidden sibling"
    );
}

#[test]
fn polling_release_keeps_the_float_proposal_until_the_native_release_arrives() {
    let now = Instant::now();
    let mut drag = gesture(now);
    enter_zone(&mut drag);
    let mut manager = manager_for(&drag);
    let key = drag.space;
    let area = drag.area;
    manager.drag = Some(drag);
    manager.pressed = true;
    for delay in [0, 16, 80, 200] {
        assert!(manager.defer_native_release(None, false, now + Duration::from_millis(delay)));
        assert!(manager.drag.as_ref().unwrap().float_released(true));
        assert!(!manager.floating.contains(&1));
    }
    assert!(!manager.defer_native_release(Some(false), false, now + Duration::from_millis(250)));
    let drag = manager.drag.take().unwrap();
    assert!(drag.float_released(true));
    manager
        .toggle_float_at(drag.id, drag.start, key, area)
        .unwrap();
    assert!(manager.floating.contains(&1));
    assert!(!manager.desktops[&key].order.contains(&1));
    assert!(manager.desktops[&key].order.contains(&2));
}

#[test]
fn the_native_release_position_wins_over_the_last_hover() {
    let now = Instant::now();
    let mut drag = gesture(now);
    enter_zone(&mut drag);
    let mut manager = manager_for(&drag);
    manager.drag = Some(drag);
    manager.pressed = true;
    assert!(manager.defer_native_release(None, false, now));
    assert!(!manager.defer_native_release(Some(false), false, now + Duration::from_millis(30)));
    let drag = manager.drag.as_mut().unwrap();
    let outside = NSPoint::new(50., 300.);
    drag.update_float_zone(
        outside,
        Some(drag.space),
        Some((drag.space, drag.area)),
        true,
    );
    assert!(!drag.float_released(true));
    assert!(!manager.floating.contains(&1));
}

#[test]
fn a_missing_or_expired_release_cancels_without_toggling_or_rearming() {
    for explicit in [None, Some(false)] {
        let now = Instant::now();
        let mut drag = gesture(now);
        enter_zone(&mut drag);
        let baseline = drag.baseline.clone();
        let key = drag.space;
        let mut manager = manager_for(&drag);
        manager.drag = Some(drag);
        manager.pressed = true;
        assert!(manager.defer_native_release(None, false, now));
        assert!(manager.defer_native_release(explicit, false, now + Duration::from_millis(350)));
        assert!(manager.drag.is_none());
        assert!(!manager.pressed);
        assert!(manager.mouse_release_pending.is_none());
        assert!(manager.desktops[&key] == baseline);
        assert!(!manager.floating.contains(&1));
        assert!(!manager.defer_native_release(
            Some(false),
            false,
            now + Duration::from_millis(351)
        ));
        assert!(manager.drag.is_none());
    }
}

#[test]
fn a_new_press_cannot_complete_the_previous_hover_or_inherit_a_polled_capture() {
    for opted_in in [false, true] {
        let now = Instant::now();
        let mut drag = gesture(now);
        enter_zone(&mut drag);
        drag.float_drop_enabled = opted_in;
        let mut manager = manager_for(&drag);
        manager.drag = Some(drag);
        manager.pressed = true;
        if opted_in {
            assert!(manager.defer_native_release(None, false, now));
        }
        assert!(!manager.defer_native_release(Some(true), true, now + Duration::from_millis(20)));
        assert!(manager.drag.is_none());
        assert!(manager.gesture_candidates.is_empty());
        assert!(
            !manager.pressed,
            "The native press must be allowed to capture its own grab point"
        );
        assert!(manager.mouse_release_pending.is_none());
        assert!(!manager.floating.contains(&1));
    }
}

#[test]
fn a_candidate_only_gesture_waits_for_native_release_before_resolving() {
    let now = Instant::now();
    let drag = gesture(now);
    let area = drag.area;
    let key = drag.space;
    let mut manager = manager_for(&drag);
    manager.gesture_candidates.push(drag);
    manager.pressed = true;
    assert!(manager.defer_native_release(None, false, now));
    assert!(!manager.defer_native_release(Some(false), false, now + Duration::from_millis(20)));
    let card = float_drop::frame(area).unwrap();
    let point = NSPoint::new(card.origin.x + 30., card.origin.y + 30.);
    resolve_gesture(
        &mut manager.drag,
        &mut manager.gesture_candidates,
        point,
        Some(key),
        |_| Some(rect(60., 45., 1500., 800.)),
    );
    let drag = manager.drag.as_mut().unwrap();
    drag.update_float_zone(point, Some(key), Some((key, area)), true);
    assert!(drag.float_released(true));
}
