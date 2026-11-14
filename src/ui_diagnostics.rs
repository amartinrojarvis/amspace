//! Explicit main-thread diagnostics using only AMSpace panels and isolated defaults.
use super::*;

fn pump(seconds: f64) {
    unsafe {
        let run: Obj = msg_send![class!(NSRunLoop), currentRunLoop];
        let until: Obj = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow:seconds];
        let _: () = msg_send![run, runUntilDate:until];
    }
}

/// Explicit localization smoke proof; never registers hotkeys or controls foreign windows.
pub(crate) fn check_localizations() -> Result<(), String> {
    if APP.with(|cell| cell.borrow().is_some()) {
        return Err("Localization diagnostics require an isolated process.".into());
    }
    unsafe {
        let application: Obj = msg_send![class!(NSApplication), sharedApplication];
        let _: bool = msg_send![application, setActivationPolicy:1isize];
        let _: () = msg_send![application, finishLaunching];
        let suite = NSString::from_str(&format!("amspace.ui.languages.{}", std::process::id()));
        let allocated: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![allocated, initWithSuiteName:&*suite];
        let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
        let target = controller();
        let mut app = App::build(target.clone(), defaults.clone());
        app.border_enabled = false;
        app.sleeping = true;
        app.build_settings();
        // A non-normal layer keeps the installed tiler from treating this
        // diagnostic window as a user's resizable document.
        let panel = app.settings.as_ref().unwrap().clone();
        let _: () = msg_send![&panel, setLevel:3isize];
        app.guide = Some(crate::discovery::Guide::new(
            Retained::as_ptr(&target) as Obj,
            false,
        ));
        app.feedback_panel = Some(crate::discovery::Feedback::new(
            Retained::as_ptr(&target) as Obj
        ));
        let _: () = msg_send![app.guide.as_ref().unwrap().window(), setLevel:3isize];
        let _: () = msg_send![app.feedback_panel.as_ref().unwrap().window(), setLevel:3isize];
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        let language = crate::i18n::language();
        let code = language.code();
        let result = (|| -> Result<(), String> {
            let bundle: Obj = msg_send![class!(NSBundle), mainBundle];
            let localizations: Obj = msg_send![bundle, preferredLocalizations];
            let count: usize = msg_send![localizations, count];
            let first: *mut NSString = if count == 0 {
                ptr::null_mut()
            } else {
                msg_send![localizations, objectAtIndex:0usize]
            };
            let localized = first
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_default();
            if localized != code {
                return Err(format!(
                    "Native bundle selected {localized:?}, expected {code:?}."
                ));
            }
            let base: *mut NSString = msg_send![bundle, privateFrameworksPath];
            if let Some(base) = base.as_ref() {
                let path = NSString::from_str(&format!("{}/Sparkle.framework", base));
                let framework: Obj = msg_send![class!(NSBundle), bundleWithPath:&*path];
                if !framework.is_null() {
                    let preferred: Obj = msg_send![framework, preferredLocalizations];
                    let first: *mut NSString = msg_send![preferred, firstObject];
                    let native = first
                        .as_ref()
                        .map(|value| value.to_string())
                        .unwrap_or_default();
                    if native != code {
                        return Err(format!("Sparkle selected {native:?}, expected {code:?}."));
                    }
                }
            }
            let _: () = msg_send![&panel, orderFrontRegardless];
            for page in 0..5isize {
                with_app(|app| {
                    let _: () = msg_send![app.tabs, selectTabViewItemAtIndex:page];
                    text(
                        app.page_title,
                        crate::i18n::tr(SETTINGS_SECTIONS[page as usize].0),
                    );
                });
                pump(0.08);
                capture_window(&panel, &format!(".cache/locale-{code}-settings-{page}.png"))?;
            }
            let content: Obj = msg_send![&panel, contentView];
            let selector: Obj = msg_send![content, viewWithTag:120isize];
            if selector.is_null() {
                return Err("The language selector is missing from General.".into());
            }
            let initial: isize = msg_send![selector, indexOfSelectedItem];
            if initial != crate::i18n::preference(Retained::as_ptr(&defaults) as Obj) as isize {
                return Err("The language selector does not reflect the saved preference.".into());
            }
            let next = if language == crate::i18n::Language::Spanish {
                2isize
            } else {
                1
            };
            let _: () = msg_send![selector, selectItemAtIndex:next];
            let delivered: bool = msg_send![selector, sendAction:sel!(change:), to:&*target];
            let saved: Obj = msg_send![&defaults, persistentDomainForName:&*suite];
            let stored: *mut NSString = msg_send![saved, objectForKey:ns_string!("app.language")];
            let expected = if next == 1 { "es" } else { "en" };
            if !delivered
                || stored
                    .as_ref()
                    .is_none_or(|value| value.to_string() != expected)
                || crate::i18n::language() != language
                || APP.with(|cell| cell.borrow().as_ref().unwrap().tile_timer.is_some())
            {
                return Err("Language selection failed to persist, changed the running language or started tiling.".into());
            }
            let document: Obj = msg_send![selector, superview];
            let _: () = msg_send![document, scrollPoint:NSPoint::new(0., 0.)];
            pump(0.08);
            capture_window(
                &panel,
                &format!(".cache/locale-{code}-settings-general-bottom.png"),
            )?;
            for page in 0..3usize {
                with_app(|app| app.show_guide_page(page));
                pump(0.08);
                let window = APP.with(|cell| {
                    cell.borrow()
                        .as_ref()
                        .unwrap()
                        .guide
                        .as_ref()
                        .unwrap()
                        .window()
                        .clone()
                });
                let visible: bool = msg_send![&window, isVisible];
                if !visible {
                    return Err("The owned help window is not visible.".into());
                }
                capture_window(&window, &format!(".cache/locale-{code}-help-{page}.png"))?;
            }
            with_app(App::show_feedback);
            pump(0.08);
            let window = APP.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .unwrap()
                    .feedback_panel
                    .as_ref()
                    .unwrap()
                    .window()
                    .clone()
            });
            let visible: bool = msg_send![&window, isVisible];
            if !visible {
                return Err("The owned feedback window is not visible.".into());
            }
            capture_window(&window, &format!(".cache/locale-{code}-feedback.png"))?;
            with_app(App::show_about);
            pump(0.08);
            let window = APP.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .unwrap()
                    .about
                    .as_ref()
                    .unwrap()
                    .clone()
            });
            let _: () = msg_send![&window, setLevel:3isize];
            let visible: bool = msg_send![&window, isVisible];
            if !visible {
                return Err("The owned about window is not visible.".into());
            }
            capture_window(&window, &format!(".cache/locale-{code}-about.png"))?;
            println!(
                "Localization {code}: five settings pages, three help pages, feedback and about captured; native bundle/Sparkle locale and deferred language preference verified."
            );
            Ok(())
        })();
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:&*target];
        if let Some(mut app) = APP.with(|cell| cell.borrow_mut().take()) {
            app.hide_flash();
            if let Some(guide) = app.guide.as_ref() {
                guide.hide();
            }
            let windows: Obj = msg_send![application, windows];
            let count: usize = msg_send![windows, count];
            for index in 0..count {
                let window: Obj = msg_send![windows, objectAtIndex:index];
                let _: () = msg_send![window, orderOut:ptr::null::<AnyObject>()];
            }
            let bar: Obj = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem:&*app.status];
        }
        let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
        result
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
            APP.with(|cell| {
                let _borrowed = cell.borrow_mut();
                mouse_delivery::enqueue(target_ptr, true, NSPoint::new(100., 42.));
                mouse_delivery::enqueue(target_ptr, false, NSPoint::new(800., 42.));
                pump(0.025);
                if mouse_delivery::pending_count() != 2 {
                    return Err("Native mouse boundaries were lost during AX reentry.".to_owned());
                }
                Ok(())
            })?;
            pump(0.035);
            if mouse_delivery::pending_count() != 0 {
                return Err("Deferred mouse boundaries did not drain after AX reentry.".into());
            }
            println!(
                "Mouse delivery: native press/release retained through nested AppKit delivery and drained in order."
            );
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
                return Err("No aparece el ajuste de la zona superior.".into());
            }
            let initial: isize = msg_send![toggle, state];
            if initial != 1 {
                return Err("La zona superior no comienza activada en preferencias nuevas.".into());
            }
            for state in [0isize, 1] {
                let _: () = msg_send![toggle, setState:state];
                let sent: bool = msg_send![toggle, sendAction:sel!(change:), to:target_ptr];
                if !sent
                    || crate::tile_manager::pref(Retained::as_ptr(&defaults) as Obj, "tile.shake")
                        != state
                {
                    return Err("El ajuste de la zona superior no guarda el estado elegido.".into());
                }
                if APP.with(|cell| cell.borrow().as_ref().unwrap().tile_timer.is_some()) {
                    return Err(
                        "Cambiar un ajuste arrancó el temporizador con Mosaico desactivado.".into(),
                    );
                }
            }
            let _: () = msg_send![&*panel, orderFrontRegardless];
            pump(0.06);
            let capture = capture_window(&panel, ".cache/float-drop-settings.png");
            let _: () = msg_send![&*panel, orderOut:ptr::null::<AnyObject>()];
            capture?;
            println!(
                "Float drop setting: default enabled, disable/enable persisted in isolated suite; native Mosaic page rendered"
            );
            check_full_picker(Retained::as_ptr(&defaults) as Obj)?;
            Ok(())
        })();
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:&*target];
        mouse_delivery::clear(target_ptr);
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
