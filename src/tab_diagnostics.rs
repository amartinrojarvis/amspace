//! Opt-in, main-thread smoke test. Only disposable child-process windows are moved.
use super::*;
use objc2::{
    rc::autoreleasepool,
    runtime::{ClassBuilder, Sel},
};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};

thread_local! {
    static STATE: RefCell<Option<Manager>> = const { RefCell::new(None) };
    static SELECTED: RefCell<Option<u32>> = const { RefCell::new(None) };
    static CONTROL_EVENTS: RefCell<Vec<(u32, isize)>> = const { RefCell::new(Vec::new()) };
    static DETACHED: RefCell<Option<u32>> = const { RefCell::new(None) };
    static CLOSE_REQUESTED: RefCell<Option<u32>> = const { RefCell::new(None) };
}

extern "C-unwind" fn select(_: Obj, _: Sel, sender: Obj) {
    let id: isize = unsafe { msg_send![sender, tag] };
    STATE.with(|state| {
        if let Some(manager) = state.borrow_mut().as_mut()
            && let Ok(id) = u32::try_from(id)
            && manager.activate_tab(id)
        {
            SELECTED.with(|selected| *selected.borrow_mut() = Some(id));
        }
    });
}

extern "C-unwind" fn drag(_: Obj, _: Sel, recognizer: Obj) {
    unsafe {
        let phase: isize = msg_send![recognizer, state];
        let button: Option<Retained<AnyObject>> = msg_send![recognizer, view];
        let location: NSPoint = msg_send![recognizer, locationInView:ptr::null::<AnyObject>()];
        STATE.with(|state| {
            if let Some(manager) = state.borrow_mut().as_mut() {
                if let Some(button) = button {
                    let id: isize = msg_send![&button, tag];
                    if manager.tab_drag(Retained::as_ptr(&button) as Obj, phase, location)
                        == TabDragOutcome::Detached
                    {
                        DETACHED.with(|detached| *detached.borrow_mut() = u32::try_from(id).ok());
                    }
                } else {
                    manager.cancel_tab_drag();
                }
            }
        });
    }
}
extern "C-unwind" fn close(_: Obj, _: Sel, sender: Obj) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    let Some(id) = tag
        .checked_add(2)
        .and_then(isize::checked_neg)
        .and_then(|id| u32::try_from(id).ok())
    else {
        return;
    };
    STATE.with(|state| {
        if let Some(manager) = state.borrow_mut().as_mut()
            && manager.close_tab(id)
        {
            CLOSE_REQUESTED.with(|requested| *requested.borrow_mut() = Some(id));
        }
    });
}
extern "C-unwind" fn control_close(_: Obj, _: Sel, sender: Obj) {
    let tag: isize = unsafe { msg_send![sender, tag] };
    CONTROL_EVENTS.with(|events| events.borrow_mut().push(((-tag - 2) as u32, 6)));
}

extern "C-unwind" fn control_click(_: Obj, _: Sel, button: Obj) {
    let id: isize = unsafe { msg_send![button, tag] };
    CONTROL_EVENTS.with(|events| events.borrow_mut().push((id as u32, 0)));
}
extern "C-unwind" fn control_drag(_: Obj, _: Sel, recognizer: Obj) {
    unsafe {
        let phase: isize = msg_send![recognizer, state];
        let button: Obj = msg_send![recognizer, view];
        let id: isize = msg_send![button, tag];
        CONTROL_EVENTS.with(|events| events.borrow_mut().push((id as u32, phase)));
    }
}

extern "C-unwind" fn control_context(_: Obj, _: Sel, button: Obj, _: Obj) {
    let id: isize = unsafe { msg_send![button, tag] };
    CONTROL_EVENTS.with(|events| events.borrow_mut().push((id as u32, 7)));
}

