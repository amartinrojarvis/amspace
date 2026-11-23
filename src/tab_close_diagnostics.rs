//! Isolated tab workflows: elevated disposable windows, never user tiles.
use super::diagnostics::{Fixture, pump};
use super::*;
use objc2::runtime::Bool;
use std::{
    cell::{Cell, RefCell},
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

const UPDATED_TITLE: &str = "AMSpace · nuevo título del fixture";

thread_local! {
    static WINDOWS: RefCell<Vec<Retained<AnyObject>>> = const { RefCell::new(Vec::new()) };
    static ALLOW_CLOSE: Cell<bool> = const { Cell::new(false) };
}

extern "C-unwind" fn should_close(_: Obj, _: Sel, _: Obj) -> Bool {
    if ALLOW_CLOSE.get() {
        Bool::YES
    } else {
        Bool::NO
    }
}

extern "C-unwind" fn command(_: Obj, _: Sel, value: Obj) {
    let command = unsafe { (&*(value as *const NSString)).to_string() };
    WINDOWS.with(|windows| unsafe {
        let windows = windows.borrow();
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        match command.as_str() {
            "allow" => ALLOW_CLOSE.set(true),
            "rename" => {
                let _: () = msg_send![&*windows[0], setTitle:&*NSString::from_str(UPDATED_TITLE)];
            }
            "minimize" => {
                let _: () = msg_send![&*windows[0], miniaturize:ptr::null::<AnyObject>()];
            }
            "restore" => {
                let _: () = msg_send![&*windows[0], deminiaturize:ptr::null::<AnyObject>()];
            }
            "hide" => {
                let _: () = msg_send![app, hide:ptr::null::<AnyObject>()];
            }
            "show" => {
                let _: () = msg_send![app, unhideWithoutActivation];
                for window in windows.iter() {
                    let _: () = msg_send![&**window, orderFrontRegardless];
                }
            }
            _ => return,
        }
        println!("OK");
        let _ = std::io::stdout().flush();
    });
}

pub fn run_fixture() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(180));
        unsafe { libc::_exit(0) }
    });
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app, setActivationPolicy:1isize];
        let _: () = msg_send![app, finishLaunching];
        let mut builder = ClassBuilder::new(c"AMSpaceCloseFixture", class!(NSObject)).unwrap();
        builder.add_method(
            sel!(windowShouldClose:),
            should_close as extern "C-unwind" fn(Obj, Sel, Obj) -> Bool,
        );
        builder.add_method(
            sel!(fixtureCommand:),
            command as extern "C-unwind" fn(Obj, Sel, Obj),
        );
        let delegate: Retained<AnyObject> = msg_send![builder.register(), new];
        // The primary monitor may be occupied by a fullscreen app. Put the
        // disposable windows on a monitor with an actual visible desktop.
        let areas: Vec<_> = spaces::visible_desktops()
            .into_iter()
            .filter_map(|(display, _)| area(display))
            .collect();
        let two_displays = std::env::args().any(|arg| arg == "--second-scene-display");
        WINDOWS.with(|windows| {
            let mut windows = windows.borrow_mut();
            for index in 0..if two_displays { 5 } else { 3 } {
                let fixture_origin = areas.get(usize::from(index >= 3)).map_or((200., 220.), |r| {
                    (
                        r.origin.x + 100.,
                        primary_height() - r.origin.y - r.size.height + 100.,
                    )
                });
                let window: Allocated<AnyObject> = msg_send![class!(NSWindow), alloc];
                let frame = rect(
                    fixture_origin.0 + 40. * index as f64,
                    fixture_origin.1,
                    420.,
                    280.,
                );
                let window: Retained<AnyObject> = msg_send![window, initWithContentRect:frame, styleMask:15usize, backing:2usize, defer:false];
                let _: () = msg_send![&window, setReleasedWhenClosed:false];
                let _: () = msg_send![&window, setLevel:3isize];
                let _: () = msg_send![&window, setTabbingMode:2isize];
                let _: () = msg_send![&window, setDelegate:&*delegate];
                let _: () = msg_send![&window, setTitle:&*NSString::from_str(&format!("AMSpace · cierre temporal {index}"))];
                let _: () = msg_send![&window, orderFrontRegardless];
                if two_displays {
                    let global: NSRect = msg_send![&window, frameRectForContentRect:frame];
                    let _: () = msg_send![&window, setFrame:global, display:false];
                }
                windows.push(window);
            }
            let ids: Vec<String> = windows.iter().map(|window| {
                let id: isize = msg_send![&**window, windowNumber];
                id.to_string()
            }).collect();
            println!("{}", ids.join(" "));
            let _ = std::io::stdout().flush();
        });
        let target = Retained::as_ptr(&delegate) as usize;
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                objc2::rc::autoreleasepool(|_| {
                    let value = NSString::from_str(&line);
                    let _: () = msg_send![target as Obj, performSelectorOnMainThread:sel!(fixtureCommand:), withObject:&*value, waitUntilDone:true];
                });
            }
        });
        let _: () = msg_send![app, run];
    }
}

