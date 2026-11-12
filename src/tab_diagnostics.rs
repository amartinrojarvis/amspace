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
    let frames = desktop.frames(area, 10.);
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
                let frames = desktop.frames(area, 10.);
                manager.draw_strips(&desktop, &frames, primary_height(), 0);
            })
        });
    }
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
        let frames = desktop.frames(area, 10.);
        if !frames
            .iter()
            .any(|(id, frame)| *id == ids[0] && near(*frame, slot))
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