/// Exercise the actual strip controls with app-local mouse events: no AX,
/// global input injection, foreign windows, or preference changes.
pub fn check_controls() -> Result<(), String> {
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app,setActivationPolicy:1isize];
        let _: () = msg_send![app, finishLaunching];
        let mut builder = ClassBuilder::new(c"AMSpaceTabControlDiagnostic", class!(NSObject))
            .ok_or("Clase de diagnóstico duplicada")?;
        builder.add_method(
            sel!(selectTileTab:),
            control_click as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(dragTileTab:),
            control_drag as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(closeTileTab:),
            control_close as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(showTileTabMenu:event:),
            control_context as extern "C-unwind" fn(Obj, Sel, Obj, Obj),
        );
        let target: Retained<AnyObject> = msg_send![builder.register(), new];
        let names: [(String, Option<Retained<AnyObject>>); 3] = [
            ("Primera ventana".into(), None),
            ("Segunda ventana".into(), None),
            ("Tercera ventana".into(), None),
        ];
        let total = names.iter().map(|(name, _)| tab_width(name)).sum::<f64>() + 16.;
        let frame = tab_bar_frame(rect(300., 330., 620., 400.), total, false);
        let panel = new_overlay(frame, primary_height());
        let _: () = msg_send![&panel,setHasShadow:true];
        let content = strip_content(
            &names,
            &[1, 2, 3],
            1,
            frame.size.width,
            Some(&target),
            crate::model::TabAccent::default(),
        );
        let _: () = msg_send![&panel,setIgnoresMouseEvents:false];
        let _: () = msg_send![&panel,setContentView:&*content];
        let _: () = msg_send![&panel, orderFrontRegardless];
        pump(0.1);
        let document: Obj = msg_send![&content, documentView];
        let button: Retained<AnyObject> = msg_send![document,viewWithTag:2isize];
        let bounds: NSRect = msg_send![&button, bounds];
        let center: NSPoint = msg_send![&button,convertPoint:NSPoint::new(bounds.size.width / 2., bounds.size.height / 2.),toView:ptr::null::<AnyObject>()];
        let number: isize = msg_send![&panel, windowNumber];
        let queue_flags = |events: &[(usize, f64, f64)], flags: usize| -> Result<(), String> {
            for (index, (kind, x, y)) in events.iter().enumerate() {
                let process: Obj = msg_send![class!(NSProcessInfo), processInfo];
                let uptime: f64 = msg_send![process, systemUptime];
                let event: Option<Retained<AnyObject>> = msg_send![class!(NSEvent),
                    mouseEventWithType:*kind,
                    location:NSPoint::new(center.x + x, center.y + y),
                    modifierFlags:flags, timestamp:uptime + index as f64 * 0.01,
                    windowNumber:number, context:ptr::null::<AnyObject>(),
                    eventNumber:index as isize, clickCount:1isize, pressure:if *kind == 2 { 0f32 } else { 1f32 }];
                let event = event.ok_or("No se pudo crear el evento local")?;
                let _: () = msg_send![app,postEvent:&*event,atStart:false];
            }
            Ok(())
        };
        let queue = |events: &[(usize, f64, f64)]| queue_flags(events, 0);
        queue(&[(1, 0., 0.), (2, 0., 0.)])?;
        pump(0.15);
        let click = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if click != [(2, 0)] {
            return Err(format!("El clic no seleccionó una sola vez: {click:?}"));
        }
        queue(&[(1, 0., 0.), (6, 1., 1.), (2, 1., 1.)])?;
        pump(0.15);
        let jitter = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if jitter != [(2, 0)] {
            return Err(format!("Un movimiento mínimo dejó de ser clic: {jitter:?}"));
        }
        queue(&[(1, 0., 0.), (6, 15., -8.), (6, 45., -28.), (2, 45., -28.)])?;
        pump(0.2);
        let dragged = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if dragged.iter().any(|(id, phase)| *id != 2 || *phase == 0)
            || dragged.iter().filter(|(_, phase)| *phase == 1).count() != 1
            || dragged.iter().filter(|(_, phase)| *phase == 3).count() != 1
        {
            return Err(format!(
                "Arrastre sin comienzo/fin único o con clic espurio: {dragged:?}"
            ));
        }
        let _: () = msg_send![&button,performClick:ptr::null::<AnyObject>()];
        let programmatic = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if programmatic != [(2, 0)] {
            return Err(format!(
                "La selección quedó bloqueada tras arrastrar: {programmatic:?}"
            ));
        }
        queue(&[(1, 0., 0.), (6, 15., -8.)])?;
        pump(0.1);
        let gestures: Obj = msg_send![&button, gestureRecognizers];
        let gesture: Obj = msg_send![gestures,objectAtIndex:0usize];
        let _: () = msg_send![gesture,setEnabled:false];
        queue(&[(2, 15., -8.)])?;
        pump(0.1);
        let cancelled = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if !cancelled.contains(&(2, 4)) || cancelled.iter().any(|(_, phase)| matches!(phase, 0 | 3))
        {
            return Err(format!("Cancelación nativa incorrecta: {cancelled:?}"));
        }
        let _: () = msg_send![gesture,setEnabled:true];
        queue(&[(1, 0., 0.), (2, 0., 0.)])?;
        pump(0.15);
        let next_click = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
        if next_click != [(2, 0)] {
            return Err(format!(
                "El clic posterior a cancelar quedó bloqueado: {next_click:?}"
            ));
        }
        for (events, flags) in [
            (&[(3usize, 0., 0.), (4, 0., 0.)][..], 0),
            (&[(1usize, 0., 0.), (2, 0., 0.)][..], 1 << 18),
        ] {
            queue_flags(events, flags)?;
            pump(0.15);
            let events = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
            if events != [(2, 7)] {
                return Err(format!(
                    "El menú contextual seleccionó o arrastró la pestaña: {events:?}"
                ));
            }
        }
        for id in 1..=3isize {
            let close: Retained<AnyObject> = msg_send![document,viewWithTag:-id - 2];
            // Exercise the native tracking callbacks without moving the user's cursor.
            let tab: Obj = msg_send![&close, superview];
            let areas: Obj = msg_send![tab, trackingAreas];
            let tracking: Obj = msg_send![areas, firstObject];
            if tracking.is_null() {
                return Err("La pestaña no tiene seguimiento nativo del cursor".into());
            }
            let owner: Obj = msg_send![tracking, owner];
            let original: NSRect = msg_send![&close, frame];
            let _: () = msg_send![owner,mouseExited:ptr::null::<AnyObject>()];
            let hidden: bool = msg_send![&close, isTransparent];
            if hidden != (id != 2) {
                return Err(format!("Visibilidad de cierre incorrecta sin hover: {id}"));
            }
            let _: () = msg_send![owner,mouseEntered:ptr::null::<AnyObject>()];
            let hidden: bool = msg_send![&close, isTransparent];
            let hovered: NSRect = msg_send![&close, frame];
            if hidden || hovered != original {
                return Err(format!("El hover oculta o desplaza el cierre: {id}"));
            }
            let bounds: NSRect = msg_send![&close, bounds];
            let point: NSPoint = msg_send![&close,convertPoint:NSPoint::new(bounds.size.width / 2., bounds.size.height / 2.),toView:ptr::null::<AnyObject>()];
            let x = point.x - center.x;
            let y = point.y - center.y;
            queue(&[(1, x, y), (2, x, y)])?;
            pump(0.15);
            let events = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
            if events != [(id as u32, 6)] {
                return Err(format!(
                    "Cerrar la pestaña {id} seleccionó/arrastró otro control: {events:?}"
                ));
            }
            queue(&[(1, x, y), (6, x, y - 40.), (2, x, y - 40.)])?;
            pump(0.15);
            let events = CONTROL_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
            if !events.is_empty() {
                return Err(format!(
                    "Soltar fuera de cerrar ejecutó una acción: {events:?}"
                ));
            }
            let _: () = msg_send![owner,mouseExited:ptr::null::<AnyObject>()];
        }
        // A native move repositions the cached strip without replacing controls
        // or fetching captions from AX. No foreign windows are needed here.
        let mut desktop = Desktop::default();
        let slot = rect(300., 330., 620., 400.);
        desktop.reconcile_at(&[1], slot, 10., None);
        for id in [2, 3] {
            desktop = desktop
                .proposed_drop(id, DropIntent::Group(id - 1), slot, 10.)
                .ok_or("No se pudo preparar el grupo de diagnóstico")?;
        }
        desktop.select_tab(2);
        let mut manager = Manager {
            pressed: true,
            strips: vec![TabStrip {
                panel: panel.clone(),
                space: (0, 0),
                area: None,
                binding: StripSpaceBinding::Fallback,
                marker: None,
                signature: names.to_vec(),
                members: vec![1, 2, 3],
                active: 1,
                built: frame,
                frame,
            }],
            ..Manager::default()
        };
        for window in [rect(324., 342., 620., 400.), slot] {
            manager.draw_strips((0, 0), &desktop, &[(2, window)], primary_height(), 0, false);
            let root: Obj = msg_send![&panel, contentView];
            let actual: NSRect = msg_send![&panel, frame];
            let expected = appkit(tab_bar_frame(window, total, false), primary_height());
            if !ptr::eq(root, Retained::as_ptr(&content)) || actual != expected {
                return Err("Mover la tira reconstruyó los controles o perdió su posición.".into());
            }
        }
        check_reorder_presentation(&mut manager.strips[0])?;
        check_strip_lifecycle(&target)?;
        capture_control(
            Retained::as_ptr(&content) as Obj,
            ".cache/tab-accent-before.png",
        )?;
        manager.set_tab_accent([0.25, 0.72, 0.62]);
        pump(0.05);
        let recolored: Obj = msg_send![&panel, contentView];
        if !ptr::eq(recolored, Retained::as_ptr(&content)) {
            return Err("Cambiar el color reconstruyó los controles de la tira.".into());
        }
        capture_control(recolored, ".cache/tab-accent-after.png")?;
        if std::fs::read(".cache/tab-accent-before.png").map_err(|e| e.to_string())?
            == std::fs::read(".cache/tab-accent-after.png").map_err(|e| e.to_string())?
        {
            return Err("Cambiar el color no actualizó la pestaña activa.".into());
        }
        manager.set_tab_accent([0.42, 0.48, 0.95]);
        let mut hint = DropHint {
            frame: slot,
            kind: HintKind::Zones,
            available: [true, true],
            active: Some(0),
        };
        manager.show_badge(hint, primary_height());
        let badge: Retained<AnyObject> = msg_send![&*manager.badges[0].panel, contentView];
        let ignores_mouse: bool = msg_send![&*manager.badges[0].panel, ignoresMouseEvents];
        if !ignores_mouse {
            return Err("Las zonas de soltado interceptan el arrastre.".into());
        }
        pump(0.05);
        capture_control(
            Retained::as_ptr(&badge) as Obj,
            ".cache/drop-zones-group.png",
        )?;
        hint.frame = rect(324., 342., 620., 400.);
        manager.show_badge(hint, primary_height());
        let moved_badge: Obj = msg_send![&*manager.badges[0].panel, contentView];
        if !ptr::eq(moved_badge, Retained::as_ptr(&badge)) {
            return Err("Mover el indicador de agrupación reconstruyó su contenido.".into());
        }
        hint.active = Some(1);
        manager.show_badge(hint, primary_height());
        let selected_badge: Obj = msg_send![&*manager.badges[0].panel, contentView];
        if !ptr::eq(selected_badge, Retained::as_ptr(&badge)) {
            return Err("Cruzar las zonas reconstruyó los controles del indicador.".into());
        }
        pump(0.05);
        capture_control(selected_badge, ".cache/drop-zones-swap.png")?;
        if std::fs::read(".cache/drop-zones-group.png").map_err(|e| e.to_string())?
            == std::fs::read(".cache/drop-zones-swap.png").map_err(|e| e.to_string())?
        {
            return Err("Cruzar las zonas no cambió el resaltado visible.".into());
        }
        hint.available[0] = false;
        hint.active = None;
        manager.show_badge(hint, primary_height());
        pump(0.05);
        let unavailable: Obj = msg_send![&*manager.badges[0].panel, contentView];
        capture_control(unavailable, ".cache/drop-zones-unavailable.png")?;
        hint.frame = rect(324., 342., 180., 180.);
        manager.show_badge(hint, primary_height());
        pump(0.05);
        let compact: Obj = msg_send![&*manager.badges[0].panel, contentView];
        capture_control(compact, ".cache/drop-zones-compact.png")?;
        manager.show_badge(
            DropHint::single(DropIntent::Screen(Zone::Left), slot),
            primary_height(),
        );
        manager.trim_overlays(1, 0);
        capture_control(Retained::as_ptr(&content) as Obj, ".cache/tab-controls.png")?;
        render_tab_presentation(&names, &target)?;
        render_tab_palette(&target)?;
        objc2::rc::autoreleasepool(|_| check_saved_picker())?;
        if let Some(seconds) = std::env::var("AMSPACE_TAB_IDLE_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| (1..=60).contains(seconds))
        {
            println!("IDLE: tira visible; muestra de recursos durante {seconds} s");
            let _ = std::io::stdout().flush();
            let deadline = Instant::now() + Duration::from_secs(seconds);
            while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                autoreleasepool(|_| {
                    let date: Obj = msg_send![class!(NSDate),dateWithTimeIntervalSinceNow:remaining.as_secs_f64()];
                    let event: Option<Retained<AnyObject>> = msg_send![app,nextEventMatchingMask:usize::MAX,untilDate:date,inMode:objc2_foundation::NSDefaultRunLoopMode,dequeue:true];
                    if let Some(event) = event {
                        let _: () = msg_send![app,sendEvent:&*event];
                    }
                    let _: () = msg_send![app, updateWindows];
                });
            }
        }
        let _: () = msg_send![&panel,orderOut:ptr::null::<AnyObject>()];
        println!(
            "PASS: selección, arrastre/cancelación, menú con clic derecho y Control-clic, cierre individual, hover y reutilización de controles al mover la tira; captura .cache/tab-controls.png. Sin AX ni ventanas ajenas."
        );
        check_float_drop_card()?;
        crate::ui::diagnostics::check_deferred_callbacks()?;
        Ok(())
    }
}

