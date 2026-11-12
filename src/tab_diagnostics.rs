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
                signature: names.to_vec(),
                members: vec![1, 2, 3],
                active: 1,
                built: frame,
                frame,
            }],
            ..Manager::default()
        };
        for window in [rect(324., 342., 620., 400.), slot] {
            manager.draw_strips(&desktop, &[(2, window)], primary_height(), 0, false);
            let root: Obj = msg_send![&panel, contentView];
            let actual: NSRect = msg_send![&panel, frame];
            let expected = appkit(tab_bar_frame(window, total, false), primary_height());
            if !ptr::eq(root, Retained::as_ptr(&content)) || actual != expected {
                return Err("Mover la tira reconstruyó los controles o perdió su posición.".into());
            }
        }
        manager.show_badge(DropIntent::Group(2), slot, primary_height());
        let badge: Retained<AnyObject> = msg_send![&*manager.badges[0].panel, contentView];
        manager.show_badge(
            DropIntent::Group(2),
            rect(324., 342., 620., 400.),
            primary_height(),
        );
        let moved_badge: Obj = msg_send![&*manager.badges[0].panel, contentView];
        if !ptr::eq(moved_badge, Retained::as_ptr(&badge)) {
            return Err("Mover el indicador de agrupación reconstruyó su contenido.".into());
        }
        manager.show_badge(DropIntent::Screen(Zone::Left), slot, primary_height());
        let changed_badge: Obj = msg_send![&*manager.badges[0].panel, contentView];
        if ptr::eq(changed_badge, Retained::as_ptr(&badge)) {
            return Err("Cambiar la intención no actualizó el indicador.".into());
        }
        manager.trim_overlays(1, 0);
        capture_control(Retained::as_ptr(&content) as Obj, ".cache/tab-controls.png")?;
        render_tab_presentation(&names, &target)?;
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
            "PASS: selección, arrastre/cancelación, cierre individual, hover y reutilización de controles al mover la tira; captura .cache/tab-controls.png. Sin AX ni ventanas ajenas."
        );
        Ok(())
    }
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
            let strip = strip_content(names, &[1, 2, 3], 1, frame.size.width, Some(target));
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
    manager.draw_strips(&desktop, &frames, primary_height(), 0, false);
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
                manager.draw_strips(&desktop, &frames, primary_height(), 0, false);
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
        let count = manager.draw_strips(&desktop, &frames, primary_height(), 0, false);
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
        let count = manager.draw_strips(&desktop, &frames, primary_height(), 0, false);
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
