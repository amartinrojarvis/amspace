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
        // Match the live tick: a snapshot taken before an animation write
        // cannot verify the placement created by that same write.
        manager.verify_placements(&active, &candidates, false, &mut spaces);
        manager.animate(&active, &candidates, &mut spaces);
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
    let frames = desktop.window_frames(area, 10.);
    manager.desktops.insert(space, desktop);
    place_layout(manager, space, area)?;
    let used = manager.draw_strips(space, &frames, primary_height(), 0, false);
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
        if index == 0 {
            let frames = manager.desktops[&space].window_frames(area, 10.);
            let primary = primary_height();
            for _ in 0..12 {
                manager.draw_strips(space, &frames, primary, 0, false);
                if !std::ptr::eq(&*dragged, &*button(manager, ids[0])?) {
                    return Err("Reconciliar una pestaña sin cambios sustituyó su control.".into());
                }
            }
            println!(
                "PASS: 12 reconciliaciones de tres ventanas conservaron el botón de arrastre."
            );
        }
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
    cases += check_modifiers(manager, ids, space, area)?;
    cases += check_placement_menu(manager, ids, space, area)?;
    cases += check_menu_rejections(manager, ids, space, area)?;
    cases += check_native_rejections(manager, ids, space, area)?;

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
                let grab = if layout == Layout::Spiral {
                    let late =
                        check_event_recipient(id, manager.windows[&id].pid, down, moved, true)?;
                    cases += 2;
                    late
                } else {
                    grab
                };
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
        unsafe {
            let _: () = msg_send![&*defaults, setBool:true, forKey:ns_string!("tile.shake")];
        }
        for animated in [false, true] {
            unsafe {
                let _: () =
                    msg_send![defaults, setBool:animated, forKey:ns_string!("tile.animate")];
            }
            let mut baseline = Desktop::configured(Layout::Grid, None);
            baseline.reconcile_at(ids, area, 10., None);
            prepare_model(manager, space, area, baseline, animated)?;
            let id = ids[0];
            let from = geometry(manager, id)?;
            let target = crate::tiling::floating_frame(from, area).ok_or("Sin marco flotante")?;
            let card = float_drop::frame(area).ok_or("Sin zona para devolver al mosaico")?;
            let release = NSPoint::new(
                card.origin.x + card.size.width * 0.5,
                card.origin.y + card.size.height * 0.5,
            );
            let tiled_down =
                NSPoint::new(from.origin.x + from.size.width * 0.4, from.origin.y + 32.);
            let mut moved_tile = from;
            moved_tile.origin.y += 20.;
            let grab = crate::tile_mouse::Grab {
                id,
                pid: manager.windows[&id].pid,
                frame: from,
            };
            // Exercise the actual zone commit, not the shortcut's toggle.
            manager.last_area_update = Some(Instant::now());
            manager.mouse_event(defaults, true, tiled_down, Some(grab));
            if !ax::set_frame(manager.windows[&id].element.0, moved_tile) {
                return Err("AX rechazó el movimiento previo a soltar en Hacer flotante".into());
            }
            finish_placements(manager, &[(id, moved_tile)])?;
            manager.mouse_event(defaults, false, release, None);
            if !manager.floating.contains(&id) {
                return Err("Soltar en Hacer flotante no sacó la ventana del mosaico".into());
            }
            // Only service animation/verification, not an idle model scan.
            finish_placements(manager, &[(id, target)])?;
            let start = ws::bounds(id).ok_or("Sin marco nativo flotante")?;
            let down = NSPoint::new(
                start.origin.x + start.size.width * 0.4,
                start.origin.y + 32.,
            );
            let mut moved = start;
            moved.origin.x += if start.origin.x > area.origin.x + 60. {
                -60.
            } else {
                60.
            };
            if !ax::set_frame(manager.windows[&id].element.0, moved) {
                return Err("AX rechazó el arrastre de la ventana recién flotante".into());
            }
            finish_placements(manager, &[(id, moved)])?;
            let grab = check_event_recipient(id, manager.windows[&id].pid, down, moved, false)?;
            manager.last_area_update = Some(Instant::now());
            manager.mouse_event(defaults, true, down, animated.then_some(grab));
            manager.tick_pointer_sample(defaults, true, None, || 1, || Some(release));
            let visible = manager
                .float_overlay
                .as_ref()
                .is_some_and(|overlay| unsafe { msg_send![&*overlay.panel, isVisible] });
            if !visible || !manager.floating.contains(&id) {
                return Err("El recuadro de retorno no estaba visible antes de soltar".into());
            }
            if !manager
                .drag
                .as_ref()
                .is_some_and(|drag| drag.id == id && drag.start == start && drag.was_floating)
            {
                return Err(
                    "El primer arrastre flotante perdió su geometría anterior al down".into(),
                );
            }
            manager.mouse_event(defaults, false, release, None);
            if manager.floating.contains(&id) || !manager.desktops[&space].order.contains(&id) {
                return Err("El primer soltado no devolvió la ventana flotante al mosaico".into());
            }
            assert_finished(manager)?;
            println!(
                "PASS E2E: retorno de ventana recién flotante con down tardío; animación={animated}"
            );
            cases += 1;
        }
        Ok(cases)
    })();
    unsafe {
        let _: () = msg_send![&*defaults, removePersistentDomainForName:&*suite];
    }
    result
}