fn check_float_drop_card() -> Result<(), String> {
    let mut manager = Manager::default();
    let frame = float_drop::frame(rect(180., 180., 1100., 760.)).unwrap();
    manager.show_float_overlay(frame, false, false);
    let panel = manager.float_overlay.as_ref().unwrap().panel.clone();
    let result = (|| unsafe {
        pump(0.04);
        let visible: bool = msg_send![&*panel, isVisible];
        let passive: bool = msg_send![&*panel, ignoresMouseEvents];
        let key: bool = msg_send![&*panel, isKeyWindow];
        if !visible || !passive || key {
            return Err("La zona superior no es visible y pasiva.".into());
        }
        let root: Obj = msg_send![&*panel, contentView];
        for (floating, highlighted, name) in [
            (false, false, "float-drop-idle"),
            (false, true, "float-drop-active"),
            (true, false, "tile-drop-idle"),
            (true, true, "tile-drop-active"),
        ] {
            manager.show_float_overlay(frame, floating, highlighted);
            pump(0.04);
            let current_root: Obj = msg_send![&*panel, contentView];
            if current_root != root
                || !ptr::eq(
                    Retained::as_ptr(&panel),
                    Retained::as_ptr(&manager.float_overlay.as_ref().unwrap().panel),
                )
            {
                return Err("Cambiar de acción o resaltado reconstruyó la zona superior.".into());
            }
            capture_control(current_root, &format!(".cache/{name}.png"))?;
        }
        let moved = float_drop::frame(rect(-1000., 0., 1000., 1400.)).unwrap();
        manager.show_float_overlay(moved, true, false);
        let current: NSRect = msg_send![&*panel, frame];
        if current != appkit(moved, primary_height()) {
            return Err("La zona superior no sigue la pantalla reutilizando su panel.".into());
        }
        manager.cancel_drag();
        let visible: bool = msg_send![&*panel, isVisible];
        if visible {
            return Err("Cancelar el arrastre dejó visible la zona superior.".into());
        }
        println!(
            "Float drop: both actions, passive panel, shared drop-hint style, reused across hover/display changes and hidden on cancellation"
        );
        Ok(())
    })();
    manager.hide_float_overlay();
    result
}

