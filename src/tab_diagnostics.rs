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
        STATE.with(|state| {
            if let Some(manager) = state.borrow_mut().as_mut() {
                if let Some(button) = button {
                    let id: isize = msg_send![&button, tag];
                    if manager.tab_drag(Retained::as_ptr(&button) as Obj, phase) && phase == 3 {
                        DETACHED.with(|detached| *detached.borrow_mut() = u32::try_from(id).ok());
                    }
                } else {
                    manager.cancel_tab_drag();
                }
            }
        });
    }
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
        let target: Retained<AnyObject> = msg_send![builder.register(), new];
        let names = [
            ("Primera ventana".into(), None),
            ("Segunda ventana".into(), None),
            ("Tercera ventana".into(), None),
        ];
        let frame = tab_bar_frame(rect(300., 330., 620., 400.), 620.);
        let panel = new_overlay(frame, primary_height());
        let content = strip_content(&names, &[1, 2, 3], 1, frame.size.width, Some(&target));
        let _: () = msg_send![&panel,setIgnoresMouseEvents:false];
        let _: () = msg_send![&panel,setContentView:&*content];
        let _: () = msg_send![&panel, orderFrontRegardless];
        pump(0.1);
        let document: Obj = msg_send![&content, documentView];
        let button: Retained<AnyObject> = msg_send![document,viewWithTag:2isize];
        let bounds: NSRect = msg_send![&button, bounds];
        let center: NSPoint = msg_send![&button,convertPoint:NSPoint::new(bounds.size.width / 2., bounds.size.height / 2.),toView:ptr::null::<AnyObject>()];
        let number: isize = msg_send![&panel, windowNumber];
        let queue = |events: &[(usize, f64, f64)]| -> Result<(), String> {
            for (index, (kind, x, y)) in events.iter().enumerate() {
                let process: Obj = msg_send![class!(NSProcessInfo), processInfo];
                let uptime: f64 = msg_send![process, systemUptime];
                let event: Option<Retained<AnyObject>> = msg_send![class!(NSEvent),
                    mouseEventWithType:*kind,
                    location:NSPoint::new(center.x + x, center.y + y),
                    modifierFlags:0usize, timestamp:uptime + index as f64 * 0.01,
                    windowNumber:number, context:ptr::null::<AnyObject>(),
                    eventNumber:index as isize, clickCount:1isize, pressure:if *kind == 2 { 0f32 } else { 1f32 }];
                let event = event.ok_or("No se pudo crear el evento local")?;
                let _: () = msg_send![app,postEvent:&*event,atStart:false];
            }
            Ok(())
        };
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
        let bounds: NSRect = msg_send![&content, bounds];
        let bitmap: Option<Retained<AnyObject>> =
            msg_send![&content,bitmapImageRepForCachingDisplayInRect:bounds];
        let bitmap = bitmap.ok_or("Sin bitmap de la tira")?;
        let _: () = msg_send![&content,cacheDisplayInRect:bounds,toBitmapImageRep:&*bitmap];
        let properties: Obj = msg_send![class!(NSDictionary), dictionary];
        let data: Option<Retained<AnyObject>> =
            msg_send![&bitmap,representationUsingType:4usize,properties:properties];
        let data = data.ok_or("Sin PNG de la tira")?;
        std::fs::create_dir_all(".cache").map_err(|error| error.to_string())?;
        let saved: bool = msg_send![&data,writeToFile:&*NSString::from_str(".cache/tab-controls.png"),atomically:true];
        let _: () = msg_send![&panel,orderOut:ptr::null::<AnyObject>()];
        if !saved {
            return Err("No se pudo guardar la captura".into());
        }
        println!(
            "PASS: clic, umbral de arrastre, inicio/fin único y cancelación AppKit; captura .cache/tab-controls.png. Sin AX ni ventanas ajenas."
        );
        Ok(())
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
    manager.draw_strips(&desktop, &frames, primary_height(), 0);
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
                manager.draw_strips(&desktop, &frames, primary_height(), 0);
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
        let count = manager.draw_strips(&desktop, &frames, primary_height(), 0);
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
    let element = ax::window_for_id(pid, closing).ok_or("Pestaña de cierre no encontrada")?;
    let close = ax::attr(element.0, "AXCloseButton").ok_or("Sin botón nativo de cierre")?;
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementPerformAction(element: ax::Cf, action: ax::Cf) -> i32;
    }
    let action = NSString::from_str("AXPress");
    if unsafe { AXUIElementPerformAction(close.0, Retained::as_ptr(&action).cast()) } != 0 {
        return Err("El cierre nativo fue rechazado".into());
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
        let count = manager.draw_strips(&desktop, &frames, primary_height(), 0);
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