fn check_event_recipient(
    id: u32,
    pid: i32,
    point: NSPoint,
    moved: NSRect,
    carries_pid: bool,
) -> Result<crate::tile_mouse::Grab, String> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateMouseEvent(
            source: *const std::ffi::c_void,
            kind: u32,
            point: NSPoint,
            button: u32,
        ) -> *const std::ffi::c_void;
        fn CGEventSetIntegerValueField(event: *const std::ffi::c_void, field: u32, value: i64);
    }
    unsafe {
        let event = ax::Owned::new(CGEventCreateMouseEvent(ptr::null(), 1, point, 0))
            .ok_or("No se pudo crear el CGEvent desechable")?;
        CGEventSetIntegerValueField(event.0, 92, i64::from(id));
        CGEventSetIntegerValueField(event.0, 40, if carries_pid { i64::from(pid) } else { 0 });
        let captured = crate::tile_mouse::Grab::from_event(event.0, point)
            .ok_or("El evento tardío perdió el destinatario nativo")?;
        if captured.id != id || captured.pid != pid || !near(captured.frame, moved) {
            return Err(
                "El callback tardío confundió destinatario o bounds por usar el punto antiguo"
                    .into(),
            );
        }
        CGEventSetIntegerValueField(event.0, 92, i64::from(u32::MAX));
        let occupied = NSPoint::new(
            moved.origin.x + moved.size.width / 2.,
            moved.origin.y + moved.size.height / 2.,
        );
        if crate::tile_mouse::Grab::from_event(event.0, occupied).is_some() {
            return Err("Un destinatario desaparecido se convirtió en otra ventana".into());
        }
        Ok(captured)
    }
}