fn check_reorder_presentation(strip: &mut TabStrip) -> Result<(), String> {
    unsafe {
        let viewport: Obj = msg_send![&*strip.panel, contentView];
        let document: Obj = msg_send![viewport, documentView];
        let location = |point: NSPoint| -> NSPoint {
            msg_send![document, convertPoint:point, toView:ptr::null::<AnyObject>()]
        };
        let last: Obj = msg_send![document, viewWithTag:3isize];
        let tab: Obj = msg_send![last, superview];
        let last_frame: NSRect = msg_send![tab, frame];
        for (point, expected) in [
            (NSPoint::new(5., 11.), Some(0)),
            (
                NSPoint::new(last_frame.origin.x + last_frame.size.width - 2., 11.),
                Some(2),
            ),
            (NSPoint::new(5., -40.), None),
        ] {
            let result = strip.reorder_target(2, location(point), false);
            if result != expected {
                return Err(format!(
                    "Destino de reordenación incorrecto: {point:?} → {result:?}"
                ));
            }
        }
        strip.reorder_target(2, location(NSPoint::new(5., 11.)), false);
        capture_control(viewport, ".cache/tab-reorder.png")?;
        if strip.marker.is_none() {
            return Err("Falta el indicador de inserción".into());
        }
        strip.clear_marker();
        // Exercise real NSClipView coordinates, not a guessed scroll offset.
        let original: NSRect = msg_send![viewport, frame];
        let _: () =
            msg_send![viewport, setFrameSize:objc2_foundation::NSSize::new(180., TAB_BAR_HEIGHT)];
        let clip: Obj = msg_send![viewport, contentView];
        let _: () = msg_send![clip, scrollToPoint:NSPoint::new(last_frame.origin.x, 0.)];
        let _: () = msg_send![viewport, reflectScrolledClipView:clip];
        let visible: NSRect = msg_send![document, visibleRect];
        let inside = location(NSPoint::new(
            last_frame.origin.x + last_frame.size.width * 0.75,
            11.,
        ));
        let target = strip.reorder_target(2, inside, false);
        strip.clear_marker();
        let _: () = msg_send![viewport, setFrame:original];
        let _: () = msg_send![clip, scrollToPoint:NSPoint::ZERO];
        let _: () = msg_send![viewport, reflectScrolledClipView:clip];
        if visible.origin.x <= 0. || target != Some(2) {
            return Err(format!(
                "Reordenar con scroll perdió el destino: {visible:?}, {target:?}"
            ));
        }
        let root: Obj = msg_send![&*strip.panel, contentView];
        if root != viewport {
            return Err("El gesto reconstruyó la tira".into());
        }
        println!(
            "PASS: inserción al principio/final, salida para separar, marcador y coordenadas con scroll; sin reconstruir controles."
        );
    }
    Ok(())
}

fn check_strip_lifecycle(target: &Retained<AnyObject>) -> Result<(), String> {
    let (space, area) = spaces::visible_desktops()
        .into_iter()
        .find_map(|key| area(key.0).map(|area| (key, area)))
        .ok_or("Sin escritorio visible para comprobar la pertenencia de la tira")?;
    let mut desktop = Desktop::default();
    desktop.reconcile_at(&[1], area, 10., None);
    desktop = desktop
        .proposed_drop(2, DropIntent::Group(1), area, 10.)
        .ok_or("Sin grupo de diagnóstico")?;
    let frames = desktop.window_frames(area, 10.);
    let mut manager = Manager {
        tab_target: Some(target.clone()),
        active_areas: vec![(space, area)],
        ..Manager::default()
    };
    manager.desktops.insert(space, desktop.clone());
    manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
    // Like the production tick, wait for WindowServer's first membership
    // publication without sleeping/blocking the production main thread.
    pump(0.15);
    manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
    let strip = &manager.strips[0];
    if !matches!(strip.binding, StripSpaceBinding::Confirmed) {
        return Err("AppKit no asignó la tira a su Space nativo".into());
    }
    let panel = strip.panel.clone();
    let root: Retained<AnyObject> = unsafe { msg_send![&*panel, contentView] };
    let number: isize = unsafe { msg_send![&*panel, windowNumber] };
    for _ in 0..20 {
        manager.space_changed_with_visible_at(Instant::now(), &[]);
        manager.active_areas.clear();
        manager.trim_overlays(0, 0);
        let visible: bool = unsafe { msg_send![&*panel, isVisible] };
        if manager.strips.len() != 1
            || !visible
            || spaces::window_space(number as u32) != Some(space.1)
        {
            return Err("La transición retiró o desancló la tira retenida".into());
        }
        manager.active_areas.push((space, area));
        manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
        let current: Obj = unsafe { msg_send![&*manager.strips[0].panel, contentView] };
        if !ptr::eq(current, Retained::as_ptr(&root))
            || !ptr::eq(
                Retained::as_ptr(&manager.strips[0].panel),
                Retained::as_ptr(&panel),
            )
        {
            return Err("Volver al contexto reconstruyó panel o controles".into());
        }
    }
    unsafe {
        capture_control(
            Retained::as_ptr(&root) as Obj,
            ".cache/tab-space-resident.png",
        )?;
    }
    manager.cancel_drag();
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("Pausar dejó una tira visible".into());
    }
    let now = Instant::now();
    manager.space_changed_with_visible_at(now, &[space]);
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("El aviso de Space anuló una pausa sin autorización de la UI".into());
    }
    manager.restore_space_strips(&[(space.0, space.1.wrapping_add(1))]);
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("Se restauró una tira de otro escritorio".into());
    }
    manager.strips[0].area = Some(rect(
        area.origin.x,
        area.origin.y,
        area.size.width + 100.,
        area.size.height,
    ));
    manager.restore_space_strips(&[space]);
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("Se reutilizaron coordenadas anteriores a un cambio de área".into());
    }
    manager.strips[0].area = Some(area);
    manager.strips[0].binding = StripSpaceBinding::Fallback;
    manager.restore_space_strips(&[space]);
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("Se restauró un panel sin pertenencia nativa confirmada".into());
    }
    manager.strips[0].binding = StripSpaceBinding::Confirmed;
    manager.set_presentation_at(crate::desktop_mode::Mode::Overview, now, None);
    manager.restore_space_strips(&[space]);
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("Se restauró una tira durante la presentación del escritorio".into());
    }
    manager.set_presentation_at(crate::desktop_mode::Mode::Normal, now, None);
    // Same authorized path as configure_tiling, before any AX scan or redraw.
    let started = Instant::now();
    manager.resume_space_strips();
    let elapsed = started.elapsed();
    let current: Obj = unsafe { msg_send![&*panel, contentView] };
    if !unsafe { msg_send![&*panel, isVisible] }
        || !manager.waiting_for_space(now)
        || manager.space_reentry.is_none()
        || !ptr::eq(current, Retained::as_ptr(&root))
    {
        return Err("Volver de presentación no restauró la tira antes de reconciliar".into());
    }
    println!(
        "PASS: restauración de tira en {:.3} ms, barrera geométrica aún activa; aislamiento por Space/área/pausa/pertenencia.",
        elapsed.as_secs_f64() * 1000.
    );
    unsafe {
        capture_control(
            Retained::as_ptr(&root) as Obj,
            ".cache/tab-space-restored.png",
        )?;
    }
    manager.cancel_drag();
    manager.active_areas.clear();
    manager.desktops.clear();
    manager.restore_space_strips(&[space]);
    if unsafe { msg_send![&*panel, isVisible] } {
        return Err("Se restauró una tira cuyo grupo ya no existe".into());
    }
    manager.trim_overlays(0, 0);
    if !manager.strips.is_empty() {
        return Err("Una tira sin grupo quedó retenida".into());
    }
    println!(
        "PASS: panel nativo del Space {}, 20 ciclos de barrera/caché sin ocultar ni reconstruir, pausa y retirada. No se cambió físicamente de escritorio.",
        space.1
    );
    Ok(())
}

