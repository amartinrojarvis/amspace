//! End-to-end pan workflows using only the elevated, disposable close fixture.
use super::*;
use crate::ui::diagnostics::{TabPanDelivery, check_tab_pan};
use objc2::rc::Allocated;
use objc2_foundation::{NSString, ns_string};

fn geometry(manager: &Manager, id: u32) -> Result<NSRect, String> {
    let (p, s) = ax::geometry(manager.windows[&id].element.0)
        .ok_or_else(|| format!("Sin geometría AX del fixture {id}"))?;
    Ok(NSRect::new(p, s))
}

fn finish_placements(manager: &mut Manager, expected: &[(u32, NSRect)]) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let candidates: Vec<_> = manager
            .windows
            .iter()
            .filter_map(|(id, w)| ws::bounds(*id).map(|frame| (w.pid, *id, frame)))
            .collect();
        let active = manager.active_areas.clone();
        let mut spaces = HashMap::new();
        manager.animate(&active, &candidates, &mut spaces);
        manager.verify_placements(&active, &candidates, false, &mut spaces);
        if manager.transitions.is_empty()
            && manager.verifications.is_empty()
            && expected.iter().all(|(id, frame)| {
                geometry(manager, *id).is_ok_and(|actual| near(actual, *frame))
                    && ws::bounds(*id).is_some_and(|native| near(native, *frame))
            })
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "La colocación AX/WindowServer no llegó al destino: {expected:?}"
            ));
        }
        super::diagnostics::pump(0.012);
    }
}

fn place_layout(manager: &mut Manager, space: Workspace, area: NSRect) -> Result<(), String> {
    let desktop = &manager.desktops[&space];
    let mut frames = desktop.window_frames(area, 10.);
    frames.extend(hidden_tab_frames(desktop, &frames));
    for (id, target) in &frames {
        manager.transition_from(*id, geometry(manager, *id)?, *target);
    }
    finish_placements(manager, &frames)
}

fn prepare(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
    layout: Layout,
    animated: bool,
) -> Result<(), String> {
    let mut desktop = Desktop::configured(layout, None);
    desktop.reconcile_at(ids, area, 10., None);
    for (target, id) in ids.iter().zip(&ids[1..]) {
        desktop = desktop
            .proposed_drop(*id, DropIntent::Group(*target), area, 10.)
            .ok_or("No se pudo preparar la pila E2E")?;
    }
    prepare_model(manager, space, area, desktop, animated)
}

fn prepare_model(
    manager: &mut Manager,
    space: Workspace,
    area: NSRect,
    desktop: Desktop,
    animated: bool,
) -> Result<(), String> {
    manager.cancel_drag();
    manager.floating.clear();
    manager.inactive_tabs.clear();
    manager.minimum_sizes.clear();
    manager.animated = Some(animated);
    manager.desktops.insert(space, desktop.clone());
    place_layout(manager, space, area)?;
    let frames = desktop.window_frames(area, 10.);
    let used = manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
    manager.trim_overlays(used, 0);
    super::diagnostics::pump(0.03);
    Ok(())
}

fn button(manager: &Manager, id: u32) -> Result<Retained<AnyObject>, String> {
    unsafe {
        let strip = manager
            .strips
            .iter()
            .find(|strip| strip.members.contains(&id))
            .ok_or("Sin tira E2E")?;
        let viewport: Obj = msg_send![&*strip.panel, contentView];
        let document: Obj = msg_send![viewport, documentView];
        let button: Option<Retained<AnyObject>> = msg_send![document, viewWithTag:id as isize];
        button.ok_or_else(|| format!("Sin control E2E del miembro {id}"))
    }
}

fn end_of_tab(button: &AnyObject) -> NSPoint {
    unsafe {
        let bounds: NSRect = msg_send![button, bounds];
        // The selection button excludes the 22 pt close area. Its center is
        // before the painted tab's midpoint, not a drop after that tab.
        let local: NSPoint = msg_send![button, convertPoint:NSPoint::new(bounds.size.width - 2., bounds.size.height / 2.), toView:ptr::null::<AnyObject>()];
        let panel: Obj = msg_send![button, window];
        let screen: NSPoint = msg_send![panel, convertPointToScreen:local];
        NSPoint::new(screen.x, primary_height() - screen.y)
    }
}

