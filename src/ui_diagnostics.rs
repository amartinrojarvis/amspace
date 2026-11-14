//! Explicit main-thread diagnostics using only AMSpace panels and isolated defaults.
use super::*;

fn pump(seconds: f64) {
    unsafe {
        let run: Obj = msg_send![class!(NSRunLoop), currentRunLoop];
        let until: Obj = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow:seconds];
        let _: () = msg_send![run, runUntilDate:until];
    }
}

pub(crate) fn check_deferred_callbacks() -> Result<(), String> {
    if APP.with(|cell| cell.borrow().is_some()) {
        return Err("El diagnóstico de entregas requiere una instancia de prueba aislada.".into());
    }
    unsafe {
        let suite = NSString::from_str(&format!("amspace.ui.delivery.{}", std::process::id()));
        let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![defaults, initWithSuiteName:&*suite];
        let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
        let target = controller();
        let target_ptr = Retained::as_ptr(&target) as Obj;
        let mut app = App::build(target.clone(), defaults.clone());
        app.border_enabled = false;
        app.flash_enabled = true;
        app.sleeping = true;
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        let result = (|| -> Result<(), String> {
            let started = Instant::now();
            for _ in 0..16 {
                APP.with(|cell| {
                    let mut borrowed = cell.borrow_mut();
                    let app = borrowed.as_mut().unwrap();
                    app.flash_desktop(None);
                    app.settle_queued = true;
                    app.focus_target = Some((0, 1, Instant::now() - Duration::from_millis(500)));
                    let visible = app
                        .flash_panel
                        .as_ref()
                        .is_some_and(|panel| msg_send![&**panel, isVisible]);
                    assert!(visible, "A fresh confirmation must become visible again");
                    for _ in 0..250 {
                        settle(target_ptr, sel!(settle:), ptr::null_mut());
                        hide_flash(target_ptr, sel!(hideFlash:), ptr::null_mut());
                        ws::queue_accessibility_event(true);
                    }
                    // A nested AppKit loop fires the real deferred selectors
                    // while App is still borrowed, just as a synchronous AX call can.
                    pump(0.025);
                });
                pump(0.035);
                let settled = APP.with(|cell| {
                    let borrowed = cell.borrow();
                    let app = borrowed.as_ref().unwrap();
                    let hidden = app.flash_panel.as_ref().is_some_and(|panel| {
                        let visible: bool = msg_send![&**panel, isVisible];
                        !visible
                    });
                    !app.settle_queued && app.focus_target.is_none() && hidden
                });
                if !settled {
                    return Err(
                        "Una entrega AX reentrante dejó el borde o su confirmación pendientes."
                            .into(),
                    );
                }
            }
            println!(
                "UI delivery: 12000 callbacks in 16 reentrant bursts, focus refreshed and flash hidden; {:.3} s",
                started.elapsed().as_secs_f64()
            );

            let (height, _) = settings_tile_geometry(560.);
            let panel = window(rect(100., 200., 560., height), 128);
            let background: Obj = msg_send![class!(NSColor), windowBackgroundColor];
            let _: () = msg_send![&*panel, setBackgroundColor:background];
            let view: Obj = msg_send![&*panel, contentView];
            with_app(|app| app.build_tile_page(view, target_ptr));
            let toggle: Obj = msg_send![view, viewWithTag:68isize];
            if toggle.is_null() {
                return Err("No aparece el ajuste del gesto de agitación.".into());
            }
            let initial: isize = msg_send![toggle, state];
            if initial != 1 {
                return Err("El gesto no comienza activado en preferencias nuevas.".into());
            }
            for state in [0isize, 1] {
                let _: () = msg_send![toggle, setState:state];
                let sent: bool = msg_send![toggle, sendAction:sel!(change:), to:target_ptr];
                if !sent
                    || crate::tile_manager::pref(Retained::as_ptr(&defaults) as Obj, "tile.shake")
                        != state
                {
                    return Err("El ajuste de agitación no guarda el estado elegido.".into());
                }
                if APP.with(|cell| cell.borrow().as_ref().unwrap().tile_timer.is_some()) {
                    return Err(
                        "Cambiar un ajuste arrancó el temporizador con Mosaico desactivado.".into(),
                    );
                }
            }
            let _: () = msg_send![&*panel, orderFrontRegardless];
            pump(0.06);
            let capture = capture_window(&panel, ".cache/shake-settings.png");
            let _: () = msg_send![&*panel, orderOut:ptr::null::<AnyObject>()];
            capture?;
            println!(
                "Shake setting: default enabled, disable/enable persisted in isolated suite; native Mosaic page rendered"
            );
            check_full_picker(Retained::as_ptr(&defaults) as Obj)?;
            Ok(())
        })();
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:&*target];
        if let Some(mut app) = APP.with(|cell| cell.borrow_mut().take()) {
            app.hide_flash();
            let bar: Obj = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem:&*app.status];
        }
        let _: () = msg_send![&*defaults, removePersistentDomainForName:&*suite];
        result
    }
}