fn check_saved_picker() -> Result<(), String> {
    unsafe {
        let suite =
            NSString::from_str(&format!("amspace.diagnostic.picker.{}", std::process::id()));
        let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*suite];
        let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        let raw = Retained::as_ptr(&defaults) as Obj;
        let result = (|| {
            let area = rect(100., 100., 1400., 800.);
            let mut saved = Desktop::configured(Layout::Spiral, Some((4, 4)));
            saved.reconcile_at(&[1, 2], area, 10., None);
            crate::custom_templates::save(
                raw,
                "Distribución con pestañas",
                saved.save_distribution(area).ok_or("Plantilla inválida")?,
            )?;
            let mut desktop = Desktop::configured(Layout::Grid, Some((7, 3)));
            desktop.reconcile_at(&[101, 102], area, 10., None);
            desktop = desktop
                .proposed_drop(103, DropIntent::Group(101), area, 10.)
                .ok_or("Grupo inválido")?;
            let context = PickerContext {
                workspace: (1, 7),
                area,
                desktop,
            };
            let mut picker = crate::tile_picker::Picker::new(context, ptr::null_mut(), raw)
                .ok_or("No se pudo abrir el selector")?;
            picker.select(Layout::ALL.len());
            if !matches!(
                picker.selection(),
                Some(crate::tile_picker::Selection::Custom(_))
            ) {
                return Err("La plantilla agrupada no se pudo seleccionar.".into());
            }
            picker.show();
            pump(0.1);
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let panel: Obj = msg_send![app, keyWindow];
            if panel.is_null() {
                return Err("El selector no recibió su ventana nativa.".into());
            }
            let view: Obj = msg_send![panel, contentView];
            capture_control(view, ".cache/picker-grouped-preview.png")
        })();
        let _: () = msg_send![&defaults,removePersistentDomainForName:&*suite];
        result
    }
}

unsafe fn capture_control(view: Obj, path: &str) -> Result<(), String> {
    unsafe {
        let bounds: NSRect = msg_send![view, bounds];
        let bitmap: Option<Retained<AnyObject>> =
            msg_send![view,bitmapImageRepForCachingDisplayInRect:bounds];
        let bitmap = bitmap.ok_or("Sin bitmap del control")?;
        let _: () = msg_send![view,cacheDisplayInRect:bounds,toBitmapImageRep:&*bitmap];
        let properties: Obj = msg_send![class!(NSDictionary), dictionary];
        let data: Option<Retained<AnyObject>> =
            msg_send![&bitmap,representationUsingType:4usize,properties:properties];
        let data = data.ok_or("Sin PNG del control")?;
        std::fs::create_dir_all(".cache").map_err(|error| error.to_string())?;
        let saved: bool = msg_send![&data,writeToFile:&*NSString::from_str(path),atomically:true];
        if saved {
            Ok(())
        } else {
            Err("No se pudo guardar la captura".into())
        }
    }
}