fn check_modifiers(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<usize, String> {
    let mut baseline = Desktop::configured(Layout::Spiral, None);
    baseline.reconcile_at(ids, area, 10., None);
    let baseline = baseline
        .proposed_drop(ids[1], DropIntent::Group(ids[0]), area, 10.)
        .ok_or("Sin pila para comprobar ⌥")?;
    let mut cases = 0;
    for id in [ids[0], ids[1]] {
        let (virtual_layout, targets) = tab_drag_layout(&baseline, id, area);
        let virtual_layout = virtual_layout.ok_or("Sin huecos de colocación para ⌥")?;
        let frame = targets
            .iter()
            .find(|(target, _)| *target == ids[2])
            .ok_or("Sin destino para ⌥")?
            .1;
        let destination = NSPoint::new(
            frame.origin.x + frame.size.width * 0.25,
            frame.origin.y + frame.size.height * 0.15,
        );
        let independent = virtual_layout
            .proposed_drop(id, DropIntent::Window(ids[2], Zone::Center), area, 10.)
            .ok_or("Sin propuesta independiente para ⌥")?;
        let grouped = baseline
            .proposed_drop(id, DropIntent::Group(ids[2]), area, 10.)
            .ok_or("Sin propuesta apilada para ⌥")?;
        for (hover, release) in [(true, true), (true, false), (false, true), (false, false)] {
            prepare_model(manager, space, area, baseline.clone(), false)?;
            let dragged = button(manager, id)?;
            check_tab_pan(
                manager,
                &dragged,
                destination,
                destination,
                TabPanDelivery::Modifiers { hover, release },
                true,
                |pending| {
                    let expected = if hover { &independent } else { &grouped };
                    if pending
                        .tab_drag
                        .as_ref()
                        .and_then(|drag| drag.proposed.as_ref())
                        != Some(expected)
                        || pending.desktops[&space] != baseline
                    {
                        return Err("⌥ no cambió la preview sin mutar el mosaico".into());
                    }
                    Ok(())
                },
            )?;
            let expected = if release { &independent } else { &grouped };
            if &manager.desktops[&space] != expected {
                return Err(
                    "El modificador final de ⌥ se perdió durante la entrega reentrante".into(),
                );
            }
            place_layout(manager, space, area)?;
            assert_finished(manager)?;
            cases += 1;
        }
    }
    println!(
        "PASS E2E: ⌥ pulsada/soltada entre hover y Ended, activa/oculta, entrega AX reentrante y agrupación restaurada."
    );
    Ok(cases)
}

fn placement_submenu(menu: &AnyObject) -> Result<Retained<AnyObject>, String> {
    unsafe {
        let title = NSString::from_str(crate::i18n::tr("Colocar en…"));
        let item: Option<Retained<AnyObject>> = msg_send![menu, itemWithTitle:&*title];
        let item = item.ok_or("No aparece Colocar en… en el menú nativo")?;
        let submenu: Option<Retained<AnyObject>> = msg_send![&*item, submenu];
        submenu.ok_or_else(|| "Colocar en… no tiene submenú nativo".into())
    }
}

fn check_placement_menu(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<usize, String> {
    let mut cases = 0;
    for layout in Layout::ALL {
        let mut baseline = Desktop::configured(layout, None);
        baseline.reconcile_at(ids, area, 10., None);
        let baseline = baseline
            .proposed_drop(ids[1], DropIntent::Group(ids[0]), area, 10.)
            .ok_or("Sin pila para el menú de colocación")?;
        for id in [ids[0], ids[1]] {
            let (virtual_layout, targets) = tab_drag_layout(&baseline, id, area);
            let virtual_layout = virtual_layout.ok_or("Sin distribución final del menú")?;
            for (position, (target, _)) in targets.iter().enumerate() {
                prepare_model(manager, space, area, baseline.clone(), false)?;
                let context = manager.tab_context(id)?;
                let placements = manager.tab_placement_targets(&context);
                if placements.len() != targets.len()
                    || placements.iter().any(|entry| entry.unavailable.is_some())
                {
                    return Err("El menú no ofrece todos los huecos finales válidos".into());
                }
                let (menu, actions) =
                    unsafe { crate::ui::tab_menu(ptr::null_mut(), Vec::new(), placements) };
                let submenu = placement_submenu(&menu)?;
                let item: Retained<AnyObject> =
                    unsafe { msg_send![&*submenu, itemAtIndex:position as isize] };
                let enabled: bool = unsafe { msg_send![&*item, isEnabled] };
                let tag: isize = unsafe { msg_send![&*item, tag] };
                if !enabled || manager.desktops[&space] != baseline {
                    return Err(
                        "Abrir el menú cambió el mosaico o deshabilitó un hueco válido".into(),
                    );
                }
                if layout == Layout::Spiral && id == ids[0] && position == 0 {
                    let anchor = button(manager, id)?;
                    capture_placement_menu(&submenu, &anchor)?;
                    if manager.desktops[&space] != baseline {
                        return Err("Cancelar el menú nativo modificó el mosaico".into());
                    }
                }
                let action = usize::try_from(tag)
                    .ok()
                    .and_then(|tag| actions.into_iter().nth(tag))
                    .ok_or("El destino del menú perdió su acción")?;
                if !matches!(action, TabAction::Place(index) if index == position) {
                    return Err("El índice de menú no corresponde a la posición final".into());
                }
                let expected = virtual_layout
                    .proposed_drop(id, DropIntent::Window(*target, Zone::Center), area, 10.)
                    .ok_or("La posición de menú no admite el miembro")?;
                manager.perform_tab_action(&context, action)?;
                if manager.desktops[&space] != expected {
                    return Err("Colocar en… no confirmó la posición elegida".into());
                }
                place_layout(manager, space, area)?;
                assert_finished(manager)?;
                cases += 1;
            }
        }
    }
    println!(
        "PASS E2E: Colocar en… ofrece y confirma todos los huecos finales, activa/oculta, en las ocho plantillas."
    );
    Ok(cases)
}

fn view_contains(view: Obj, text: &str) -> bool {
    if view.is_null() {
        return false;
    }
    unsafe {
        let readable: bool = msg_send![view, respondsToSelector:sel!(stringValue)];
        if readable {
            let value: *mut NSString = msg_send![view, stringValue];
            if value
                .as_ref()
                .is_some_and(|value| value.to_string().contains(text))
            {
                return true;
            }
        }
        let children: Obj = msg_send![view, subviews];
        let count: usize = msg_send![children, count];
        (0..count).any(|index| {
            let child: Obj = msg_send![children, objectAtIndex:index];
            view_contains(child, text)
        })
    }
}

fn capture_native_window(number: isize, path: &str) -> Result<(), String> {
    std::fs::create_dir_all(".cache").map_err(|error| error.to_string())?;
    let status = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-o", "-l", &number.to_string(), path])
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("No se pudo capturar el rechazo de destino".into())
    }
}

fn check_menu_rejections(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<usize, String> {
    let mut baseline = Desktop::configured(Layout::Spiral, None);
    baseline.reconcile_at(ids, area, 10., None);
    let baseline = baseline
        .proposed_drop(ids[1], DropIntent::Group(ids[0]), area, 10.)
        .ok_or("Sin pila para rechazos de colocación")?;
    prepare_model(manager, space, area, baseline.clone(), false)?;
    let context = manager.tab_context(ids[0])?;
    let changed = baseline
        .proposed_drop(
            ids[2],
            DropIntent::Window(baseline.order[0], Zone::Center),
            area,
            10.,
        )
        .ok_or("Sin cambio de distribución para invalidar el menú")?;
    manager.desktops.insert(space, changed.clone());
    if manager
        .perform_tab_action(&context, TabAction::Place(0))
        .is_ok()
        || manager.desktops[&space] != changed
        || !manager.transitions.is_empty()
        || !manager.verifications.is_empty()
    {
        return Err("Un menú antiguo colocó sobre una distribución distinta".into());
    }
    prepare_model(manager, space, area, baseline.clone(), false)?;
    let context = manager.tab_context(ids[0])?;
    let original_areas = manager.active_areas.clone();
    let entry = manager
        .active_areas
        .iter_mut()
        .find(|(key, _)| *key == space)
        .ok_or("Sin área del fixture")?;
    entry.1.size.width -= 20.;
    let rejected = manager
        .perform_tab_action(&context, TabAction::Place(0))
        .is_err()
        && manager.desktops[&space] == baseline
        && manager.transitions.is_empty()
        && manager.verifications.is_empty();
    manager.active_areas = original_areas;
    if !rejected {
        return Err("Un menú antiguo aplicó coordenadas sobre un área distinta".into());
    }
    prepare_model(manager, space, area, baseline.clone(), false)?;
    let mut blocked = baseline;
    manager.minimum_sizes.insert(
        ids[0],
        objc2_foundation::NSSize::new(area.size.width + 100., area.size.height + 100.),
    );
    blocked.set_minimum_sizes(&manager.minimum_sizes);
    manager.desktops.insert(space, blocked.clone());
    let context = manager.tab_context(ids[0])?;
    let placements = manager.tab_placement_targets(&context);
    let reason = placements
        .iter()
        .find_map(|entry| entry.unavailable)
        .ok_or("El menú no explicó la restricción de tamaño")?;
    if placements.is_empty() || placements.iter().any(|entry| entry.unavailable.is_none()) {
        return Err("El menú ofreció una posición que no respeta el mínimo".into());
    }
    let count = placements.len();
    let (menu, _) = unsafe { crate::ui::tab_menu(ptr::null_mut(), Vec::new(), placements) };
    let submenu = placement_submenu(&menu)?;
    for index in 0..count {
        unsafe {
            let item: Obj = msg_send![&*submenu, itemAtIndex:index as isize];
            let enabled: bool = msg_send![item, isEnabled];
            let title: *mut NSString = msg_send![item, title];
            if enabled
                || title
                    .as_ref()
                    .is_none_or(|title| !title.to_string().contains(reason))
            {
                return Err("La posición incompatible no muestra el motivo deshabilitado".into());
            }
        }
    }
    let (_, targets) = tab_drag_layout(&blocked, ids[0], area);
    let frame = targets
        .iter()
        .find(|(id, _)| *id == ids[2])
        .ok_or("Sin hitbox virtual rechazado")?
        .1;
    let destination = NSPoint::new(
        frame.origin.x + frame.size.width * 0.25,
        frame.origin.y + frame.size.height * 0.7,
    );
    let dragged = button(manager, ids[0])?;
    check_tab_pan(
        manager,
        &dragged,
        destination,
        destination,
        TabPanDelivery::Reentrant,
        true,
        |pending| {
            if pending
                .tab_drag
                .as_ref()
                .and_then(|drag| drag.proposed.as_ref())
                .is_some()
                || pending.desktops[&space] != blocked
            {
                return Err("El destino rechazado resaltó una propuesta o mutó el modelo".into());
            }
            let badge = pending
                .badges
                .iter()
                .find(|badge| unsafe {
                    let visible: bool = msg_send![&*badge.panel, isVisible];
                    let content: Obj = msg_send![&*badge.panel, contentView];
                    visible && view_contains(content, reason)
                })
                .ok_or("El destino rechazado no explicó el mínimo durante el hover")?;
            let number: isize = unsafe { msg_send![&*badge.panel, windowNumber] };
            capture_native_window(number, ".cache/e2e-tab-rejected-minimum.png")
        },
    )?;
    if manager.desktops[&space] != blocked || !manager.transitions.is_empty() {
        return Err("Soltar sobre el rechazo modificó la distribución".into());
    }
    assert_finished(manager)?;
    println!(
        "PASS E2E: menú obsoleto por modelo/área rechazado, posiciones incompatibles deshabilitadas con motivo y mínimo explicado durante hover sin commit."
    );
    Ok(4)
}

fn capture_placement_menu(menu: &Retained<AnyObject>, anchor: &AnyObject) -> Result<(), String> {
    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        static NSEventTrackingRunLoopMode: Obj;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowLevelForKey(key: i32) -> i32;
    }
    let menu_level = i64::from(unsafe { CGWindowLevelForKey(11) });
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result = captured.clone();
    let cancel = menu.clone();
    let callback = block2::RcBlock::new(move |_: Obj| {
        let capture = (|| {
            let window = ws::transition_windows()
                .ok_or("No se pudo consultar el menú propio")?
                .into_iter()
                .find(|window| {
                    window.pid == std::process::id() as i32
                        && window.onscreen
                        && window.layer == menu_level
                })
                .ok_or("El submenú nativo no apareció en la pila propia")?;
            capture_native_window(window.id as isize, ".cache/e2e-tab-placement-menu.png")
        })();
        *result.borrow_mut() = Some(capture);
        unsafe {
            let _: () = msg_send![&*cancel, cancelTracking];
        }
    });
    unsafe {
        let timer: Retained<AnyObject> = msg_send![class!(NSTimer), timerWithTimeInterval:0.3f64, repeats:false, block:&*callback];
        let run_loop: Obj = msg_send![class!(NSRunLoop), currentRunLoop];
        let _: () =
            msg_send![run_loop, addTimer:&*timer, forMode:objc2_foundation::NSRunLoopCommonModes];
        let _: () = msg_send![run_loop, addTimer:&*timer, forMode:NSEventTrackingRunLoopMode];
        let bounds: NSRect = msg_send![anchor, bounds];
        let _: bool = msg_send![&**menu, popUpMenuPositioningItem:ptr::null::<AnyObject>(), atLocation:NSPoint::new(0., bounds.size.height), inView:anchor];
        let _: () = msg_send![&*timer, invalidate];
    }
    captured
        .borrow_mut()
        .take()
        .ok_or("El menú terminó antes de ejecutar la captura y cancelación propias")?
}

fn check_native_rejections(
    manager: &mut Manager,
    ids: &[u32],
    space: Workspace,
    area: NSRect,
) -> Result<usize, String> {
    for full in [true, false] {
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile_at(ids, area, 10., None);
        let mut desktop = desktop
            .proposed_drop(ids[1], DropIntent::Group(ids[2]), area, 10.)
            .ok_or("Sin grupo para rechazo nativo")?;
        desktop.max_group_tabs = if full { 2 } else { 5 };
        let target = desktop
            .order
            .iter()
            .copied()
            .find(|id| *id != ids[0])
            .ok_or("Sin destino visible para rechazo nativo")?;
        prepare_model(manager, space, area, desktop.clone(), false)?;
        let minimums = if full {
            HashMap::new()
        } else {
            HashMap::from([(
                ids[0],
                objc2_foundation::NSSize::new(area.size.width + 100., area.size.height + 100.),
            )])
        };
        let intent = if full {
            DropIntent::Group(target)
        } else {
            DropIntent::Window(target, Zone::Center)
        };
        let input = DropContext {
            workspace: space,
            id: ids[0],
            area,
            desktop: &desktop,
            minimums: &minimums,
        };
        let mut previews = DropPreviewCache::default();
        let preview = previews.preview(input, intent);
        if preview.is_some() {
            return Err("El hover nativo rechazado produjo una colocación".into());
        }
        let hint = previews
            .hint(input, intent, true, preview)
            .ok_or("El rechazo nativo no mostró indicador")?;
        if hint.active.is_some() {
            return Err("El indicador nativo resaltó un destino incompatible".into());
        }
        manager.show_badge(hint, primary_height());
        super::diagnostics::pump(0.04);
        let badge = manager
            .badges
            .first()
            .ok_or("Sin panel del rechazo nativo")?;
        let reason = crate::i18n::tr(if full { GROUP_FULL } else { MINIMUM_CAPACITY });
        unsafe {
            let content: Obj = msg_send![&*badge.panel, contentView];
            let ignores: bool = msg_send![&*badge.panel, ignoresMouseEvents];
            if !ignores || !view_contains(content, reason) {
                return Err("El rechazo nativo no explica su límite o intercepta el ratón".into());
            }
            let number: isize = msg_send![&*badge.panel, windowNumber];
            capture_native_window(
                number,
                if full {
                    ".cache/e2e-native-rejected-full.png"
                } else {
                    ".cache/e2e-native-rejected-minimum.png"
                },
            )?;
        }
        if manager.desktops[&space] != desktop {
            return Err("El hover nativo rechazado modificó el modelo".into());
        }
        manager.trim_overlays(0, 0);
    }
    println!(
        "PASS E2E: hover nativo explica grupo lleno y mínimos, sin resaltar, interceptar input ni mutar el mosaico."
    );
    Ok(2)
}