fn capture_window(panel: &AnyObject, path: &str) -> Result<(), String> {
    let number: isize = unsafe { msg_send![panel, windowNumber] };
    std::fs::create_dir_all(".cache").map_err(|error| error.to_string())?;
    // Native controls can be compositor-backed and absent from cacheDisplay.
    // Capture only this owned window, never the user's desktop.
    let output = std::process::Command::new("/usr/sbin/screencapture")
        .args(["-x", "-o", "-l", &number.to_string(), path])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "No se pudo capturar el panel propio: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

unsafe fn capture_view(view: Obj, path: &NSString) -> Result<(), String> {
    unsafe {
        let _: () = msg_send![view, displayIfNeeded];
        let bounds: NSRect = msg_send![view, bounds];
        let bitmap: Option<Retained<AnyObject>> =
            msg_send![view, bitmapImageRepForCachingDisplayInRect:bounds];
        let bitmap = bitmap.ok_or("No se pudo capturar la vista nativa.")?;
        let _: () = msg_send![view, cacheDisplayInRect:bounds, toBitmapImageRep:&*bitmap];
        let properties: Obj = msg_send![class!(NSDictionary), dictionary];
        let png: Option<Retained<AnyObject>> =
            msg_send![&bitmap, representationUsingType:4usize, properties:properties];
        let png = png.ok_or("No se pudo codificar la captura.")?;
        std::fs::create_dir_all(".cache").map_err(|error| error.to_string())?;
        let saved: bool = msg_send![&png, writeToFile:path, atomically:true];
        if saved {
            Ok(())
        } else {
            Err("No se pudo guardar la captura.".into())
        }
    }
}

unsafe fn check_full_picker(defaults: Obj) -> Result<(), String> {
    use crate::tiling::{Desktop, DropIntent, Layout};
    unsafe {
        let area = rect(0., 31., 1080., 1889.);
        let mut desktop = Desktop::configured(Layout::Spiral, None);
        desktop.reconcile_at(&[1, 2], area, 10., None);
        let distribution = desktop
            .save_distribution(area)
            .ok_or("Plantilla inválida.")?;
        for index in 0..64 {
            crate::custom_templates::save(
                defaults,
                &format!("Plantilla {index:02}"),
                distribution.clone(),
            )?;
        }
        let desktop = desktop
            .proposed_drop(3, DropIntent::Group(1), area, 10.)
            .ok_or("No se pudo preparar el grupo del selector.")?;
        let context = crate::tile_manager::PickerContext {
            workspace: (0, 0),
            area,
            desktop,
        };
        let mut picker = crate::tile_picker::Picker::new(context, ptr::null_mut(), defaults)
            .ok_or("No se pudo abrir la biblioteca completa.")?;
        picker.select(71);
        if !matches!(
            picker.selection(),
            Some(crate::tile_picker::Selection::Custom(_))
        ) {
            return Err("La última plantilla guardada no se pudo seleccionar.".into());
        }
        picker.show();
        pump(0.06);
        let app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let panel: Obj = msg_send![app, keyWindow];
        if panel.is_null() {
            return Err("La galería no recibió su ventana nativa.".into());
        }
        let view: Obj = msg_send![panel, contentView];
        let children: Obj = msg_send![view, subviews];
        let count: usize = msg_send![children, count];
        let scroll = (0..count)
            .find_map(|index| {
                let child: Obj = msg_send![children, objectAtIndex:index];
                let is_scroll: bool = msg_send![child, isKindOfClass:class!(NSScrollView)];
                is_scroll.then_some(child)
            })
            .ok_or("La biblioteca no permite desplazarse.")?;
        let document: Obj = msg_send![scroll, documentView];
        let selected: Obj = msg_send![document, viewWithTag:71isize];
        if selected.is_null() {
            return Err("Falta la última tarjeta de la biblioteca.".into());
        }
        let bounds: NSRect = msg_send![selected, bounds];
        let frame: NSRect = msg_send![document, convertRect:bounds, fromView:selected];
        let visible: NSRect = msg_send![document, visibleRect];
        if bounds.size.height < 100.
            || frame.origin.x < visible.origin.x - 1.
            || frame.origin.y < visible.origin.y - 1.
            || frame.origin.x + frame.size.width > visible.origin.x + visible.size.width + 1.
            || frame.origin.y + frame.size.height > visible.origin.y + visible.size.height + 1.
        {
            return Err("La última tarjeta se encoge o queda fuera del área desplazable.".into());
        }
        capture_view(view, ns_string!(".cache/picker-full-library.png"))?;
        println!(
            "Picker: 64 saved templates + 8 built-ins, portrait viewport, last card fully visible at {:.0} × {:.0} pt",
            bounds.size.width, bounds.size.height
        );
        Ok(())
    }
}