struct RestoreFocus(Option<Retained<AnyObject>>);
impl Drop for RestoreFocus {
    fn drop(&mut self) {
        if let Some(app) = &self.0 {
            unsafe {
                let _: bool = msg_send![&**app, activateWithOptions:0usize];
            }
        }
    }
}

pub(crate) fn observe_scene_reentry(manager: &mut Manager, defaults: Obj) {
    if manager.space_reentry.is_some() {
        let active = manager.active_areas.clone();
        let sample = manager.reentry_sample(defaults, &active);
        manager.observe_reentry(sample, Instant::now());
    }
}

fn check_scene_displays(
    manager: &mut Manager,
    pid: i32,
    ids: &[u32],
    first: Workspace,
) -> Result<(), String> {
    let areas: Vec<_> = spaces::visible_desktops()
        .into_iter()
        .filter_map(|key| Some((key, area(key.0)?)))
        .collect();
    let mut second = None;
    for &id in &ids[3..] {
        let element = ax::window_for_id(pid, id).ok_or("Falta la ventana AX del otro monitor")?;
        let (p, s) = ax::geometry(element.0).ok_or("Sin geometría del otro monitor")?;
        let frame = NSRect::new(p, s);
        let native = spaces::window_space(id).ok_or("Sin Space de la segunda ventana")?;
        let key = workspace_at_frame(native, frame, &areas)
            .filter(|key| key.0 != first.0 && second.is_none_or(|other| *key == other))
            .ok_or("El fixture no apareció en la segunda pantalla física")?;
        second = Some(key);
        let original_area = area(key.0).ok_or("Sin área útil del otro monitor")?;
        manager.windows.insert(
            id,
            Managed {
                pid,
                space: key,
                original: frame,
                original_area,
                element,
            },
        );
    }
    let second = second.ok_or("Se necesitan dos pantallas para este escenario")?;
    let mut desktop = Desktop::configured(Layout::Spiral, None);
    let area = area(second.0).ok_or("Sin área útil del otro monitor")?;
    desktop.reconcile_at(&ids[3..], area, 10., None);
    manager.desktops.insert(second, desktop);
    unsafe {
        let suite = NSString::from_str(&format!("amspace.scene-displays.{}", std::process::id()));
        let alloc: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let prefs: Retained<AnyObject> = msg_send![alloc, initWithSuiteName:&*suite];
        let defaults = Retained::as_ptr(&prefs) as Obj;
        save(defaults, "tile.enabled", 1);
        let result: Result<(), String> = (|| {
            let capture = manager.capture_scene(defaults)?;
            if capture.displays.len() != 2 || capture.windows.len() != 5 {
                return Err("La escena no capturó los cinco puestos de dos monitores".into());
            }
            let id = crate::scene_library::save(defaults, "Dos pantallas", &capture.displays)?;
            let scene =
                crate::scene_library::entry(defaults, &id).ok_or("Falta la escena persistida")?;
            let review = manager.review_scene(defaults, &scene.displays)?;
            let choices = crate::ui::diagnostics::quick_choices(&review, None);
            let absent = choices[0].ok_or("Falta la primera ventana del fixture")?;
            let held = manager.windows.remove(&absent).unwrap();
            let partial = manager.review_scene(defaults, &scene.displays)?;
            let partial = crate::ui::diagnostics::quick_choices(&partial, None);
            let split = scene.displays[0].bundles.len();
            let missing = partial[..split]
                .iter()
                .filter(|slot| slot.is_none())
                .count();
            if partial[split..] != choices[split..] || missing != 1 {
                return Err("Un puesto vacío tomó la ventana del otro monitor".into());
            }
            manager.windows.insert(absent, held);
            manager.apply_scene(defaults, &review, &choices)?;
            let mut expected = Vec::new();
            for &id in ids {
                let w = &manager.windows[&id];
                let (p, s) = ax::geometry(w.element.0).ok_or("Sin marco colocado")?;
                expected.push((id, w.space, NSRect::new(p, s)));
                let drifted = rect(p.x + 2., p.y + 1., s.width - 3., s.height - 2.);
                if !ax::set_frame(w.element.0, drifted) {
                    return Err("No se pudo desplazar la ventana de prueba".into());
                }
            }
            let review = manager.review_scene(defaults, &scene.displays)?;
            manager.apply_scene(defaults, &review, &choices)?;
            for (id, key, frame) in expected {
                let w = &manager.windows[&id];
                let (p, s) = ax::geometry(w.element.0).ok_or("Sin marco final")?;
                let space = spaces::window_space(id).ok_or("Sin Space final")?;
                let actual = NSRect::new(p, s);
                let initial = if ids[..3].contains(&id) {
                    first
                } else {
                    second
                };
                if workspace_at_frame(space, actual, &areas) != Some(key)
                    || actual != frame
                    || key != initial
                {
                    return Err(format!(
                        "La escena no recuperó monitor, Space y marco de {id}"
                    ));
                }
            }
            println!(
                "PASS: guardado y carga de cinco ventanas en dos monitores físicos ({first:?}, {second:?}); posición, tamaño y pertenencia exactos."
            );
            Ok(())
        })();
        let _: () = msg_send![&prefs, removePersistentDomainForName:&*suite];
        result?;
    }
    manager.desktops.remove(&second);
    for id in &ids[3..] {
        manager.forget_window(*id);
    }
    Ok(())
}

