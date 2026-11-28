//! Isolated tab workflows: elevated disposable windows, never user tiles.
use super::diagnostics::{Fixture, pump};
use super::*;
use objc2::runtime::Bool;
use objc2_foundation::ns_string;
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
            "focus-last" => {
                let _: () = msg_send![app, activateIgnoringOtherApps:true];
                let _: () = msg_send![&**windows.last().unwrap(), makeKeyAndOrderFront:ptr::null::<AnyObject>()];
            }
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
        let four_windows = std::env::args().any(|arg| arg == "--shared-app-tabs");
        let mixed = std::env::args().any(|arg| arg == "--mixed-app-tabs");
        let fixed = std::env::args().any(|arg| arg == "--float-overlap-fixture");
        let manual = std::env::args().any(|arg| arg == "--resizable-float");
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app, setActivationPolicy:if four_windows || mixed || fixed { 0isize } else { 1isize }];
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
        let count = if two_displays {
            5
        } else if four_windows {
            4
        } else if mixed {
            2
        } else {
            3
        };
        WINDOWS.with(|windows| {
            let mut windows = windows.borrow_mut();
            for index in 0..count {
                let fixture_origin = areas.get(usize::from(two_displays && index >= 3)).map_or((200., 220.), |r| {
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
                let window: Retained<AnyObject> = msg_send![window, initWithContentRect:frame, styleMask:if fixed && !manual { 3usize } else { 15usize }, backing:2usize, defer:false];
                let _: () = msg_send![&window, setReleasedWhenClosed:false];
                let _: () = msg_send![&window, setLevel:if fixed && !manual { 0isize } else { 3isize }];
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

/// Admit only AX windows of the disposable launch fixture to the isolated manager.
pub(crate) fn admit_launched_fixture(manager: &mut Manager, pid: i32, area: NSRect) -> Vec<u32> {
    let mut ids = Vec::new();
    for (id, element) in ax::application_windows(pid).unwrap_or_default() {
        let Some((position, size)) = ax::geometry(element.0) else {
            continue;
        };
        let frame = NSRect::new(position, size);
        let Some(space) = spaces::visible_desktops().into_iter().find(|key| {
            spaces::window_space(id) == Some(key.1)
                && self::area(key.0) == Some(area)
                && overlap(frame, area) > 0.
        }) else {
            continue;
        };
        if let std::collections::hash_map::Entry::Vacant(slot) = manager.windows.entry(id) {
            slot.insert(Managed {
                pid,
                space,
                original: frame,
                original_area: area,
                element,
            });
            manager.floating.insert(id);
        }
        ids.push(id);
    }
    ids.sort_unstable();
    ids
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
    let mut expected = Vec::with_capacity(ids.len());
    for (key, distribution) in [
        (first, "2 4 3 0 0 h0.3335 w0 h0.5 w1 w2"),
        (second, "2 3 2 0 0 v0.725 w0 w1"),
    ] {
        let desktop = manager.desktops.get_mut(&key).unwrap();
        if !desktop.restore_distribution(distribution) {
            return Err("No se pudo preparar el reparto asimétrico del fixture".into());
        }
        let area = areas.iter().find(|(k, _)| *k == key).unwrap().1;
        for (id, target) in desktop.window_frames(area, 10.) {
            let window = manager.windows[&id].element.0;
            if !ax::set_frame(window, target) {
                return Err("No se pudo preparar el marco inicial del fixture".into());
            }
            let (p, s) = ax::geometry(window).ok_or("Sin marco antes de guardar")?;
            let frame = NSRect::new(p, s);
            if frame != ax::integral_frame(target) {
                return Err("El fixture no alcanzó el marco inicial".into());
            }
            expected.push((id, key, frame));
        }
    }
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
            for &(id, _, frame) in &expected {
                let w = &manager.windows[&id];
                let p = frame.origin;
                let s = frame.size;
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

fn front_pid() -> i32 {
    unsafe {
        let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
        let front: Obj = msg_send![workspace, frontmostApplication];
        msg_send![front, processIdentifier]
    }
}

fn check_mixed_app_tabs() -> Result<(), String> {
    fn spawn() -> Result<(Fixture, [u32; 2]), String> {
        let mut fixture = Fixture(
            Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                .args(["--tab-close-fixture", "--mixed-app-tabs"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?,
        );
        let mut line = String::new();
        BufReader::new(fixture.0.stdout.as_mut().unwrap())
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        let ids = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "El fixture no creó dos ventanas")?;
        Ok((fixture, ids))
    }

    let (shared, [hidden, chosen]) = spawn()?;
    let (mut other, [cover, previous]) = spawn()?;
    let shared_pid = shared.0.id() as i32;
    let other_pid = other.0.id() as i32;
    let ids = [hidden, cover, chosen, previous];
    let space_id = spaces::window_space(hidden).ok_or("Sin Space para el fixture")?;
    let (space, frame) = spaces::visible_desktops()
        .into_iter()
        .filter(|key| key.1 == space_id)
        .find_map(|key| Some((key, area(key.0)?)))
        .ok_or("Sin área útil para el fixture")?;
    let mut manager = Manager::default();
    manager.active_areas.push((space, frame));
    let mut desktop = Desktop::configured(Layout::Spiral, None);
    desktop.reconcile_at(&ids, frame, 10., None);
    for (member, active) in [(hidden, cover), (chosen, previous)] {
        desktop = desktop
            .proposed_drop(active, DropIntent::Group(member), frame, 10.)
            .ok_or("No se pudieron crear los grupos mixtos")?;
    }
    let frames = desktop.window_frames(frame, 10.);
    for (id, target) in frames
        .iter()
        .copied()
        .chain(hidden_tab_frames(&desktop, &frames))
    {
        let pid = if id == hidden || id == chosen {
            shared_pid
        } else {
            other_pid
        };
        let element = ax::window_for_id(pid, id).ok_or("Ventana AX ausente")?;
        if !ax::set_frame(element.0, target) {
            return Err("No se pudo superponer el fixture".into());
        }
        let (position, size) = ax::geometry(element.0).ok_or("Sin geometría de la pestaña")?;
        if !near(NSRect::new(position, size), target) {
            return Err("La ventana desechable no alcanzó el marco de su grupo".into());
        }
        manager.windows.insert(
            id,
            Managed {
                pid,
                space,
                original: target,
                original_area: frame,
                element,
            },
        );
    }
    manager.desktops.insert(space, desktop);
    if !ax::raise(manager.windows[&cover].element.0) {
        return Err("No se pudo cubrir la pestaña inactiva".into());
    }
    writeln!(other.0.stdin.as_mut().unwrap(), "focus-last").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(other.0.stdout.as_mut().unwrap())
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    pump(0.15);
    if reply.trim() != "OK"
        || front_pid() == shared_pid
        || ax::focused_id(other_pid) != Some(previous)
        || ws::window_above(cover, hidden) != Some(true)
        || ws::window_above(previous, chosen) != Some(true)
    {
        return Err("El fixture no empezó con las dos pestañas compartidas detrás".into());
    }
    crate::ui::diagnostics::check_tab_focus_click(&mut manager, shared_pid, hidden, chosen)?;
    pump(0.3);
    if ws::window_above(chosen, previous) != Some(true) {
        return Err("La pestaña elegida no quedó encima en el segundo bloque".into());
    }
    if manager.desktops[&space].tab_role(chosen) != Some(true) {
        return Err("El modelo del segundo bloque no seleccionó la pestaña elegida".into());
    }
    if manager.desktops[&space].tab_role(cover) != Some(true)
        || ws::window_above(cover, hidden) != Some(true)
    {
        return Err(
            "La otra ventana de la aplicación tapó la pestaña activa del primer bloque".into(),
        );
    }
    println!("PASS: la pestaña elegida subió en su bloque y la otra del mismo PID siguió detrás.");
    Ok(())
}

fn check_shared_app_tabs(
    manager: &mut Manager,
    ids: &[u32],
    pid: i32,
    space: Workspace,
    area: NSRect,
) -> Result<(), String> {
    let mut desktop = Desktop::configured(Layout::Spiral, None);
    desktop.reconcile_at(ids, area, 10., None);
    for pair in ids.chunks_exact(2) {
        desktop = desktop
            .proposed_drop(pair[1], DropIntent::Group(pair[0]), area, 10.)
            .ok_or("No se pudieron preparar los dos grupos de la misma app")?;
    }
    manager.desktops.insert(space, desktop);
    let current_focus = || (front_pid(), ax::focused_id(pid));
    if ax::focused_id(pid) != Some(ids[3]) {
        return Err("El fixture no obtuvo el foco inicial en el segundo grupo".into());
    }
    crate::ui::diagnostics::check_tab_focus_click(manager, pid, ids[3], ids[0])?;
    for (chosen, other) in [(ids[0], ids[3]), (ids[2], ids[0]), (ids[1], ids[2])] {
        if chosen != ids[0] {
            if !manager.activate_tab(chosen) {
                return Err(format!("Pestaña rechazada: {chosen}"));
            }
            let deadline = Instant::now() + Duration::from_millis(150);
            while current_focus() != (pid, Some(chosen)) {
                if Instant::now() >= deadline {
                    return Err(format!("La pestaña {chosen} no recibió foco de teclado"));
                }
                pump(0.01);
            }
        }
        let desktop = &manager.desktops[&space];
        if desktop.tab_role(chosen) != Some(true) || desktop.tab_role(other) != Some(true) {
            return Err("La selección cambió la pestaña activa del otro grupo".into());
        }
    }
    println!("PASS: foco AX por ventana y selección independiente en dos grupos del mismo PID.");
    Ok(())
}

fn check_window_motion(manager: &mut Manager, ids: &[u32], area: NSRect) -> Result<(), String> {
    // Three different workloads in one batch: translate, resize, and both.
    let active = manager.active_areas.clone();
    let mut paths = Vec::new();
    for (index, &id) in ids.iter().enumerate() {
        let from = manager.windows[&id].original;
        let mut to = from;
        if index != 1 {
            to.origin.x = area.origin.x + 20.;
            to.origin.y = area.origin.y + 40.;
        }
        if index != 0 {
            to.size.width -= 80.;
            to.size.height -= 60.;
        }
        manager.transition_from(id, from, to);
        paths.push((id, from, to, false));
    }
    let started = Instant::now();
    let mut costs = Vec::new();
    while started.elapsed() < Duration::from_secs(3) {
        let tick = Instant::now();
        let mut visible = HashMap::new();
        for (id, from, to, intermediate) in &mut paths {
            let frame = ws::bounds(*id).ok_or("WindowServer perdió el fixture")?;
            *intermediate |= !near(frame, *from) && !near(frame, *to);
            visible.insert((manager.windows[id].pid, *id), frame);
        }
        let mut spaces = HashMap::new();
        manager.verify_placements(&active, &visible, false, &mut spaces);
        if !manager.transitions.is_empty() {
            manager.animate(&active, &visible, &mut spaces);
            costs.push(tick.elapsed());
        }
        if manager.transitions.is_empty() && manager.verifications.is_empty() {
            for (id, _, to, intermediate) in paths {
                let (p, s) = ax::geometry(manager.windows[&id].element.0).ok_or("Sin marco AX")?;
                if !intermediate
                    || !near(NSRect::new(p, s), to)
                    || !ws::bounds(id).is_some_and(|f| near(f, to))
                {
                    return Err(format!(
                        "Movimiento intermedio o destino sin confirmar: {id}"
                    ));
                }
            }
            costs.sort_unstable();
            println!(
                "Tres ventanas: {} pasos; trabajo mediano {:?}, máximo {:?}; destino AX y WindowServer confirmado en {:?}.",
                costs.len(),
                costs[costs.len() / 2],
                costs[costs.len() - 1],
                started.elapsed()
            );
            return Ok(());
        }
        pump((manager.tick_interval() - tick.elapsed().as_secs_f64()).max(0.));
    }
    Err("Las transiciones del fixture no se asentaron.".into())
}

pub fn check(mode: &str) -> Result<(), String> {
    let scenes_only = mode == "--check-work-scenes";
    let focus_only = mode == "--check-tab-focus";
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
    if mode == "--check-tab-stacking" {
        return check_mixed_app_tabs();
    }
    let two_displays = scenes_only && spaces::visible_desktops().len() >= 2;
    let mut fixture = Fixture(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--tab-close-fixture")
            .args(two_displays.then_some("--second-scene-display"))
            .args(focus_only.then_some("--shared-app-tabs"))
            .args(
                matches!(mode, "--check-floating-tabs" | "--check-floating-transfer")
                    .then_some("--float-overlap-fixture"),
            )
            .args(
                (mode == "--check-floating-transfer"
                    && std::env::args().any(|arg| arg == "--resizable-float"))
                .then_some("--resizable-float"),
            )
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
    let expected = if two_displays {
        5
    } else if focus_only {
        4
    } else {
        3
    };
    if ids.len() != expected {
        return Err("El fixture no creó todas las ventanas previstas".into());
    }
    let pid = fixture.0.id() as i32;
    let start = Instant::now();
    let first = loop {
        if let Some(first) = ax::window_for_id(pid, ids[0]) {
            break first;
        }
        if start.elapsed() >= Duration::from_secs(2) {
            return Err("AX no encontró el fixture".into());
        }
        pump(0.05);
    };
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
    for id in &ids[..if scenes_only { 3 } else { expected }] {
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
    if mode == "--check-window-motion" {
        return check_window_motion(&mut manager, &ids, area);
    }
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
    if mode == "--check-floating-transfer" {
        use crate::window_move::{Destination, Pending, Progress};
        let float = *ids.last().ok_or("Falta la flotante del fixture")?;
        let destination = spaces::desktop_ids_for_display(space.0)
            .into_iter()
            .enumerate()
            .find(|(_, other)| *other != space.1)
            .ok_or("Se necesitan dos escritorios en el monitor del fixture")?;
        let manual = std::env::args().any(|arg| arg == "--resizable-float");
        if ax::resizable(manager.windows[&float].element.0) != manual {
            return Err(
                "La capacidad AX de redimensionar no coincide con el caso de prueba".into(),
            );
        }
        if manual {
            manager.floating.insert(float);
        }
        send("focus-last", &mut input, &mut output)?;
        let original = manager.windows[&float].original;
        let suite = NSString::from_str(&format!("amspace.float-transfer.{}", std::process::id()));
        let prefs: Retained<AnyObject> = unsafe {
            let alloc: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
            let prefs: Retained<AnyObject> = msg_send![alloc, initWithSuiteName:&*suite];
            let yes: Obj = msg_send![class!(NSNumber), numberWithInt:1i32];
            let values: Obj = msg_send![class!(NSDictionary), dictionaryWithObject:yes, forKey:ns_string!("tile.enabled")];
            let _: () = msg_send![&prefs, registerDefaults:values];
            prefs
        };
        let defaults = Retained::as_ptr(&prefs) as Obj;
        let mut pending = Pending::start_expected(
            &mut manager,
            defaults,
            Destination::Space(destination.0),
            Some((pid, float)),
        )?;
        if !pending.plan.background_frames().is_empty() {
            return Err("El envío flotante preparó escrituras de mosaico".into());
        }
        loop {
            match pending.poll() {
                Progress::Waiting => pump(0.05),
                Progress::Failed(reason) => return Err(reason.into()),
                Progress::Arrived => break,
            }
        }
        if !pending.arrived() || spaces::window_space(float) != Some(destination.1) {
            return Err("WindowServer no confirmó el escritorio solicitado".into());
        }
        manager.commit_keyboard_transfer(pending.plan, pid, float, pending.element, original);
        if !manager.floating.contains(&float)
            || manager.windows[&float].space != (space.0, destination.1)
            || !ws::bounds(float).is_some_and(|actual| near(actual, original))
            || !spaces::visible_desktops().contains(&space)
        {
            return Err("Se alteró el marco, el estado flotante o el escritorio de origen".into());
        }
        println!(
            "PASS: flotante {float} enviada de {} a {} sin cambiar de escritorio, marco ni mosaico.",
            space.1, destination.1
        );
        return Ok(());
    }
    if mode == "--check-floating-tabs" {
        let [first, active, float] = ids[..] else {
            return Err("Faltan ventanas del fixture".into());
        };
        let mut desktop = Desktop::default();
        desktop.reconcile_at(&[first], area, 10., None);
        let desktop = desktop
            .proposed_drop(active, DropIntent::Group(first), area, 10.)
            .ok_or("Sin grupo de prueba")?;
        manager.desktops.insert(space, desktop);
        manager.floating.insert(float);
        // Put the header in the middle of the floating window.
        let front = manager.windows[&float].original;
        let mut slot = front;
        slot.origin.y += front.size.height / 2.;
        send("focus-last", &mut input, &mut output)?;
        let frames = [(active, slot)];
        manager.draw_strips(space, &frames, primary_height(), 0, false);
        for restore in [false, true] {
            if restore {
                manager.strips[0].hide();
                manager.restore_space_strips(&[space]);
            }
            pump(0.08);
            let strip = &manager.strips[0];
            let panel: isize = unsafe { msg_send![&*strip.panel, windowNumber] };
            let p = NSPoint::new(strip.frame.origin.x + 12., strip.frame.origin.y + 8.);
            if !contains(front, p)
                || ws::window_above(float, panel as u32) != Some(true)
                || ws::window_above(panel as u32, active) != Some(true)
                || manager.tab_strip_at(p)
                || ax::focused_id(pid) != Some(float)
            {
                return Err(format!(
                    "Orden o foco incorrecto al cubrir la tira: restauración={restore}, flotante={float}, tira={panel}, activa={active}"
                ));
            }
        }
        println!(
            "PASS: flotante delante de la tira, foco y hit-test correctos al crearla y restaurarla."
        );
        return Ok(());
    }
    if focus_only {
        send("focus-last", &mut input, &mut output)?;
        return check_shared_app_tabs(&mut manager, &ids, pid, space, area);
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