fn assert_finished(manager: &Manager) -> Result<(), String> {
    let visible = |panel: &AnyObject| -> bool { unsafe { msg_send![panel, isVisible] } };
    if manager.tab_drag.is_some()
        || manager.preview.iter().any(|panel| visible(panel))
        || manager.badges.iter().any(|badge| visible(&badge.panel))
        || manager
            .float_overlay
            .as_ref()
            .is_some_and(|overlay| visible(&overlay.panel))
        || manager.strips.iter().any(|strip| strip.marker.is_some())
    {
        return Err("El pan dejó una intención, marcador o preview activa".into());
    }
    Ok(())
}

pub(super) fn check(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<(), String> {
    let card = float_drop::frame(area).ok_or("Sin zona superior E2E")?;
    let floating = NSPoint::new(
        card.origin.x + card.size.width / 2.,
        card.origin.y + card.size.height / 2.,
    );
    let outside = NSPoint::new(
        area.origin.x + area.size.width * 0.75,
        area.origin.y + area.size.height * 0.6,
    );
    let mut cases = check_native_boundaries(manager, ids, space, area)?;
    for (index, layout) in Layout::ALL.into_iter().enumerate() {
        // Both transition paths are exercised on real AX windows.
        let animated = index % 2 == 0;
        prepare(manager, ids, space, area, layout, animated)?;
        let before = manager.desktops[&space].clone();
        let original: Vec<_> = ids
            .iter()
            .map(|id| Ok((*id, geometry(manager, *id)?)))
            .collect::<Result<_, String>>()?;
        let dragged = button(manager, ids[0])?;
        let last_button = button(manager, ids[2])?;
        let last = end_of_tab(&last_button);
        check_tab_pan(
            manager,
            &dragged,
            last,
            last,
            TabPanDelivery::Reentrant,
            true,
            |pending| {
                if pending.tab_drag.is_none() || pending.desktops[&space] != before {
                    return Err(
                        "La reordenación se perdió o cambió el modelo antes de soltar".into(),
                    );
                }
                Ok(())
            },
        )?;
        let desktop = &manager.desktops[&space];
        if desktop.tab_strip(ids[0]) != Some((&[ids[1], ids[2], ids[0]][..], 1)) {
            return Err(format!("Reordenación E2E incorrecta en {layout:?}"));
        }
        for (id, frame) in original {
            if !near(geometry(manager, id)?, frame) {
                return Err("Reordenar escribió geometría nativa".into());
            }
        }
        assert_finished(manager)?;
        cases += 1;

        prepare(manager, ids, space, area, layout, animated)?;
        let dragged = button(manager, ids[0])?;
        check_tab_pan(
            manager,
            &dragged,
            outside,
            outside,
            TabPanDelivery::Reentrant,
            true,
            |pending| {
                if pending.tab_drag.is_none() || pending.desktops[&space].tab_role(ids[0]).is_none()
                {
                    return Err("La separación se perdió o se confirmó antes de soltar".into());
                }
                Ok(())
            },
        )?;
        if manager.desktops[&space].tab_role(ids[0]).is_some()
            || manager.desktops[&space].order.len() != 2
            || manager.floating.contains(&ids[0])
        {
            return Err(format!("Separación E2E incorrecta en {layout:?}"));
        }
        place_layout(manager, space, area)?;
        assert_finished(manager)?;
        cases += 1;

        for id in [ids[0], ids[2]] {
            prepare(manager, ids, space, area, layout, animated)?;
            let from = geometry(manager, id)?;
            let expected =
                crate::tiling::floating_frame(from, area).ok_or("Sin destino flotante E2E")?;
            let dragged = button(manager, id)?;
            check_tab_pan(
                manager,
                &dragged,
                floating,
                floating,
                TabPanDelivery::Reentrant,
                true,
                |pending| {
                    if pending
                        .tab_drag
                        .as_ref()
                        .is_none_or(|drag| !drag.float_hover)
                        || pending.floating.contains(&id)
                        || pending.desktops[&space].tab_role(id).is_none()
                    {
                        return Err(
                            "La zona flotante no reconoció el pan o confirmó durante hover".into(),
                        );
                    }
                    if index == 0 && id == ids[0] {
                        let overlay = pending
                            .float_overlay
                            .as_ref()
                            .ok_or("Sin panel flotante E2E")?;
                        let number: isize = unsafe { msg_send![&*overlay.panel, windowNumber] };
                        std::fs::create_dir_all(".cache").map_err(|error| error.to_string())?;
                        let status = std::process::Command::new("/usr/sbin/screencapture")
                            .args([
                                "-x",
                                "-l",
                                &number.to_string(),
                                ".cache/e2e-tab-float-preview.png",
                            ])
                            .status()
                            .map_err(|error| error.to_string())?;
                        if !status.success() {
                            return Err("No se pudo capturar el panel flotante E2E".into());
                        }
                    }
                    Ok(())
                },
            )?;
            if !manager.floating.contains(&id) || manager.desktops[&space].tab_role(id).is_some() {
                return Err(format!("La pestaña {id} no pasó a flotante en {layout:?}"));
            }
            finish_placements(manager, &[(id, expected)])?;
            assert_finished(manager)?;
            let from = geometry(manager, id)?;
            manager.toggle_float_window(id, from, area)?;
            if manager.floating.contains(&id) {
                return Err("La ventana flotante no pudo volver al mosaico".into());
            }
            place_layout(manager, space, area)?;
            if !manager.desktops[&space].order.contains(&id) {
                return Err("La vuelta al mosaico no creó un hueco independiente".into());
            }
            cases += 2;
        }
        println!(
            "PASS E2E: {layout:?}, reordenar/separar reentrantes, flotante activa/inactiva y retorno, animación={animated}"
        );
    }

    for delivery in [
        TabPanDelivery::Cancelled,
        TabPanDelivery::Expired,
        TabPanDelivery::Paused,
    ] {
        prepare(manager, ids, space, area, Layout::Spiral, false)?;
        let baseline = manager.desktops[&space].clone();
        let dragged = button(manager, ids[0])?;
        check_tab_pan(
            manager,
            &dragged,
            floating,
            floating,
            delivery,
            true,
            |pending| {
                if pending.tab_drag.is_some() || pending.desktops[&space] != baseline {
                    return Err(format!(
                        "{delivery:?} no canceló la intención sin mutar el modelo"
                    ));
                }
                Ok(())
            },
        )?;
        if manager.desktops[&space] != baseline || !manager.floating.is_empty() {
            return Err(format!("{delivery:?} confirmó un pan invalidado"));
        }
        assert_finished(manager)?;
        cases += 1;
    }

    // A mouse-up at a new location must use that event, not stale hover.
    for (hover, release, expect_float, enabled) in [
        (floating, outside, false, true),
        (outside, floating, true, true),
        (floating, floating, false, false),
    ] {
        prepare(manager, ids, space, area, Layout::Spiral, false)?;
        let dragged = button(manager, ids[0])?;
        check_tab_pan(
            manager,
            &dragged,
            hover,
            release,
            TabPanDelivery::Immediate,
            enabled,
            |_| Ok(()),
        )?;
        if manager.floating.contains(&ids[0]) != expect_float
            || (enabled && manager.desktops[&space].tab_role(ids[0]).is_some())
        {
            return Err("El pan ignoró el punto final o la desactivación del gesto".into());
        }
        assert_finished(manager)?;
        cases += 1;
    }
    prepare(manager, ids, space, area, Layout::Spiral, false)?;
    let dragged = button(manager, ids[0])?;
    let last_button = button(manager, ids[2])?;
    let last = end_of_tab(&last_button);
    check_tab_pan(
        manager,
        &dragged,
        floating,
        last,
        TabPanDelivery::Reentrant,
        true,
        |_| Ok(()),
    )?;
    if !manager.floating.is_empty()
        || manager.desktops[&space].tab_strip(ids[0]) != Some((&[ids[1], ids[2], ids[0]][..], 1))
    {
        return Err("Volver a la barra confirmó flotante o perdió la reordenación".into());
    }
    assert_finished(manager)?;
    cases += 1;

    prepare(manager, ids, space, area, Layout::Spiral, false)?;
    let dragged = button(manager, ids[0])?;
    check_tab_pan(
        manager,
        &dragged,
        floating,
        floating,
        TabPanDelivery::Burst,
        true,
        |pending| {
            if pending
                .tab_drag
                .as_ref()
                .is_none_or(|drag| !drag.float_hover)
            {
                return Err("La ráfaga de movimiento perdió el pan flotante".into());
            }
            Ok(())
        },
    )?;
    if !manager.floating.contains(&ids[0]) || manager.desktops[&space].tab_role(ids[0]).is_some() {
        return Err("La ráfaga de movimiento perdió la soltada flotante".into());
    }
    assert_finished(manager)?;
    cases += 1;

    prepare(manager, ids, space, area, Layout::Spiral, false)?;
    let baseline = manager.desktops[&space].clone();
    let dragged = button(manager, ids[0])?;
    check_tab_pan(
        manager,
        &dragged,
        floating,
        floating,
        TabPanDelivery::ExpiredEnd,
        true,
        |pending| {
            if pending
                .tab_drag
                .as_ref()
                .is_none_or(|drag| !drag.float_hover)
            {
                return Err("El pan no llegó a hover antes de caducar su fin".into());
            }
            Ok(())
        },
    )?;
    if manager.desktops[&space] != baseline || !manager.floating.is_empty() {
        return Err("Una soltada caducada confirmó la intención flotante".into());
    }
    assert_finished(manager)?;
    cases += 1;

    prepare(manager, ids, space, area, Layout::Spiral, false)?;
    let baseline = manager.desktops[&space].clone();
    let dragged = button(manager, ids[0])?;
    check_tab_pan(
        manager,
        &dragged,
        floating,
        floating,
        TabPanDelivery::Reentrant,
        true,
        |pending| {
            if pending.tab_drag.is_none() {
                return Err("El pan no empezó antes de retirar su control".into());
            }
            unsafe {
                let _: () = msg_send![&*dragged, removeFromSuperview];
            }
            Ok(())
        },
    )?;
    if manager.desktops[&space] != baseline || !manager.floating.is_empty() {
        return Err("Un control retirado confirmó su soltada pendiente".into());
    }
    assert_finished(manager)?;
    manager.trim_overlays(0, 0);
    cases += 1;

    cases += check_destinations(manager, ids, space, area)?;

    prepare(manager, ids, space, area, Layout::Spiral, false)?;
    println!(
        "PASS E2E: {cases} escenarios de arrastre/retorno (pan y fronteras nativas) sobre ventanas AX reales; cancelación, caducidad, pausa, punto final y zona desactivada; ninguna entrada global ni ventana ajena."
    );
    Ok(())
}

fn check_destinations(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<usize, String> {
    let mut cases = 0;
    let point = |frame: NSRect, fraction: f64| {
        // Off-center x avoids the floating card on a full-width top row.
        NSPoint::new(
            frame.origin.x + frame.size.width * 0.25,
            frame.origin.y + frame.size.height * fraction,
        )
    };
    for (index, layout) in Layout::ALL.into_iter().enumerate() {
        let mut baseline = Desktop::configured(layout, None);
        baseline.reconcile_at(ids, area, 10., None);
        let baseline = baseline
            .proposed_drop(ids[1], DropIntent::Group(ids[0]), area, 10.)
            .ok_or("No se pudo preparar el origen con pestañas y otro hueco")?;
        let animated = index % 2 == 0;
        for id in [ids[0], ids[1]] {
            let (virtual_layout, targets) = tab_drag_layout(&baseline, id, area);
            let virtual_layout = virtual_layout.ok_or("Sin distribución virtual E2E")?;
            for (target, frame) in &targets {
                prepare_model(manager, space, area, baseline.clone(), animated)?;
                let dragged = button(manager, id)?;
                let expected = virtual_layout
                    .proposed_drop(id, DropIntent::Window(*target, Zone::Center), area, 10.)
                    .ok_or("El destino virtual E2E no admite el miembro")?;
                let destination = point(*frame, 0.7);
                check_tab_pan(
                    manager,
                    &dragged,
                    destination,
                    destination,
                    TabPanDelivery::Reentrant,
                    true,
                    |pending| {
                        let drag = pending
                            .tab_drag
                            .as_ref()
                            .ok_or("El pan no se activó al primer intento")?;
                        if drag.proposed.as_ref() != Some(&expected)
                            || pending.desktops[&space] != baseline
                        {
                            return Err(
                                "La preview perdió el destino elegido o confirmó antes de soltar"
                                    .into(),
                            );
                        }
                        if index == 0 && id == ids[0] && *target == targets[0].0 {
                            let panel = pending
                                .preview
                                .first()
                                .ok_or("Sin preview nativa de posición")?;
                            let badge = pending
                                .badges
                                .first()
                                .ok_or("Sin indicador nativo del destino")?;
                            for (panel, path) in [
                                (panel, ".cache/e2e-tab-target-preview.png"),
                                (&badge.panel, ".cache/e2e-tab-target-hint.png"),
                            ] {
                                let number: isize = unsafe { msg_send![&**panel, windowNumber] };
                                let status = std::process::Command::new("/usr/sbin/screencapture")
                                    .args(["-x", "-l", &number.to_string(), path])
                                    .status()
                                    .map_err(|error| error.to_string())?;
                                if !status.success() {
                                    return Err("No se pudo capturar el destino nativo".into());
                                }
                            }
                        }
                        Ok(())
                    },
                )?;
                if manager.desktops[&space] != expected || manager.floating.contains(&id) {
                    return Err(format!(
                        "El destino {target} no recibió la pestaña {id} en {layout:?}"
                    ));
                }
                place_layout(manager, space, area)?;
                assert_finished(manager)?;
                cases += 1;
            }

            // A tab can join the independent destination without an intermediate
            // detach/float, and the original source must retain its other member.
            prepare_model(manager, space, area, baseline.clone(), animated)?;
            let dragged = button(manager, id)?;
            let frame = targets
                .iter()
                .find(|(target, _)| *target == ids[2])
                .ok_or("Sin destino independiente E2E")?
                .1;
            let expected = baseline
                .proposed_drop(id, DropIntent::Group(ids[2]), area, 10.)
                .ok_or("El grupo de destino E2E no admite el miembro")?;
            check_tab_pan(
                manager,
                &dragged,
                point(frame, 0.15),
                point(frame, 0.15),
                TabPanDelivery::Reentrant,
                true,
                |pending| {
                    if pending
                        .tab_drag
                        .as_ref()
                        .and_then(|drag| drag.proposed.as_ref())
                        != Some(&expected)
                        || pending.desktops[&space] != baseline
                    {
                        return Err("Apilar desde pestaña no mostró su propuesta sin mutar".into());
                    }
                    Ok(())
                },
            )?;
            if manager.desktops[&space] != expected {
                return Err("La pestaña no se apiló en el grupo elegido".into());
            }
            place_layout(manager, space, area)?;
            assert_finished(manager)?;
            cases += 1;

            // Ended must choose the last tile, not the tile highlighted in hover.
            prepare_model(manager, space, area, baseline.clone(), animated)?;
            let dragged = button(manager, id)?;
            let first = targets.first().unwrap();
            let last = targets.last().unwrap();
            let expected = virtual_layout
                .proposed_drop(id, DropIntent::Window(last.0, Zone::Center), area, 10.)
                .ok_or("Sin destino final E2E")?;
            check_tab_pan(
                manager,
                &dragged,
                point(first.1, 0.7),
                point(last.1, 0.7),
                TabPanDelivery::Immediate,
                true,
                |_| Ok(()),
            )?;
            if manager.desktops[&space] != expected {
                return Err("Ended confirmó el hueco previo en vez del punto final".into());
            }
            place_layout(manager, space, area)?;
            assert_finished(manager)?;
            cases += 1;
        }

        prepare_model(manager, space, area, baseline.clone(), animated)?;
        let dragged = button(manager, ids[0])?;
        let (_, targets) = tab_drag_layout(&baseline, ids[0], area);
        let destination = point(targets.last().unwrap().1, 0.7);
        let outside = NSPoint::new(area.origin.x + area.size.width + 30., destination.y);
        check_tab_pan(
            manager,
            &dragged,
            destination,
            outside,
            TabPanDelivery::Reentrant,
            true,
            |_| Ok(()),
        )?;
        if manager.desktops[&space] != baseline || !manager.floating.is_empty() {
            return Err("Soltar fuera de la pantalla de origen confirmó un tile".into());
        }
        assert_finished(manager)?;
        cases += 1;

        for pending_drop in [false, true] {
            prepare_model(manager, space, area, baseline.clone(), animated)?;
            if pending_drop {
                manager.pending_drop = Some(PendingDrop {
                    id: ids[2],
                    source: space,
                    destination: space,
                    intent: Some(DropIntent::Group(ids[1])),
                    expires: Instant::now() + Duration::from_millis(700),
                });
            } else {
                let frame = baseline
                    .frames(area, 10.)
                    .into_iter()
                    .find_map(|(id, frame)| (id == ids[2]).then_some(frame))
                    .unwrap();
                manager.drag = Some(Drag {
                    id: ids[2],
                    pid: manager.windows[&ids[2]].pid,
                    was_floating: false,
                    space,
                    start: frame,
                    point: NSPoint::new(frame.origin.x + 20., frame.origin.y + 20.),
                    drop: None,
                    candidate: None,
                    candidate_since: Instant::now(),
                    moved: false,
                    resized: false,
                    resize_gesture: false,
                    baseline: baseline.clone(),
                    area,
                    previews: DropPreviewCache::default(),
                    native_captured: true,
                    float_drop_enabled: true,
                    float_target: None,
                    float_hover: false,
                });
                manager.mouse_release_pending = Some(Instant::now());
            }
            let dragged = button(manager, ids[0])?;
            check_tab_pan(
                manager,
                &dragged,
                destination,
                destination,
                TabPanDelivery::Reentrant,
                true,
                |pending| {
                    if pending.pending_drop.is_some()
                        || pending.drag.is_some()
                        || pending.mouse_release_pending.is_some()
                        || pending.tab_drag.is_none()
                    {
                        return Err(
                            "Un gesto nativo anterior bloqueó el primer pan de pestaña".into()
                        );
                    }
                    Ok(())
                },
            )?;
            if manager.desktops[&space].tab_role(ids[0]).is_some() || manager.pending_drop.is_some()
            {
                return Err("El gesto pendiente sustituyó el nuevo destino de la pestaña".into());
            }
            place_layout(manager, space, area)?;
            assert_finished(manager)?;
            cases += 1;
        }
        println!(
            "PASS E2E: {layout:?}, todos los huecos finales, activa/oculta, apilar, punto final, salida y gesto anterior"
        );
    }
    Ok(cases)
}

fn check_native_boundaries(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<usize, String> {
    let suite = NSString::from_str(&format!(".amspace.native.grab.{}", std::process::id()));
    let defaults: Allocated<AnyObject> = unsafe { msg_send![class!(NSUserDefaults), alloc] };
    let defaults: Retained<AnyObject> = unsafe { msg_send![defaults, initWithSuiteName:&*suite] };
    unsafe {
        let _: () = msg_send![&*defaults, setBool:false, forKey:ns_string!("tile.shake")];
        let _: () = msg_send![&*defaults, setBool:false, forKey:ns_string!("tile.animate")];
    }
    let result = (|| {
        let defaults = Retained::as_ptr(&defaults) as Obj;
        let mut cases = 0;
        for layout in Layout::ALL {
            for grouped in [false, true] {
                let mut baseline = Desktop::configured(layout, None);
                baseline.reconcile_at(ids, area, 10., None);
                prepare_model(manager, space, area, baseline, false)?;
                let id = ids[0];
                let frame = ws::bounds(id).ok_or("Sin bounds nativos del fixture")?;
                if !near(frame, geometry(manager, id)?) {
                    return Err("El grab no partió de geometría nativa confirmada".into());
                }
                let grab = crate::tile_mouse::Grab {
                    id,
                    pid: manager.windows[&id].pid,
                    frame,
                };
                let down = NSPoint::new(
                    frame.origin.x + frame.size.width * 0.4,
                    frame.origin.y + 40.,
                );
                let target = manager.desktops[&space]
                    .frames(area, 10.)
                    .into_iter()
                    .find_map(|(other, frame)| (other == ids[2]).then_some(frame))
                    .unwrap();
                let release = NSPoint::new(
                    target.origin.x + target.size.width / 2.,
                    target.origin.y + target.size.height * if grouped { 0.15 } else { 0.7 },
                );
                let mut moved = frame;
                moved.origin.x = (frame.origin.x + release.x - down.x).clamp(
                    area.origin.x,
                    area.origin.x + area.size.width - frame.size.width,
                );
                moved.origin.y = (frame.origin.y + release.y - down.y).clamp(
                    area.origin.y,
                    area.origin.y + area.size.height - frame.size.height,
                );
                if !ax::set_frame(manager.windows[&id].element.0, moved) {
                    return Err("AX rechazó el movimiento desechable anterior a la entrega".into());
                }
                finish_placements(manager, &[(id, moved)])?;
                // The boundary arrived while APP was borrowed. Deliver it only
                // after the native window has moved away from its grab point.
                manager.last_area_update = Some(Instant::now());
                manager.mouse_event(defaults, true, down, Some(grab));
                let captured = manager
                    .drag
                    .as_ref()
                    .ok_or("El primer down diferido perdió la ventana que ya se había movido")?;
                if captured.id != id
                    || captured.start != frame
                    || !captured.native_captured
                    || captured.float_drop_enabled
                {
                    return Err(
                        "El down usó identidad/bounds tardíos o ignoró la zona desactivada".into(),
                    );
                }
                let native_frame = ws::bounds(id);
                let classification = (captured.moved, captured.resized, captured.resize_gesture);
                let intent = if grouped {
                    DropIntent::Group(ids[2])
                } else {
                    DropIntent::Window(ids[2], Zone::Center)
                };
                let mut expected = manager.desktops[&space]
                    .proposed_drop(id, intent, area, 10.)
                    .ok_or("No se pudo calcular el intercambio/apilado nativo esperado")?;
                expected.fit(area, 10.);
                if !manager.defer_native_release(None, false, Instant::now()) {
                    return Err("El reloj consumió el arrastre antes del soltado nativo".into());
                }
                manager.mouse_event(defaults, false, release, None);
                if manager.desktops[&space] != expected
                    || manager.drag.is_some()
                    || manager.pending_drop.is_some()
                {
                    return Err(format!(
                        "{layout:?}: primer soltado no confirmó {intent:?}; inicio={frame:?}, solicitado={moved:?}, nativo={native_frame:?}, clasificación={classification:?}, display={:?}, actual={:?}, esperado={:?}, error={:?}",
                        point_display(release),
                        manager.desktops[&space].frames(area, 10.),
                        expected.frames(area, 10.),
                        manager.interaction_error,
                    ));
                }
                place_layout(manager, space, area)?;
                assert_finished(manager)?;
                cases += 1;
            }
            println!(
                "PASS E2E: {layout:?}, intercambio y apilado al primer down/up diferido; flotante desactivado"
            );
        }
        Ok(cases)
    })();
    unsafe {
        let _: () = msg_send![&*defaults, removePersistentDomainForName:&*suite];
    }
    result
}