pub fn check(scenes_only: bool) -> Result<(), String> {
    if !ax::trusted() {
        return Err(
            "El diagnóstico de cierre necesita Accesibilidad; no modifica permisos.".into(),
        );
    }
    let _focus = unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app, setActivationPolicy:1isize];
        let _: () = msg_send![app, finishLaunching];
        let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        RestoreFocus(msg_send![workspace, frontmostApplication])
    };
    let two_displays = scenes_only && spaces::visible_desktops().len() >= 2;
    let mut fixture = Fixture(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--tab-close-fixture")
            .args(two_displays.then_some("--second-scene-display"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let mut input = fixture.0.stdin.take().ok_or("Sin entrada de fixture")?;
    let mut output = BufReader::new(fixture.0.stdout.take().ok_or("Sin salida de fixture")?);
    let mut line = String::new();
    output.read_line(&mut line).map_err(|e| e.to_string())?;
    let ids = line
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if ids.len() != if two_displays { 5 } else { 3 } {
        return Err("El fixture no creó todas las ventanas previstas".into());
    }
    pump(0.15);
    let pid = fixture.0.id() as i32;
    let first = ax::window_for_id(pid, ids[0]).ok_or("AX no encontró el fixture")?;
    let (position, size) = ax::geometry(first.0).ok_or("Sin geometría AX")?;
    let native_space = spaces::window_space(ids[0]).ok_or("Sin Space del fixture")?;
    let (space, area) = spaces::visible_desktops()
        .into_iter()
        .filter(|key| key.1 == native_space)
        .filter_map(|key| area(key.0).map(|area| (key, area)))
        .max_by(|(_, a), (_, b)| {
            overlap(NSRect::new(position, size), *a)
                .total_cmp(&overlap(NSRect::new(position, size), *b))
        })
        .ok_or("Sin área visible del fixture")?;
    let mut manager = Manager::default();
    manager.active_areas.push((space, area));
    for id in &ids[..3] {
        let element = ax::window_for_id(pid, *id).ok_or("Falta miembro AX")?;
        let (p, s) = ax::geometry(element.0).ok_or("Falta geometría AX")?;
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
    desktop.reconcile_at(&ids[..3], area, 10., None);
    if scenes_only {
        manager.desktops.insert(space, desktop);
        if two_displays {
            check_scene_displays(&mut manager, pid, &ids, space)?;
        }
        return crate::ui::diagnostics::check_scene_fixture(manager, &ids[..3], pid, area);
    }
    for (target, id) in ids.iter().zip(&ids[1..]) {
        desktop = desktop
            .proposed_drop(*id, DropIntent::Group(*target), area, 10.)
            .ok_or("No se pudo agrupar fixture")?;
    }
    manager.desktops.insert(space, desktop);
    super::drag_diagnostics::check(&mut manager, &ids, space, area)?;
    let send = |command: &str,
                input: &mut std::process::ChildStdin,
                output: &mut BufReader<std::process::ChildStdout>|
     -> Result<(), String> {
        writeln!(input, "{command}").map_err(|e| e.to_string())?;
        let mut reply = String::new();
        output.read_line(&mut reply).map_err(|e| e.to_string())?;
        if reply.trim() != "OK" {
            return Err(format!("Fixture no confirmó {command}: {reply}"));
        }
        pump(0.12);
        Ok(())
    };
    send("rename", &mut input, &mut output)?;
    let frames = manager.desktops[&space].window_frames(area, 10.);
    manager.draw_strips(space, &frames, primary_height(), 0, false);
    unsafe {
        let scroll: Obj = msg_send![&*manager.strips[0].panel, contentView];
        let document: Obj = msg_send![scroll, documentView];
        let tab: Obj = msg_send![document, viewWithTag:ids[0] as isize];
        let title: Option<Retained<NSString>> = msg_send![tab, title];
        if title.is_none_or(|value| value.to_string() != UPDATED_TITLE) {
            return Err("La pestaña conservó el título AX anterior al cambio.".into());
        }
    }
    println!("PASS: el título nuevo de la ventana llegó al botón de la pestaña.");
    let retire = |manager: &mut Manager| -> Result<(), String> {
        let identities = ws::all_window_ids().ok_or("Sin instantánea del compositor")?;
        manager.retire_closed_windows(&HashSet::new(), Some(&identities));
        Ok(())
    };
    if !manager.close_tab(ids[0]) {
        return Err("La pestaña oculta no aceptó la petición AX".into());
    }
    pump(0.15);
    retire(&mut manager)?;
    if manager.windows.len() != 3 || manager.desktops[&space].groups()[0].members.len() != 3 {
        return Err("Un cierre vetado eliminó un miembro".into());
    }
    for command in ["minimize", "restore", "hide", "show"] {
        send(command, &mut input, &mut output)?;
        retire(&mut manager)?;
        if manager.windows.len() != 3 || manager.desktops[&space].groups()[0].members.len() != 3 {
            return Err(format!("{command} se confundió con destruir la ventana"));
        }
    }
    send("allow", &mut input, &mut output)?;
    for index in 0..2 {
        let closing = if index == 0 {
            ids[1]
        } else {
            manager.desktops[&space].order[0]
        };
        let was_active = manager.desktops[&space].tab_role(closing);
        if was_active != Some(index == 1) {
            return Err("La prueba no cubre cierre inactivo y activo".into());
        }
        if index == 0 {
            crate::ui::diagnostics::check_reentrant_close(&mut manager, closing)?;
        } else if !manager.close_tab(closing) {
            return Err(format!("Cierre rechazado: {closing}"));
        }
        pump(0.15);
        if ws::ordered_in(closing) != Some(false)
            || !ax::destroyed(manager.windows[&closing].element.0)
        {
            return Err("El fixture no confirmó la retirada y destrucción AX".into());
        }
        retire(&mut manager)?;
        if manager.windows.contains_key(&closing)
            || manager.desktops[&space].tab_role(closing).is_some()
        {
            return Err("La reconciliación dejó una pestaña fantasma".into());
        }
    }
    let remaining = &manager.desktops[&space];
    let [survivor] = remaining.order.as_slice() else {
        return Err("El cierre no conservó exactamente un hueco superviviente".into());
    };
    if manager.windows.len() != 1
        || !manager.windows.contains_key(survivor)
        || ws::ordered_in(*survivor) != Some(true)
        || remaining.window_frames(area, 10.) != remaining.frames(area, 10.)
    {
        return Err("El último miembro no recuperó el hueco sin cabecera".into());
    }
    println!(
        "PASS: × reentrante entregada una vez; caducidad/pausa conservan la ventana; petición AX sobre pestaña inactiva/activa, veto nativo conservado, minimizar/ocultar no destruyen miembros, cierre retenido retirado por reconciliación y singleton sin cabecera. Solo fixtures elevados; preferencias intactas."
    );
    Ok(())
}