/// Show the real strip against disposable window surfaces, without foreign AX.
unsafe fn render_tab_presentation(
    names: &[(String, Option<Retained<AnyObject>>)],
    target: &AnyObject,
) -> Result<(), String> {
    unsafe {
        let panel = new_overlay(rect(200., 180., 720., 330.), primary_height());
        let canvas = alloc_view(class!(NSView), rect(0., 0., 720., 330.));
        let appearance: Obj = msg_send![class!(NSAppearance),appearanceNamed:&*NSString::from_str("NSAppearanceNameDarkAqua")];
        let _: () = msg_send![&canvas,setAppearance:appearance];
        let _: () = msg_send![&canvas,setWantsLayer:true];
        let layer: Obj = msg_send![&canvas, layer];
        let background: *const crate::native::CgColor =
            msg_send![color(0.15, 0.16, 0.18, 1.), CGColor];
        let _: () = msg_send![layer,setBackgroundColor:background];
        let total = names.iter().map(|(name, _)| tab_width(name)).sum::<f64>()
            + 4. * names.len().saturating_sub(1) as f64
            + 8.;
        for (index, (centered, width, title)) in [
            (false, 680., "Izquierda · predeterminado"),
            (true, 680., "Centro"),
            (false, 260., "Ventana estrecha · desplazamiento horizontal"),
        ]
        .into_iter()
        .enumerate()
        {
            let window = rect(20., 45. + index as f64 * 105., width, 58.);
            let surface = alloc_view(class!(NSView), appkit(window, 330.));
            let _: () = msg_send![&surface,setWantsLayer:true];
            let layer: Obj = msg_send![&surface, layer];
            let background: *const crate::native::CgColor =
                msg_send![color(0.23, 0.24, 0.27, 1.), CGColor];
            let _: () = msg_send![layer,setBackgroundColor:background];
            let _: () = msg_send![layer,setCornerRadius:8f64];
            crate::native::label(
                Retained::as_ptr(&surface) as Obj,
                rect(12., 18., width - 24., 22.),
                title,
                12.,
                0.,
                false,
            );
            let _: () = msg_send![&canvas,addSubview:&*surface];
            let frame = tab_bar_frame(window, total, centered);
            let strip = strip_content(
                names,
                &[1, 2, 3],
                1,
                frame.size.width,
                Some(target),
                crate::model::TabAccent::default(),
            );
            let _: () = msg_send![&strip,setFrame:appkit(frame,330.)];
            let _: () = msg_send![&canvas,addSubview:&*strip];
        }
        let _: () = msg_send![&panel,setContentView:&*canvas];
        let _: () = msg_send![&panel, orderFrontRegardless];
        pump(0.1);
        let result = capture_control(
            Retained::as_ptr(&canvas) as Obj,
            ".cache/tab-presentation.png",
        );
        let _: () = msg_send![&panel,orderOut:ptr::null::<AnyObject>()];
        result
    }
}

/// Original app icon plus monochrome extremes on light/dark window surfaces.
unsafe fn render_tab_palette(target: &AnyObject) -> Result<(), String> {
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let application_icon: Obj = msg_send![app, applicationIconImage];
        let application_icon =
            Retained::retain(application_icon).ok_or("Sin icono de aplicación")?;
        let monochrome = |value: f64| -> Retained<AnyObject> {
            let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
            let image: Retained<AnyObject> =
                msg_send![image, initWithSize:objc2_foundation::NSSize::new(12., 12.)];
            let _: () = msg_send![&image, lockFocus];
            let fill = color(value, value, value, 1.);
            let _: () = msg_send![fill, setFill];
            let path: Obj = msg_send![class!(NSBezierPath), bezierPathWithRoundedRect:rect(1., 1., 10., 10.), xRadius:2f64, yRadius:2f64];
            let _: () = msg_send![path, fill];
            let _: () = msg_send![&image, unlockFocus];
            image
        };
        let names = [
            ("Icono original · aplicación".into(), Some(application_icon)),
            ("Icono oscuro · título".into(), Some(monochrome(0.))),
            ("Icono claro · título".into(), Some(monochrome(1.))),
        ];
        let panel = new_overlay(rect(200., 180., 920., 450.), primary_height());
        let canvas = alloc_view(class!(NSView), rect(0., 0., 920., 450.));
        let appearance: Obj = msg_send![class!(NSAppearance), appearanceNamed:&*NSString::from_str("NSAppearanceNameDarkAqua")];
        let _: () = msg_send![&canvas, setAppearance:appearance];
        let parent = Retained::as_ptr(&canvas) as Obj;
        for (index, (title, rgb)) in [
            ("Índigo", [0.42, 0.48, 0.95]),
            ("Menta", [0.25, 0.72, 0.62]),
            ("Ámbar", [0.94, 0.65, 0.27]),
            ("Rosa", [0.88, 0.42, 0.60]),
            ("Negro", [0., 0., 0.]),
            ("Blanco", [1., 1., 1.]),
        ]
        .into_iter()
        .enumerate()
        {
            let y = 450. - (index + 1) as f64 * 75.;
            let light = index % 2 != 0;
            let background = if light {
                [0.90, 0.91, 0.93]
            } else {
                [0.15, 0.16, 0.18]
            };
            rounded_plate(parent, rect(0., y, 920., 75.), background, 1., 0.);
            let label =
                crate::native::label(parent, rect(12., y + 26., 86., 22.), title, 12., 0., false);
            let value = if light { 0.12 } else { 0.95 };
            let _: () = msg_send![label, setTextColor:color(value, value, value, 1.)];
            let strip = strip_content(
                &names,
                &[1, 2, 3],
                index % 3,
                790.,
                Some(target),
                crate::model::TabAccent::new(rgb),
            );
            let _: () = msg_send![&strip, setFrame:rect(110., y + 26., 790., TAB_BAR_HEIGHT)];
            let _: () = msg_send![&canvas, addSubview:&*strip];
        }
        let _: () = msg_send![&panel, setContentView:&*canvas];
        let _: () = msg_send![&panel, orderFrontRegardless];
        pump(0.1);
        let result = capture_control(parent, ".cache/tab-palette.png");
        let _: () = msg_send![&panel, orderOut:ptr::null::<AnyObject>()];
        result
    }
}

struct Fixture(Child);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Panels;
impl Drop for Panels {
    fn drop(&mut self) {
        STATE.with(|state| {
            if let Some(manager) = state.borrow_mut().take() {
                manager.hide_preview();
            }
        });
    }
}
fn pump(seconds: f64) {
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < end {
        autoreleasepool(|_| unsafe {
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let date: Obj = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow:0.01f64];
            let event: Option<Retained<AnyObject>> = msg_send![app, nextEventMatchingMask:usize::MAX, untilDate:date, inMode:objc2_foundation::NSDefaultRunLoopMode, dequeue:true];
            if let Some(event) = event {
                let _: () = msg_send![app, sendEvent:&*event];
            }
            let _: () = msg_send![app, updateWindows];
        });
    }
}

fn post_mouse(kind: u32, point: NSPoint) -> Result<(), String> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateMouseEvent(
            source: ax::Cf,
            kind: u32,
            point: NSPoint,
            button: u32,
        ) -> ax::Cf;
        fn CGEventPost(tap: u32, event: ax::Cf);
    }
    let event = ax::Owned::new(unsafe { CGEventCreateMouseEvent(ptr::null(), kind, point, 0) })
        .ok_or("No se pudo crear el clic de diagnóstico")?;
    unsafe { CGEventPost(0, event.0) };
    Ok(())
}
struct Cursor(Option<NSPoint>);
impl Drop for Cursor {
    fn drop(&mut self) {
        if let Some(point) = self.0 {
            let _ = post_mouse(5, point);
        }
    }
}

