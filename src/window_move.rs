//! Bounded, verified native transfers. No model commit precedes native arrival.
use crate::{
    accessibility as ax,
    native::*,
    spaces,
    tile_manager::{self, KeyboardPlacement, KeyboardTransfer, Manager, Workspace},
    window_server as ws,
};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
    sel,
};
use objc2_foundation::NSRect;
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};
/// Distinguishes a no-op reselection from a real failure so the caller can
/// still confirm the desktop with the soft frame.
pub const SAME_DESTINATION: &str = "La ventana ya está en ese destino.";
fn operation_class() -> Option<&'static AnyClass> {
    static LOAD: OnceLock<bool> = OnceLock::new();
    if !*LOAD.get_or_init(|| unsafe {
        !libc::dlopen(
            c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
            libc::RTLD_LAZY,
        )
        .is_null()
    }) {
        return None;
    }
    AnyClass::get(c"SLSBridgedMoveWindowsToManagedSpaceOperation")
}
pub fn space_moves_available() -> bool {
    operation_class().is_some_and(|class| unsafe {
        msg_send![class,instancesRespondToSelector:sel!(initWithWindows:spaceID:)]
    }) && crate::sky_bridge::asynchronous_operation().is_some()
}
fn request(id: u32, space: u64) -> bool {
    let Some(class) = operation_class().filter(|_| space_moves_available()) else {
        return false;
    };
    unsafe {
        let number: Obj = msg_send![class!(NSNumber),numberWithUnsignedInt:id];
        let windows: Obj = msg_send![class!(NSArray),arrayWithObject:number];
        let operation: Allocated<AnyObject> = msg_send![class, alloc];
        let operation: Option<Retained<AnyObject>> =
            msg_send![operation,initWithWindows:windows,spaceID:space];
        let Some(operation) = operation else {
            return false;
        };
        let Some(perform) = crate::sky_bridge::asynchronous_operation() else {
            return false;
        };
        let _ = perform(Retained::as_ptr(&operation) as Obj);
        if std::env::var_os("AMSPACE_TRACE_TRANSFERS").is_some() {
            eprintln!("bridge window={id} destination={space} submitted");
        }
        true
    }
}
#[derive(Clone, Copy)]
pub enum Destination {
    Space(usize),
    Monitor(bool),
}
#[derive(Default)]
struct Confirmation {
    stable: u8,
}
impl Confirmation {
    fn observe(&mut self, matching: bool) -> bool {
        if matching {
            self.stable = self.stable.saturating_add(1);
        } else {
            self.stable = 0;
        }
        self.stable >= 2
    }
}
fn geometry_retry_due(attempts: u8, elapsed: Option<Duration>) -> bool {
    attempts < 2
        && (elapsed.is_none()
            || (attempts == 1 && elapsed.is_some_and(|t| t >= Duration::from_millis(120))))
}
pub struct Pending {
    pub pid: i32,
    pub id: u32,
    pub element: ax::Owned,
    pub original: NSRect,
    pub plan: KeyboardTransfer,
    pub follow: bool,
    started: Instant,
    geometry_started: Option<Instant>,
    attempts: u8,
    confirmation: Confirmation,
    monitor: bool,
    background: Option<BackgroundPlacement>,
    background_windows: Vec<KeyboardPlacement>,
    configuration: Vec<ScreenGeometry>,
}
#[derive(Default)]
struct BackgroundPlacement {
    started: Option<Instant>,
    last_write: Option<Instant>,
    attempts: u8,
}
fn frame_matches(actual: NSRect, target: NSRect) -> bool {
    (actual.origin.x - target.origin.x).abs() <= 2.
        && (actual.origin.y - target.origin.y).abs() <= 2.
        && (actual.size.width - target.size.width).abs() <= 2.
        && (actual.size.height - target.size.height).abs() <= 2.
}
fn same_transfer_screens(
    expected: &[ScreenGeometry],
    current: &[ScreenGeometry],
    source: u32,
    destination: u32,
) -> bool {
    [source, destination].into_iter().all(|id| {
        expected
            .iter()
            .find(|screen| screen.id == id)
            .is_some_and(|before| current.iter().any(|after| after == before))
    })
}
impl BackgroundPlacement {
    /// No activation or Space selection: AX can resize normal hidden windows.
    /// Refusing apps fall back to ordinary reconciliation on entry.
    fn step(&mut self, destination: Workspace, frames: &[KeyboardPlacement]) -> bool {
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        if frames.is_empty()
            || now.duration_since(started) >= Duration::from_millis(600)
            || spaces::visible_desktops().contains(&destination)
        {
            return true;
        }
        let Some(identities) = ws::all_window_ids() else {
            return false;
        };
        let write = geometry_retry_due(
            self.attempts,
            self.last_write.map(|last| now.duration_since(last)),
        );
        let mut settled = true;
        for placement in frames {
            let KeyboardPlacement {
                pid,
                id,
                element,
                target,
            } = placement;
            if started.elapsed() >= Duration::from_millis(600) {
                return true;
            }
            // Hidden apps may omit windows from AXWindows. Retain references
            // before moving, but revalidate native owner, ID and Space each time.
            if !identities.contains(&(*pid, *id)) {
                continue;
            }
            if spaces::window_space(*id) != Some(destination.1) {
                continue;
            }
            if ax::window_id(element.0) != Some(*id)
                || !ax::tileable(element.0)
                || ax::flag(element.0, "AXMinimized")
                || ax::flag(element.0, "AXFullScreen")
            {
                continue;
            }
            let matches = ax::geometry(element.0)
                .is_some_and(|(p, s)| frame_matches(NSRect::new(p, s), *target));
            if !matches {
                settled = false;
                if write
                    && spaces::window_space(*id) == Some(destination.1)
                    && !spaces::visible_desktops().contains(&destination)
                {
                    ax::set_frame(element.0, *target);
                }
            }
        }
        if write {
            self.attempts += 1;
            self.last_write = Some(now);
        }
        settled
    }
}
#[derive(Debug, PartialEq)]
pub enum Progress {
    Waiting,
    Arrived,
    Failed(&'static str),
}
fn overlap(a: NSRect, b: NSRect) -> f64 {
    ((a.origin.x + a.size.width).min(b.origin.x + b.size.width) - a.origin.x.max(b.origin.x))
        .max(0.)
        * ((a.origin.y + a.size.height).min(b.origin.y + b.size.height)
            - a.origin.y.max(b.origin.y))
        .max(0.)
}
pub fn neighbor(source: u32, right: bool, screens: &[(u32, NSRect)]) -> Option<u32> {
    let from = screens.iter().find(|(id, _)| *id == source)?.1;
    let center = |r: NSRect| {
        (
            r.origin.x + r.size.width / 2.,
            r.origin.y + r.size.height / 2.,
        )
    };
    let (x, y) = center(from);
    screens
        .iter()
        .filter(|(id, r)| {
            *id != source
                && if right {
                    center(*r).0 > x + 1.
                } else {
                    center(*r).0 < x - 1.
                }
        })
        .min_by(|(aid, a), (bid, b)| {
            let metric = |r: NSRect| {
                let (cx, cy) = center(r);
                (cx - x).abs() + 0.25 * (cy - y).abs()
            };
            metric(*a).total_cmp(&metric(*b)).then_with(|| aid.cmp(bid))
        })
        .map(|(id, _)| *id)
}
impl Pending {
    pub fn start(
        manager: &mut Manager,
        defaults: Obj,
        destination: Destination,
    ) -> Result<Self, &'static str> {
        if !ax::trusted() {
            return Err(crate::i18n::tr(
                "Concede Accesibilidad para usar los atajos de ventanas.",
            ));
        }
        if crate::focus::secure_input() || crate::desktop_mode::active() {
            return Err(crate::i18n::tr(
                "El traslado está en pausa en este contexto.",
            ));
        }
        let (pid, id, element) = unsafe {
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            if buttons != 0 {
                return Err(crate::i18n::tr(
                    "Suelta el ratón antes de trasladar una ventana.",
                ));
            }
            let workspace: Obj = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app: Obj = msg_send![workspace, frontmostApplication];
            let pid: i32 = msg_send![app, processIdentifier];
            if pid == std::process::id() as i32 || ax::focus_is_protected(pid) {
                return Err(crate::i18n::tr(
                    "Selecciona una ventana normal de otra aplicación.",
                ));
            }
            let id = ax::focused_id(pid).ok_or(crate::i18n::tr("No hay una ventana activa."))?;
            let element = ax::window_for_id(pid, id)
                .ok_or(crate::i18n::tr("La ventana activa ya no está disponible."))?;
            if !ax::tileable(element.0)
                || ax::flag(element.0, "AXFullScreen")
                || ax::flag(element.0, "AXMinimized")
            {
                return Err(crate::i18n::tr("Esta ventana no admite traslados."));
            }
            (pid, id, element)
        };
        let (position, size) = ax::geometry(element.0).ok_or(crate::i18n::tr(
            "No se puede leer la geometría de la ventana activa.",
        ))?;
        let ax_frame = NSRect::new(position, size);
        let original = ws::bounds(id).ok_or(crate::i18n::tr(
            "WindowServer no puede confirmar la ventana activa.",
        ))?;
        if (ax_frame.origin.x - original.origin.x).abs() > 4.
            || (ax_frame.origin.y - original.origin.y).abs() > 4.
        {
            return Err(crate::i18n::tr(
                "Espera a que la ventana termine de moverse antes de trasladarla.",
            ));
        }
        let native = spaces::window_space(id).ok_or(crate::i18n::tr(
            "La ventana está fijada a varios escritorios o no tiene un destino inequívoco.",
        ))?;
        let configuration = screen_configuration();
        let screens: Vec<_> = configuration
            .iter()
            .filter_map(|s| tile_manager::area(s.id).map(|area| (s.id, area)))
            .collect();
        let source_display = screens
            .iter()
            .max_by(|(_, a), (_, b)| overlap(original, *a).total_cmp(&overlap(original, *b)))
            .filter(|(_, r)| overlap(original, *r) > 0.)
            .map(|(id, _)| *id)
            .ok_or(crate::i18n::tr("La pantalla de origen no está disponible."))?;
        let source: Workspace = (source_display, native);
        if !spaces::visible_desktops().contains(&source) {
            return Err(crate::i18n::tr(
                "La ubicación de la ventana está cambiando; espera un instante.",
            ));
        }
        let (destination, monitor) = match destination {
            Destination::Space(index) => {
                let target = *spaces::desktop_ids_for_display(source_display)
                    .get(index)
                    .ok_or(crate::i18n::tr(
                        "Ese escritorio no existe en la pantalla de la ventana activa.",
                    ))?;
                ((source_display, target), false)
            }
            Destination::Monitor(right) => {
                let target = neighbor(source_display, right, &screens)
                    .ok_or(crate::i18n::tr("No hay otra pantalla en esa dirección."))?;
                let space = spaces::visible_desktops()
                    .into_iter()
                    .find(|(id, _)| *id == target)
                    .ok_or(crate::i18n::tr(
                        "La pantalla de destino no muestra un escritorio normal.",
                    ))?
                    .1;
                ((target, space), true)
            }
        };
        if destination == source {
            return Err(crate::i18n::tr(SAME_DESTINATION));
        }
        if destination.1 != source.1 && !space_moves_available() {
            return Err(crate::i18n::tr(
                "Esta versión de macOS no ofrece el traslado seguro entre escritorios.",
            ));
        }
        let plan =
            manager.prepare_keyboard_transfer(defaults, pid, id, source, destination, original)?;
        let background_windows = manager.keyboard_transfer_windows(&plan);
        if destination.1 != source.1 && !request(id, destination.1) {
            return Err("macOS no ha aceptado la solicitud de traslado.");
        }
        Ok(Self {
            pid,
            id,
            element,
            original,
            plan,
            follow: !monitor && tile_manager::pref_or(defaults, "shortcuts.stay", 1) == 0,
            started: Instant::now(),
            geometry_started: None,
            attempts: 0,
            confirmation: Confirmation::default(),
            monitor,
            background: None,
            background_windows,
            configuration,
        })
    }
    fn configuration_matches(&self) -> bool {
        same_transfer_screens(
            &self.configuration,
            &screen_configuration(),
            self.plan.source.0,
            self.plan.destination.0,
        )
    }
    pub fn arrived(&self) -> bool {
        self.configuration_matches()
            && ws::all_window_ids().is_some_and(|ids| ids.contains(&(self.pid, self.id)))
            && ax::window_id(self.element.0) == Some(self.id)
            && spaces::window_space(self.id) == Some(self.plan.destination.1)
    }
    pub fn poll(&mut self) -> Progress {
        if !ax::trusted() {
            return Progress::Failed(crate::i18n::tr(
                "Se ha revocado Accesibilidad; el traslado queda cancelado.",
            ));
        }
        if !self.configuration_matches() {
            return Progress::Failed(crate::i18n::tr(
                "La configuración de pantallas cambió; se comprobará la ubicación real sin aplicar coordenadas antiguas.",
            ));
        }
        if self.started.elapsed() > Duration::from_secs(2) {
            return Progress::Failed(crate::i18n::tr(
                "macOS no confirmó el traslado a tiempo; se comprobará la ubicación real.",
            ));
        }
        if crate::focus::secure_input() || crate::desktop_mode::active() {
            self.confirmation.observe(false);
            return Progress::Waiting;
        }
        let Some(identities) = ws::all_window_ids() else {
            self.confirmation.observe(false);
            return Progress::Waiting;
        };
        if !identities.contains(&(self.pid, self.id)) {
            return Progress::Failed(crate::i18n::tr(
                "La ventana se cerró o cambió de identidad durante el traslado.",
            ));
        }
        let buttons: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
        if buttons != 0 {
            return Progress::Failed(crate::i18n::tr(
                "Traslado interrumpido por un gesto de ratón.",
            ));
        }
        if spaces::destination(self.plan.destination.1).is_none()
            || tile_manager::area(self.plan.destination.0).is_none()
        {
            return Progress::Failed(crate::i18n::tr("El destino ya no está disponible."));
        }
        if ax::window_id(self.element.0) != Some(self.id)
            || ax::flag(self.element.0, "AXFullScreen")
            || ax::flag(self.element.0, "AXMinimized")
        {
            return Progress::Failed(crate::i18n::tr(
                "La ventana ha cambiado durante el traslado.",
            ));
        }
        if !self.arrived() {
            self.confirmation.observe(false);
            return if self.started.elapsed() > Duration::from_secs(2) {
                Progress::Failed(crate::i18n::tr(
                    "macOS no confirmó el traslado; no se ha aplicado una distribución nueva.",
                ))
            } else {
                Progress::Waiting
            };
        }
        if self.monitor {
            let now = Instant::now();
            let retry_due = geometry_retry_due(
                self.attempts,
                self.geometry_started.map(|t| now.duration_since(t)),
            );
            if retry_due && self.attempts < 2 {
                ax::set_frame(self.element.0, self.plan.target);
                self.geometry_started = Some(now);
                self.attempts += 1;
                self.confirmation.observe(false);
                return Progress::Waiting;
            }
            let Some(frame) = ws::bounds(self.id) else {
                self.confirmation.observe(false);
                return Progress::Waiting;
            };
            let Some(area) = tile_manager::area(self.plan.destination.0) else {
                return Progress::Failed(crate::i18n::tr(
                    "La pantalla de destino se ha desconectado.",
                ));
            };
            if overlap(frame, area) < frame.size.width * frame.size.height * 0.5 {
                self.confirmation.observe(false);
                return if self.started.elapsed() > Duration::from_secs(2) {
                    Progress::Failed(crate::i18n::tr(
                        "La aplicación no confirmó el cambio de pantalla.",
                    ))
                } else {
                    Progress::Waiting
                };
            }
        }
        if !self.confirmation.observe(true) {
            return Progress::Waiting;
        }
        if !self
            .background
            .get_or_insert_with(BackgroundPlacement::default)
            .step(self.plan.destination, &self.background_windows)
        {
            return Progress::Waiting;
        }
        Progress::Arrived
    }
}
/// Read-only cold-cache planning against real native destination metadata.
pub fn check_transfer_preflight() -> Result<(), String> {
    if !ax::trusted() {
        return Err("Accessibility is required".into());
    }
    let visible = spaces::visible_desktops();
    let (pid, id, source, frame) = ws::tile_windows()
        .into_iter()
        .filter(|(pid, _, _)| *pid != std::process::id() as i32)
        .find_map(|(pid, id, frame)| {
            let space = spaces::window_space(id)?;
            let source = *visible.iter().find(|(_, native)| *native == space)?;
            let element = ax::window_for_id(pid, id)?;
            ax::tileable(element.0).then_some((pid, id, source, frame))
        })
        .ok_or("No normal visible window for read-only preflight")?;
    // Process-local defaults exercise tiling without changing user preferences.
    let defaults: Retained<AnyObject> = unsafe {
        let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![defaults,initWithSuiteName:&*objc2_foundation::NSString::from_str(&format!("amspace.preflight.{}",std::process::id()))];
        let values: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
        let enabled: Obj = msg_send![class!(NSNumber),numberWithInt:1i32];
        let _: () = msg_send![&values,setObject:enabled,forKey:&*objc2_foundation::NSString::from_str("tile.enabled")];
        let _: () = msg_send![&defaults,registerDefaults:&*values];
        defaults
    };
    let mut manager = Manager::default();
    let desktops = spaces::desktop_ids_for_display(source.0);
    for (index, space) in desktops.into_iter().enumerate() {
        if space == source.1 {
            continue;
        }
        let plan = manager
            .prepare_keyboard_transfer(
                Retained::as_ptr(&defaults) as Obj,
                pid,
                id,
                source,
                (source.0, space),
                frame,
            )
            .map_err(|e| format!("Desktop {}: {e}", index + 1))?;
        if !plan
            .background_frames()
            .iter()
            .any(|(window, _)| *window == id)
        {
            return Err("Diagnostic did not exercise a tiled arrival".into());
        }
        println!(
            "Desktop {}: cold-cache preflight accepted, {} tiled frames; no move requested",
            index + 1,
            plan.background_frames().len()
        );
    }
    Ok(())
}
/// A disposable foreign AppKit window, never a user's document.
pub fn run_transfer_fixture() {
    use std::io::Write;
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(15));
        unsafe {
            libc::_exit(0);
        }
    });
    unsafe {
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![app,setActivationPolicy:1isize];
        let _: () = msg_send![app, finishLaunching];
        let window: Allocated<AnyObject> = msg_send![class!(NSWindow), alloc];
        let window: Retained<AnyObject> = msg_send![window,initWithContentRect:rect(200.,200.,320.,180.),styleMask:15usize,backing:2usize,defer:false];
        let _: () = msg_send![&window,setReleasedWhenClosed:false];
        let _: () = msg_send![&window,setCollectionBehavior:0usize];
        let _: () = msg_send![&window,setIgnoresMouseEvents:true];
        let _: () = msg_send![&window,setTitle:&*objc2_foundation::NSString::from_str("AMSpace · prueba temporal")];
        let _: () = msg_send![&window, orderFrontRegardless];
        let id: isize = msg_send![&window, windowNumber];
        println!("{id}");
        let _ = std::io::stdout().flush();
        let _: () = msg_send![app, run];
    }
}
pub fn check_native_bridge() -> Result<(), String> {
    use std::io::{BufRead, BufReader};
    if !space_moves_available() {
        return Err("Native Space transfer is unavailable".into());
    }
    let snapshot = spaces::snapshot();
    let source = snapshot
        .active
        .and_then(|i| snapshot.desktops.get(i))
        .copied()
        .ok_or("No active normal desktop")?;
    let destinations: Vec<_> = snapshot
        .desktops
        .iter()
        .copied()
        .filter(|id| *id != source)
        .collect();
    if destinations.is_empty() {
        return Err("A second desktop is required".into());
    }
    struct Fixture(std::process::Child);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut fixture = Fixture(
        std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--space-transfer-fixture")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let pid = fixture.0.id() as i32;
    let stdout = fixture
        .0
        .stdout
        .take()
        .ok_or("Fixture stdout is unavailable")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let line = BufReader::new(stdout).lines().next().and_then(Result::ok);
        let _ = sender.send(line);
    });
    let id: u32 = receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|e| e.to_string())?
        .ok_or("No test window ID")?
        .parse()
        .map_err(|_| "Invalid test window ID")?;
    let wait = |space: u64| unsafe {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if spaces::window_space(id) == Some(space) {
                return true;
            }
            let until: Obj = msg_send![class!(NSDate),dateWithTimeIntervalSinceNow:0.025f64];
            let run: Obj = msg_send![class!(NSRunLoop), currentRunLoop];
            let _: () = msg_send![run,runUntilDate:until];
        }
        eprintln!(
            "Test window {id}: requested={space}, actual={:?}, onscreen={:?}",
            spaces::window_space(id),
            ws::visible_normal_window_ids().map(|ids| ids.contains(&(pid, id)))
        );
        false
    };
    if !wait(source) {
        return Err("Test window did not appear at origin".into());
    }
    let element = ax::window_for_id(pid, id).ok_or("No AX test window before transfer")?;
    for target in destinations
        .into_iter()
        .flat_map(|destination| [destination, source])
    {
        if !request(id, target) || !wait(target) {
            return Err(format!(
                "Native arrival was not confirmed for desktop {target}"
            ));
        }
        println!("Verified disposable foreign window {id} on desktop {target}");
        if target != source {
            let destination = target;
            let display = spaces::destination(destination)
                .ok_or("Destination disappeared")?
                .display;
            let area = tile_manager::area(display).ok_or("No destination display area")?;
            let frame = rect(
                area.origin.x + 30.,
                area.origin.y + 30.,
                area.size.width * 0.6,
                area.size.height * 0.6,
            );
            let mut placement = BackgroundPlacement::default();
            let frames = [KeyboardPlacement {
                pid,
                id,
                element: element.clone(),
                target: frame,
            }];
            while !placement.step((display, destination), &frames) {
                unsafe {
                    let until: Obj =
                        msg_send![class!(NSDate),dateWithTimeIntervalSinceNow:0.025f64];
                    let run: Obj = msg_send![class!(NSRunLoop), currentRunLoop];
                    let _: () = msg_send![run,runUntilDate:until];
                }
            }
            if !ax::geometry(element.0)
                .is_some_and(|(p, s)| frame_matches(NSRect::new(p, s), frame))
            {
                eprintln!(
                    "Hidden fixture: id={:?}, tileable={}, actual={:?}, target={frame:?}, visible={:?}, attempts={}",
                    ax::window_id(element.0),
                    ax::tileable(element.0),
                    ax::geometry(element.0),
                    spaces::visible_desktops(),
                    placement.attempts
                );
                return Err("Hidden window did not accept background geometry".into());
            }
            println!("Verified hidden window geometry without selecting its desktop");
        }
    }
    let after = spaces::snapshot();
    if after.active.and_then(|i| after.desktops.get(i)).copied() != Some(source) {
        return Err("Active desktop changed unexpectedly".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::NSSize;
    #[test]
    fn topology_permutations_cancel_only_when_a_participant_changes() {
        let permutations = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut checked = 0;
        for portrait in [false, true] {
            for scale in [1., 1.5, 2.] {
                let screens = [
                    ScreenGeometry {
                        id: 1,
                        frame: rect(0., 0., 2560., 1440.),
                        visible: rect(10., 41., 2540., 1327.),
                        scale: 2.,
                    },
                    ScreenGeometry {
                        id: 2,
                        frame: rect(
                            -1080.,
                            -300.,
                            if portrait { 1080. } else { 1920. },
                            if portrait { 1920. } else { 1080. },
                        ),
                        visible: rect(
                            -1070.,
                            -259.,
                            if portrait { 1060. } else { 1900. },
                            if portrait { 1870. } else { 1030. },
                        ),
                        scale,
                    },
                    ScreenGeometry {
                        id: 3,
                        frame: rect(0., -1080., 1920., 1080.),
                        visible: rect(0., -1055., 1920., 1055.),
                        scale: 1.,
                    },
                ];
                for source in 1..=3 {
                    for destination in 1..=3 {
                        for permutation in permutations {
                            let current = permutation.map(|i| screens[i]);
                            assert!(same_transfer_screens(
                                &screens,
                                &current,
                                source,
                                destination
                            ));
                            for changed in 0..3 {
                                for field in 0..5 {
                                    let mut altered = current;
                                    match field {
                                        0 => altered[changed].scale += 0.25,
                                        1 => altered[changed].frame.origin.x += 10.,
                                        2 => {
                                            altered[changed].frame.size = NSSize::new(
                                                altered[changed].frame.size.height,
                                                altered[changed].frame.size.width,
                                            )
                                        }
                                        3 => altered[changed].visible.size.height -= 25.,
                                        _ => altered[changed].id += 100,
                                    }
                                    let participant = current[changed].id == source
                                        || current[changed].id == destination;
                                    assert_eq!(
                                        same_transfer_screens(
                                            &screens,
                                            &altered,
                                            source,
                                            destination
                                        ),
                                        !participant
                                    );
                                    checked += 1;
                                }
                                let retained: Vec<_> = current
                                    .iter()
                                    .copied()
                                    .enumerate()
                                    .filter(|(i, _)| *i != changed)
                                    .map(|(_, screen)| screen)
                                    .collect();
                                let participant = current[changed].id == source
                                    || current[changed].id == destination;
                                assert_eq!(
                                    same_transfer_screens(&screens, &retained, source, destination),
                                    !participant
                                );
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        eprintln!("TOPOLOGY_MATRIX changes_and_disconnects={checked}");
    }
    #[test]
    fn transfer_topology_checks_both_participants_not_enumeration_order() {
        let source = ScreenGeometry {
            id: 1,
            frame: rect(0., 0., 1680., 1050.),
            visible: rect(0., 25., 1680., 1025.),
            scale: 2.,
        };
        let destination = ScreenGeometry {
            id: 2,
            frame: rect(-1080., -400., 1080., 1920.),
            visible: rect(-1080., -375., 1080., 1895.),
            scale: 1.,
        };
        let other = ScreenGeometry {
            id: 3,
            frame: rect(1680., 0., 1920., 1080.),
            visible: rect(1680., 25., 1920., 1055.),
            scale: 1.,
        };
        let before = [source, destination, other];
        assert!(same_transfer_screens(
            &before,
            &[other, destination, source],
            1,
            2
        ));
        assert!(same_transfer_screens(&before, &[source, destination], 1, 2));
        assert!(!same_transfer_screens(&before, &[source, other], 1, 2));
        assert!(!same_transfer_screens(&before, &[destination, other], 1, 2));
        for changed in [
            ScreenGeometry {
                frame: rect(-1920., -400., 1920., 1080.),
                ..destination
            },
            ScreenGeometry {
                scale: 2.,
                ..destination
            },
            ScreenGeometry {
                visible: rect(-1080., -350., 1080., 1870.),
                ..destination
            },
            ScreenGeometry {
                frame: rect(0., -400., 1080., 1920.),
                ..destination
            },
        ] {
            assert!(!same_transfer_screens(
                &before,
                &[source, changed, other],
                1,
                2
            ));
        }
        assert!(same_transfer_screens(&before, &before, 1, 1));
        assert!(!same_transfer_screens(&before, &[destination, other], 1, 1));
    }
    #[test]
    fn equally_near_monitors_have_stable_selection_after_enumeration_changes() {
        let screens = [
            (1, rect(0., 0., 1600., 900.)),
            (3, rect(1600., -900., 900., 1600.)),
            (2, rect(1600., 200., 900., 1600.)),
        ];
        let mut reordered = screens;
        reordered.reverse();
        assert_eq!(neighbor(1, true, &screens), neighbor(1, true, &reordered));
        let tied = [
            (1, rect(0., 0., 1600., 900.)),
            (3, rect(1600., 0., 1200., 900.)),
            (2, rect(1600., 0., 1200., 900.)),
        ];
        assert_eq!(neighbor(1, true, &tied), Some(2));
    }
    #[test]
    fn arrival_requires_two_consecutive_confirmations_and_resets_after_uncertainty() {
        let mut c = Confirmation::default();
        assert!(!c.observe(true));
        assert!(!c.observe(false));
        assert!(!c.observe(true));
        assert!(c.observe(true));
        assert!(!c.observe(false));
        assert!(!c.observe(true));
        assert!(c.observe(true));
    }
    #[test]
    fn geometry_retries_are_delayed_and_strictly_bounded() {
        assert!(geometry_retry_due(0, None));
        assert!(!geometry_retry_due(1, Some(Duration::from_millis(119))));
        assert!(geometry_retry_due(1, Some(Duration::from_millis(120))));
        assert!(!geometry_retry_due(2, None));
        assert!(!geometry_retry_due(2, Some(Duration::from_secs(5))));
    }
    #[test]
    fn vertical_only_and_disconnected_monitors_are_not_left_or_right_targets() {
        let screens = [
            (1, rect(0., 0., 1600., 900.)),
            (2, rect(0., 900., 1600., 900.)),
        ];
        assert_eq!(neighbor(1, false, &screens), None);
        assert_eq!(neighbor(1, true, &screens), None);
        assert_eq!(neighbor(8, true, &screens), None);
        assert_eq!(neighbor(1, true, &[]), None);
    }
    #[test]
    fn directional_monitor_selection_uses_geometry_and_does_not_wrap() {
        let screens = [
            (1, rect(0., 0., 1600., 900.)),
            (2, rect(-900., -200., 900., 1600.)),
            (3, rect(1600., 200., 1200., 800.)),
            (4, rect(1600., 1800., 1200., 800.)),
        ];
        assert_eq!(neighbor(1, false, &screens), Some(2));
        assert_eq!(neighbor(1, true, &screens), Some(3));
        assert_eq!(neighbor(2, false, &screens), None);
        assert_eq!(neighbor(3, false, &screens), Some(1));
    }
}