pub fn run_fixture() {
    // A watchdog bounds orphaned fixtures if the inspecting process crashes.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(90));
        unsafe { libc::_exit(0) }
    });
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app, setActivationPolicy:1isize];
        let _: () = msg_send![app, finishLaunching];
        let mut windows = Vec::new();
        for index in 0..3 {
            let window: Allocated<AnyObject> = msg_send![class!(NSWindow), alloc];
            let window: Retained<AnyObject> = msg_send![window, initWithContentRect:rect(160. + index as f64 * 60., 180., 360., 240.), styleMask:15usize, backing:2usize, defer:false];
            let _: () = msg_send![&window, setReleasedWhenClosed:false];
            let _: () = msg_send![&window, setTabbingMode:2isize];
            let _: () = msg_send![&window, setTitle:&*NSString::from_str(&format!("AMSpace · pestaña temporal {index} · título largo"))];
            let _: () = msg_send![&window, orderFrontRegardless];
            windows.push(window);
        }
        let ids: Vec<isize> = windows
            .iter()
            .map(|window| msg_send![&**window, windowNumber])
            .collect();
        println!("{} {} {}", ids[0], ids[1], ids[2]);
        let _ = std::io::stdout().flush();
        let _: () = msg_send![app, run];
    }
}

pub fn check() -> Result<(), String> {
    let _lock = crate::instance::acquire()
        .map_err(|e| e.to_string())?
        .ok_or("Cierra la instancia normal de AMSpace antes del diagnóstico.")?;
    if !ax::trusted() {
        return Err(
            "El diagnóstico necesita Accesibilidad; no cambia permisos ni preferencias.".into(),
        );
    }
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app, setActivationPolicy:1isize];
        let _: () = msg_send![app, finishLaunching];
    }
    let mut fixture = Fixture(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--tab-group-fixture")
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let mut line = String::new();
    BufReader::new(fixture.0.stdout.take().ok_or("Sin salida del fixture")?)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let ids: Vec<u32> = line
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if ids.len() != 3 {
        return Err("El fixture no creó tres ventanas".into());
    }
    pump(0.3);
    let pid = fixture.0.id() as i32;
    let (space, area) = spaces::visible_desktops()
        .into_iter()
        .filter_map(|key| area(key.0).map(|area| (key, area)))
        .find(|(key, _)| spaces::window_space(ids[0]) == Some(key.1))
        .ok_or("Sin escritorio visible para el fixture")?;
    let target: Retained<AnyObject> = unsafe {
        let mut builder = ClassBuilder::new(c"AMSpaceTabDiagnostic", class!(NSObject))
            .ok_or("Clase de diagnóstico duplicada")?;
        builder.add_method(
            sel!(selectTileTab:),
            select as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(closeTileTab:),
            close as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        builder.add_method(
            sel!(dragTileTab:),
            drag as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        msg_send![builder.register(), new]
    };
    let mut manager = Manager::with_tab_target(target);
    manager.active_areas.push((space, area));
    manager.animated = Some(false);
    for id in &ids {
        let element = ax::window_for_id(pid, *id).ok_or("AX no encontró una ventana temporal")?;
        let (p, s) = ax::geometry(element.0).ok_or("AX no publicó geometría")?;
        manager.windows.insert(
            *id,
            Managed {
                pid,
                space,
                original: NSRect::new(p, s),
                original_area: area,
                element,
            },
        );
    }
    let mut desktop = Desktop::configured(Layout::Spiral, None);
    desktop.reconcile_at(&ids, area, 10., None);
    let target_frame = desktop
        .frames(area, 10.)
        .into_iter()
        .find(|(id, _)| *id == ids[0])
        .unwrap()
        .1;
    let point = NSPoint::new(
        target_frame.origin.x + target_frame.size.width / 2.,
        target_frame.origin.y + target_frame.size.height / 4.,
    );
    let intent = drop_intent(area, &desktop.frames(area, 10.), ids[2], point, false, true)
        .ok_or("Sin intención de agrupación")?;
    if intent != DropIntent::Group(ids[0]) {
        return Err(format!("Intención incorrecta: {intent:?}"));
    }
    desktop = arrival_proposal(&desktop, ids[2], Some(intent), area, None)
        .ok_or("Agrupación rechazada")?;
    let frames = desktop.window_frames(area, 10.);
    for (id, frame) in &frames {
        if !ax::set_frame(manager.windows[id].element.0, *frame) {
            return Err("AX rechazó el destino activo".into());
        }
    }
    for (id, frame) in hidden_tab_frames(&desktop, &frames) {
        manager.place_hidden_tab(id, frame);
    }
    manager.desktops.insert(space, desktop.clone());
    manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
    STATE.with(|state| *state.borrow_mut() = Some(manager));
    let _panels = Panels;
    pump(0.2);
    let slot = frames.iter().find(|(id, _)| *id == ids[2]).unwrap().1;
    for id in [ids[0], ids[2]] {
        let element = ax::window_for_id(pid, id).ok_or("Fixture desaparecido")?;
        let (p, s) = ax::geometry(element.0).ok_or("Geometría desaparecida")?;
        if !near(NSRect::new(p, s), slot) {
            return Err(format!("Miembro {id} fuera del hueco: {p:?} {s:?}"));
        }
    }
    let _cursor = Cursor(pointer());
    for round in 0..40 {
        // Drain every round: icons/titles must survive their getter's pool.
        let id = ids[if round % 2 == 0 { 0 } else { 2 }];
        let button: Retained<AnyObject> = STATE.with(|state| unsafe {
            let state = state.borrow();
            let manager = state.as_ref().unwrap();
            let scroll: Obj = msg_send![&*manager.strips[0].panel, contentView];
            let document: Obj = msg_send![scroll, documentView];
            let button: Option<Retained<AnyObject>> = msg_send![document, viewWithTag:id as isize];
            button.ok_or("No hay botón para el miembro")
        })?;
        if round == 0 {
            let point = unsafe {
                let bounds: NSRect = msg_send![&button, bounds];
                let center = NSPoint::new(bounds.size.width / 2., bounds.size.height / 2.);
                let local: NSPoint =
                    msg_send![&button, convertPoint:center, toView:ptr::null::<AnyObject>()];
                let window: Obj = msg_send![&button, window];
                let screen: NSPoint = msg_send![window, convertPointToScreen:local];
                NSPoint::new(screen.x, primary_height() - screen.y)
            };
            post_mouse(1, point)?;
            post_mouse(2, point)?;
        } else {
            autoreleasepool(|_| unsafe {
                let _: () = msg_send![&button, performClick:ptr::null::<AnyObject>()];
            });
        }
        pump(0.1);
        if SELECTED.with(|selected| *selected.borrow()) != Some(id) {
            return Err(format!("El botón no seleccionó {id}"));
        }
        if ws::front_window(pid).is_none_or(|window| window.id != id) {
            return Err(format!("La selección no elevó {id}"));
        }
        autoreleasepool(|_| {
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                let manager = state.as_mut().unwrap();
                let desktop = manager.desktops[&space].clone();
                let frames = desktop.window_frames(area, 10.);
                manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
            })
        });
    }
    let detached = ids[0];
    let button: Retained<AnyObject> = STATE.with(|state| unsafe {
        let state = state.borrow();
        let manager = state.as_ref().unwrap();
        let scroll: Obj = msg_send![&*manager.strips[0].panel, contentView];
        let document: Obj = msg_send![scroll, documentView];
        let button: Option<Retained<AnyObject>> = msg_send![document,viewWithTag:detached as isize];
        button.ok_or("Sin botón para arrastrar")
    })?;
    let point = unsafe {
        let bounds: NSRect = msg_send![&button, bounds];
        let local: NSPoint = msg_send![&button,convertPoint:NSPoint::new(bounds.size.width / 2., bounds.size.height / 2.),toView:ptr::null::<AnyObject>()];
        let window: Obj = msg_send![&button, window];
        let screen: NSPoint = msg_send![window,convertPointToScreen:local];
        NSPoint::new(screen.x, primary_height() - screen.y)
    };
    post_mouse(1, point)?;
    post_mouse(6, NSPoint::new(point.x + 20., point.y + 12.))?;
    post_mouse(6, NSPoint::new(point.x + 80., point.y + 40.))?;
    post_mouse(2, NSPoint::new(point.x + 80., point.y + 40.))?;
    pump(0.2);
    if DETACHED.with(|value| *value.borrow()) != Some(detached) {
        return Err("Arrastrar la pestaña no confirmó su separación".into());
    }
    let detached_frames = STATE.with(|state| -> Result<Vec<(u32, NSRect)>, String> {
        let state = state.borrow();
        let manager = state.as_ref().unwrap();
        let desktop = &manager.desktops[&space];
        if desktop.tab_role(detached).is_some() || desktop.order.len() != 3 {
            return Err("La pestaña arrastrada no tiene un hueco independiente".into());
        }
        let frames = desktop.window_frames(area, 10.);
        for (id, frame) in &frames {
            if !ax::set_frame(manager.windows[id].element.0, *frame) {
                return Err("AX rechazó el hueco independiente".into());
            }
        }
        Ok(frames)
    })?;
    pump(0.15);
    for (id, frame) in detached_frames {
        let element = ax::window_for_id(pid, id).ok_or("Fixture separado desaparecido")?;
        let (position, size) = ax::geometry(element.0).ok_or("Sin geometría separada")?;
        if !near(NSRect::new(position, size), frame) {
            return Err(format!("La ventana separada {id} no llegó a su hueco"));
        }
    }
    let logical_slot = STATE.with(|state| -> Result<NSRect, String> {
        let mut state = state.borrow_mut();
        let manager = state.as_mut().unwrap();
        let mut desktop = arrival_proposal(
            &manager.desktops[&space],
            detached,
            Some(DropIntent::Group(ids[2])),
            area,
            None,
        )
        .ok_or("No se pudo restaurar el grupo para comprobar cierre")?;
        desktop.select_tab(ids[2]);
        let frames = desktop.window_frames(area, 10.);
        let logical_slot = desktop
            .frames(area, 10.)
            .into_iter()
            .find(|(id, _)| *id == ids[2])
            .unwrap()
            .1;
        for (id, frame) in frames
            .iter()
            .copied()
            .chain(hidden_tab_frames(&desktop, &frames))
        {
            if !ax::set_frame(manager.windows[&id].element.0, frame) {
                return Err("No se pudo restaurar la geometría agrupada".into());
            }
        }
        manager.desktops.insert(space, desktop.clone());
        let count = manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
        manager.trim_overlays(count, 0);
        Ok(logical_slot)
    })?;
    println!(
        "PASS: arrastre de pestaña con CGEvent, separación y geometría independiente verificadas."
    );
    STATE.with(|state| {
        let state = state.borrow();
        let manager = state.as_ref().unwrap();
        let panel = &manager.strips[0].panel;
        let id: isize = unsafe { msg_send![&**panel, windowNumber] };
        println!("SURFACE: agrupación, geometría compartida y 40 selecciones nativas (primer clic CGEvent); panel={id}, fixture_pid={pid}, ids={ids:?}");
    });
    // Keep the actual surface visible briefly for an external screenshot.
    pump(3.);
    let closing = ids[2];
    STATE.with(|state| {
        let state = state.borrow();
        let panel = &state.as_ref().unwrap().strips[0].panel;
        let content: Obj = unsafe { msg_send![&**panel, contentView] };
        let document: Obj = unsafe { msg_send![content, documentView] };
        let button: Retained<AnyObject> =
            unsafe { msg_send![document,viewWithTag:-(closing as isize) - 2] };
        drop(state);
        unsafe {
            let _: () = msg_send![&button,performClick:ptr::null::<AnyObject>()];
        }
    });
    if CLOSE_REQUESTED.with(|requested| *requested.borrow()) != Some(closing) {
        return Err("El control de cierre no solicitó el cierre nativo".into());
    }
    pump(0.2);
    if ws::ordered_in(closing) != Some(false)
        && ws::all_window_ids().is_none_or(|all| all.contains(&(pid, closing)))
    {
        return Err("La ventana temporal no se cerró".into());
    }
    STATE.with(|state| -> Result<(), String> {
        let mut state = state.borrow_mut();
        let manager = state.as_mut().unwrap();
        manager.forget_window(closing);
        let desktop = manager.desktops[&space].clone();
        let frames = desktop.window_frames(area, 10.);
        if !frames
            .iter()
            .any(|(id, frame)| *id == ids[0] && near(*frame, logical_slot))
        {
            return Err("Cerrar la activa no conservó el hueco de su hermana".into());
        }
        let count = manager.draw_strips(space, &desktop, &frames, primary_height(), 0, false);
        manager.trim_overlays(count, 0);
        if manager
            .strips
            .iter()
            .any(|strip| unsafe { msg_send![&*strip.panel, isVisible] })
        {
            return Err("La tira vacía quedó visible tras cerrar".into());
        }
        Ok(())
    })?;
    println!(
        "PASS: cierre nativo, promoción en el mismo hueco y retirada de la tira; preferencias sin modificar."
    );
    Ok(())
}
