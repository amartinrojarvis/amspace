//! Explicit main-thread diagnostics using only AMSpace panels and isolated defaults.
use super::*;

fn pump(seconds: f64) {
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < end {
        objc2::rc::autoreleasepool(|_| unsafe {
            let app: Obj = msg_send![class!(NSApplication), sharedApplication];
            let date: Obj = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow:0.01f64];
            let event: Option<Retained<AnyObject>> = msg_send![app,
                nextEventMatchingMask:usize::MAX, untilDate:date,
                inMode:objc2_foundation::NSDefaultRunLoopMode, dequeue:true];
            if let Some(event) = event {
                let _: () = msg_send![app, sendEvent:&*event];
            }
            let _: () = msg_send![app, updateWindows];
        });
    }
}

// Exercise the production AppKit layout at several usable screen sizes without
// changing display modes, user defaults, or windows belonging to other apps.
unsafe fn check_settings_layout(
    target: &Retained<AnyObject>,
    defaults: &Retained<AnyObject>,
) -> Result<(), String> {
    unsafe {
        for (name, width, height) in [
            ("roomy", 1600., 1200.),
            ("compact", 1280., 740.),
            ("portrait", 800., 1400.),
            ("short", 1280., 640.),
        ] {
            let mut app = App::build(target.clone(), defaults.clone());
            app.border_enabled = false;
            app.sleeping = true;
            app.build_settings_for_screen(rect(0., 0., width, height));
            let panel = app.settings.as_ref().unwrap().clone();
            let _: () = msg_send![&panel, setLevel:3isize];
            let _: () = msg_send![&panel, orderFrontRegardless];
            let result = (|| -> Result<(), String> {
                for (page, section) in SETTINGS_SECTIONS.iter().take(5).enumerate() {
                    let _: () = msg_send![app.tabs, selectTabViewItemAtIndex:page];
                    text(app.page_title, crate::i18n::tr(section.0));
                    pump(0.08);
                    let item: Obj = msg_send![app.tabs, tabViewItemAtIndex:page];
                    let view: Obj = msg_send![item, view];
                    let viewport: NSRect = msg_send![view, bounds];
                    let children: Obj = msg_send![view, subviews];
                    let inner: Obj = msg_send![children, firstObject];
                    let frame: NSRect = msg_send![inner, frame];
                    let bounds: NSRect = msg_send![inner, bounds];
                    if (frame.size.width - bounds.size.width).abs() > 0.5
                        || (frame.size.height - bounds.size.height).abs() > 0.5
                        || (frame.size.width - viewport.size.width).abs() > 0.5
                        || (page != 0 && (frame.size.height - viewport.size.height).abs() > 0.5)
                    {
                        return Err(format!(
                            "{name}, page {page}: scaled or unused page viewport."
                        ));
                    }
                    let scroll: Obj = if page == 1 {
                        app.space_scroll
                    } else if page == 0 {
                        ptr::null_mut()
                    } else {
                        let control = match page {
                            2 => app.tile_controls[0],
                            3 => app.shortcut_fields[0].0,
                            _ => app.login,
                        };
                        msg_send![control, enclosingScrollView]
                    };
                    if !scroll.is_null() {
                        let clip: Obj = msg_send![scroll, contentView];
                        let visible: NSRect = msg_send![clip, bounds];
                        let document: Obj = msg_send![scroll, documentView];
                        let extent: NSRect = msg_send![document, frame];
                        let content_height = match page {
                            1 => app.space_ids.len() as f64 * 52.,
                            2 => settings_tile_height(extent.size.width),
                            3 => SETTINGS_SHORTCUTS_HEIGHT,
                            _ => SETTINGS_GENERAL_HEIGHT,
                        };
                        let reserved = if page == 1 { 282. } else { 0. };
                        let available = bounds.size.height - reserved;
                        let needs_scroll = content_height > available + 0.5;
                        let scrolls = extent.size.height > visible.size.height + 0.5;
                        if (visible.size.height - available).abs() > 0.5
                            || scrolls != needs_scroll
                            || (extent.size.height - content_height.max(available)).abs() > 0.5
                        {
                            return Err(format!(
                                "{name}, page {page}: avoidable scrolling or unreachable content \
                                 (content={content_height}, available={available}, \
                                 viewport={}, document={}).",
                                visible.size.height, extent.size.height,
                            ));
                        }
                        // Check both ends through the actual native clip view.
                        for y in [0., (extent.size.height - visible.size.height).max(0.)] {
                            let _: () = msg_send![clip, scrollToPoint:NSPoint::new(0., y)];
                            let actual: NSRect = msg_send![clip, bounds];
                            if (actual.origin.y - y).abs() > 0.5 {
                                return Err(format!(
                                    "{name}, page {page}: unreachable scroll endpoint."
                                ));
                            }
                        }
                        let controls: Obj = msg_send![document, subviews];
                        let count: usize = msg_send![controls, count];
                        for i in 0..count {
                            let control: Obj = msg_send![controls, objectAtIndex:i];
                            let frame: NSRect = msg_send![control, frame];
                            if frame.origin.x < -0.5
                                || frame.origin.y < -0.5
                                || frame.origin.x + frame.size.width > extent.size.width + 0.5
                                || frame.origin.y + frame.size.height > extent.size.height + 0.5
                            {
                                return Err(format!(
                                    "{name}, page {page}: clipped settings control."
                                ));
                            }
                            let field: bool = msg_send![control, isKindOfClass:class!(NSTextField)];
                            if page == 3 && field {
                                let cell: Obj = msg_send![control, cell];
                                let fitted: NSSize = msg_send![cell, cellSizeForBounds:rect(0., 0., frame.size.width, frame.size.height)];
                                if fitted.width > frame.size.width + 0.5
                                    || fitted.height > frame.size.height + 0.5
                                {
                                    let title: *mut NSString = msg_send![control, stringValue];
                                    return Err(format!(
                                        "{name}, page {page}: clipped shortcut text: {}.",
                                        title
                                            .as_ref()
                                            .map(|value| value.to_string())
                                            .unwrap_or_default(),
                                    ));
                                }
                            }
                        }
                    }
                    capture_window(
                        &panel,
                        &format!(
                            ".cache/locale-{}-settings-{name}-{page}.png",
                            crate::i18n::language().code()
                        ),
                    )?;
                }
                Ok(())
            })();
            let _: () = msg_send![&panel, orderOut:ptr::null::<AnyObject>()];
            let bar: Obj = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem:&*app.status];
            result?;
        }
        println!(
            "Settings layout: all five pages at roomy, compact, portrait and short usable areas; native point sizes, necessary scrolling only, both endpoints reachable."
        );
        Ok(())
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
            check_settings_layout(&target, &defaults)?;
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
                if page == 2 {
                    let content: Obj = msg_send![&panel, contentView];
                    for (tag, layout) in [
                        (77isize, crate::tiling::Layout::CenteredWide),
                        (78isize, crate::tiling::Layout::ThreeBands),
                    ] {
                        let choice: Obj = msg_send![content, viewWithTag:tag];
                        if choice.is_null() {
                            return Err("Missing orientation default selector.".into());
                        }
                        let _: () = msg_send![choice, selectItemAtIndex:layout.selection_index()];
                        let delivered: bool =
                            msg_send![choice, sendAction:sel!(change:), to:&*target];
                        if !delivered
                            || crate::tile_manager::default_layout(
                                Retained::as_ptr(&defaults) as Obj,
                                tag == 78,
                            ) != layout
                            || APP.with(|cell| cell.borrow().as_ref().unwrap().tile_timer.is_some())
                        {
                            return Err("Orientation defaults were not saved independently or started tiling.".into());
                        }
                    }
                    pump(0.08);
                    capture_window(
                        &panel,
                        &format!(".cache/locale-{code}-settings-defaults.png"),
                    )?;
                    let choice: Obj = msg_send![content, viewWithTag:77isize];
                    let document: Obj = msg_send![choice, superview];
                    let _: () = msg_send![document, scrollPoint:NSPoint::new(0., 0.)];
                    pump(0.08);
                    capture_window(
                        &panel,
                        &format!(".cache/locale-{code}-settings-tiling-bottom.png"),
                    )?;
                }
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
                mouse_delivery::enqueue(target_ptr, true, NSPoint::new(100., 42.), None);
                mouse_delivery::enqueue(target_ptr, false, NSPoint::new(800., 42.), None);
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
            check_permission_recovery(target_ptr, &defaults)?;
            check_power_reentry(target_ptr, &defaults)?;
            APP.with(|cell| {
                let mut borrowed = cell.borrow_mut();
                borrowed.as_mut().unwrap().sleeping = false;
                crate::desktop_mode::update(crate::desktop_mode::Mode::Desktop);
                pump(0.025);
            });
            pump(0.035);
            if APP.with(|cell| cell.borrow().as_ref().unwrap().presentation_timer.is_none()) {
                return Err(
                    "La entrada a Mostrar escritorio se perdió durante reentrada AX.".into(),
                );
            }
            APP.with(|cell| {
                let mut borrowed = cell.borrow_mut();
                // Never resume a real tiler/hover monitor in this isolated UI.
                borrowed.as_mut().unwrap().sleeping = true;
                crate::desktop_mode::update(crate::desktop_mode::Mode::Normal);
                pump(0.025);
            });
            pump(0.035);
            if APP.with(|cell| cell.borrow().as_ref().unwrap().presentation_timer.is_some()) {
                return Err("La salida de Mostrar escritorio dejó un reloj activo.".into());
            }
            APP.with(|cell| {
                let _borrowed = cell.borrow_mut();
                crate::desktop_mode::update(crate::desktop_mode::Mode::Desktop);
                pump(0.025);
                crate::desktop_mode::update(crate::desktop_mode::Mode::Normal);
            });
            pump(0.035);
            if APP.with(|cell| cell.borrow().as_ref().unwrap().presentation_timer.is_some()) {
                return Err(
                    "Una transición de presentación obsoleta se entregó tras la salida.".into(),
                );
            }
            println!(
                "Presentation delivery: entry/exit survive AX reentry; stale transitions do not restart the presentation clock."
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
        crate::desktop_mode::update(crate::desktop_mode::Mode::Normal);
        mouse_delivery::clear(target_ptr);
        if let Some(mut app) = APP.with(|cell| cell.borrow_mut().take()) {
            app.stop_permission_watch();
            if let Some(timer) = app.tile_timer.take() {
                let _: () = msg_send![&timer, invalidate];
            }
            app.tile_mouse = None;
            app.hide_flash();
            if let Some(timer) = app.presentation_timer.take() {
                let _: () = msg_send![&timer, invalidate];
            }
            let bar: Obj = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem:&*app.status];
        }
        let _: () = msg_send![&*defaults, removePersistentDomainForName:&*suite];
        result
    }
}

fn check_permission_recovery(target: Obj, defaults: &AnyObject) -> Result<(), String> {
    if !ax::trusted() {
        println!(
            "SKIP permission recovery: this diagnostic process has no Accessibility grant; TCC was not changed."
        );
        return Ok(());
    }
    unsafe {
        let _: () = msg_send![defaults, setBool:true, forKey:ns_string!("tile.enabled")];
        APP.with(|cell| -> Result<(), String> {
            let mut borrowed = cell.borrow_mut();
            let app = borrowed.as_mut().unwrap();
            app.sleeping = false;
            app.hover_enabled = false;
            app.permission_trusted = false;
            app.permission_deadline = Some(Instant::now() + Duration::from_secs(300));
            if !app.refresh_permission()
                || !app.permission_trusted
                || app.permission_deadline.is_some()
                || app.tile_timer.is_none()
                || app.tile_mouse.is_none()
            {
                return Err(
                    "Conceder AX no arrancó el motor/monitor sin actividad de ventanas.".into(),
                );
            }
            // Never run global reconciliation: cancel both the first-cycle
            // selector and clock before yielding this borrowed main-thread turn.
            app.tiling_paused = true;
            app.configure_tiling();
            if app.tile_timer.is_some() || app.tile_mouse.is_some() {
                return Err("Pausar tras conceder AX dejó el motor activo.".into());
            }
            app.permission_trusted = false;
            app.permission_deadline = Some(Instant::now() + Duration::from_secs(300));
            permission_changed(target, sel!(permissionChanged:), ptr::null_mut());
            pump(0.025);
            if app.permission_trusted {
                return Err("El callback de permisos atravesó un préstamo de APP.".into());
            }
            Ok(())
        })?;
        pump(0.035);
        APP.with(|cell| -> Result<(), String> {
            let mut borrowed = cell.borrow_mut();
            let app = borrowed.as_mut().unwrap();
            if !app.permission_trusted
                || app.permission_deadline.is_some()
                || app.tile_timer.is_some()
                || app.tile_mouse.is_some()
            {
                return Err("La entrega AX reentrante se perdió o ignoró la pausa.".into());
            }
            let _: () = msg_send![defaults, setBool:false, forKey:ns_string!("tile.enabled")];
            app.tiling_paused = false;
            app.permission_trusted = false;
            app.refresh_permission();
            if app.tile_timer.is_some() || app.tile_mouse.is_some() {
                return Err("Conceder AX activó Mosaico con su preferencia apagada.".into());
            }
            app.permission_deadline = Some(Instant::now() - Duration::from_secs(1));
            Ok(())
        })?;
        permission_tick(target, sel!(permissionTick:), ptr::null_mut());
        if APP.with(|cell| {
            cell.borrow()
                .as_ref()
                .unwrap()
                .permission_deadline
                .is_some()
        }) {
            return Err("El seguimiento temporal de permisos sobrevivió a su plazo.".into());
        }
        APP.with(|cell| {
            cell.borrow_mut().as_mut().unwrap().permission_deadline =
                Some(Instant::now() + Duration::from_secs(300));
        });
        sleep(target, sel!(sleep:), ptr::null_mut());
        if APP.with(|cell| {
            cell.borrow()
                .as_ref()
                .unwrap()
                .permission_deadline
                .is_some()
        }) {
            return Err("Dormir dejó pendiente el seguimiento de permisos.".into());
        }
    }
    println!(
        "Permission recovery: real AX authority with stale unauthorized cache starts clock/monitor without window activity; deferred delivery, pause/off, expiry and sleep checked. No TCC reset and no global placement tick executed."
    );
    Ok(())
}

fn check_power_reentry(target: Obj, defaults: &AnyObject) -> Result<(), String> {
    unsafe {
        let _: () = msg_send![defaults, setBool:false, forKey:ns_string!("tile.enabled")];
    }
    let switch = if ax::trusted() {
        match spaces::visible_desktops().first() {
            Some((_, space)) => Some(space_switch::Switch::start(*space).map_err(str::to_owned)?),
            None => None,
        }
    } else {
        None
    };
    let had_switch = switch.is_some();
    APP.with(|cell| -> Result<(), String> {
        let mut borrowed = cell.borrow_mut();
        let app = borrowed.as_mut().unwrap();
        app.sleeping = false;
        app.hover_enabled = false;
        app.switching = switch;
        sleep(target, sel!(sleep:), ptr::null_mut());
        pump(0.025);
        if app.sleeping {
            return Err("Dormir atravesó el préstamo de APP durante reentrada AX.".into());
        }
        Ok(())
    })?;
    pump(0.035);
    APP.with(|cell| -> Result<(), String> {
        let mut borrowed = cell.borrow_mut();
        let app = borrowed.as_mut().unwrap();
        if !app.sleeping
            || app.switching.is_some()
            || app.pending_focus.is_some()
            || app.tile_timer.is_some()
            || app.hover_timer.is_some()
        {
            return Err("Dormir se perdió o dejó acciones/relojes anteriores activos.".into());
        }
        wake(target, sel!(wake:), ptr::null_mut());
        pump(0.025);
        if !app.sleeping {
            return Err("Despertar atravesó el préstamo de APP durante reentrada AX.".into());
        }
        Ok(())
    })?;
    pump(0.035);
    APP.with(|cell| -> Result<(), String> {
        let borrowed = cell.borrow();
        let app = borrowed.as_ref().unwrap();
        if app.sleeping
            || app.switching.is_some()
            || app.pending_focus.is_some()
            || app.tile_timer.is_some()
            || app.hover_timer.is_some()
        {
            return Err(
                "Despertar se perdió, reanudó una acción vieja o activó funciones deshabilitadas."
                    .into(),
            );
        }
        Ok(())
    })?;
    println!(
        "Power callbacks: local sleep/wake survive AX reentry; old navigation cancelled={had_switch}; disabled tiling/hover stay off. No physical sleep, Space switch or global input."
    );
    if !had_switch {
        println!(
            "SKIP in-flight Space cancellation: no Accessibility grant or normal native desktop available."
        );
    }
    Ok(())
}

/// Exercise the production × callback against a disposable foreign window.
pub(crate) fn check_reentrant_close(
    manager: &mut crate::tile_manager::Manager,
    id: u32,
) -> Result<(), String> {
    if APP.with(|cell| cell.borrow().is_some()) {
        return Err("El diagnóstico de cierre requiere una instancia UI aislada.".into());
    }
    unsafe {
        let suite = NSString::from_str(&format!("amspace.ui.close.{}", std::process::id()));
        let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![defaults, initWithSuiteName:&*suite];
        let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
        let _: () = msg_send![&defaults, setBool:true, forKey:ns_string!("tile.enabled")];
        let target = controller();
        let target_ptr = Retained::as_ptr(&target) as Obj;
        let mut app = App::build(target.clone(), defaults.clone());
        app.border_enabled = false;
        app.tile_manager = std::mem::take(manager);
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        let panel = window(rect(100., 200., 160., 50.), 128);
        let _: () = msg_send![&*panel, setLevel:3isize];
        let root: Obj = msg_send![&*panel, contentView];
        let close = button(
            root,
            rect(60., 8., 40., 32.),
            "×",
            target_ptr,
            -(id as isize) - 2,
        );
        let _: () = msg_send![close, setAction:sel!(closeTileTab:)];
        let _: () = msg_send![&*panel, orderFrontRegardless];
        pump(0.05);
        let result = (|| -> Result<(), String> {
            // A destructive action must expire while AX keeps the UI busy.
            APP.with(|cell| {
                let _borrowed = cell.borrow_mut();
                let _: bool = msg_send![close, sendAction:sel!(closeTileTab:), to:target_ptr];
                pump(0.38);
            });
            if tab_delivery::pending_count() != 0 || ws::ordered_in(id) != Some(true) {
                return Err("Un cierre caducado se entregó o quedó pendiente.".into());
            }
            // Pausing after the click, before delivery, must keep the window open.
            APP.with(|cell| {
                let mut borrowed = cell.borrow_mut();
                let _: bool = msg_send![close, sendAction:sel!(closeTileTab:), to:target_ptr];
                borrowed.as_mut().unwrap().tiling_paused = true;
            });
            pump(0.035);
            if tab_delivery::pending_count() != 0 || ws::ordered_in(id) != Some(true) {
                return Err("Un cierre encolado ignoró la pausa de Mosaico.".into());
            }
            APP.with(|cell| -> Result<(), String> {
                let mut borrowed = cell.borrow_mut();
                borrowed.as_mut().unwrap().tiling_paused = false;
                let _: bool = msg_send![close, sendAction:sel!(closeTileTab:), to:target_ptr];
                pump(0.025);
                if tab_delivery::pending_count() != 1 || ws::ordered_in(id) != Some(true) {
                    return Err(
                        "El clic de cierre se perdió o ejecutó durante reentrada AX.".into(),
                    );
                }
                Ok(())
            })?;
            pump(0.05);
            if tab_delivery::pending_count() != 0 {
                return Err("El clic de cierre no se entregó al liberar la UI.".into());
            }
            Ok(())
        })();
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:&*target];
        tab_delivery::clear(target_ptr);
        let _: () = msg_send![&*panel, orderOut:ptr::null::<AnyObject>()];
        if let Some(mut app) = APP.with(|cell| cell.borrow_mut().take()) {
            *manager = std::mem::take(&mut app.tile_manager);
            let bar: Obj = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem:&*app.status];
        }
        let _: () = msg_send![&*defaults, removePersistentDomainForName:&*suite];
        result
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TabPanDelivery {
    Immediate,
    Reentrant,
    Burst,
    Expired,
    ExpiredEnd,
    Paused,
    Cancelled,
    Modifiers { hover: bool, release: bool },
}

/// Drive the real pan recognizer and production controller, including nested AX
/// delivery. The caller owns every foreign window in this isolated manager.
pub(crate) fn check_tab_pan(
    manager: &mut crate::tile_manager::Manager,
    button: &AnyObject,
    destination: NSPoint,
    release_at: NSPoint,
    delivery: TabPanDelivery,
    floating_enabled: bool,
    inspect_preview: impl FnOnce(&crate::tile_manager::Manager) -> Result<(), String>,
) -> Result<(), String> {
    if APP.with(|cell| cell.borrow().is_some()) {
        return Err("El diagnóstico de arrastre requiere una instancia UI aislada.".into());
    }
    unsafe {
        let primary = primary_height().ok_or("Sin pantalla principal para el arrastre")?;
        let suite = NSString::from_str(&format!("amspace.ui.pan.{}", std::process::id()));
        let defaults: Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Retained<AnyObject> = msg_send![defaults, initWithSuiteName:&*suite];
        let _: () = msg_send![&defaults, removePersistentDomainForName:&*suite];
        let _: () = msg_send![&defaults, setBool:true, forKey:ns_string!("tile.enabled")];
        let _: () = msg_send![&defaults, setBool:floating_enabled, forKey:ns_string!("tile.shake")];
        let target = controller();
        let target_ptr = Retained::as_ptr(&target) as Obj;
        let mut app = App::build(target.clone(), defaults.clone());
        app.border_enabled = false;
        app.tile_manager = std::mem::take(manager);
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        let gestures: Obj = msg_send![button, gestureRecognizers];
        let gesture: Obj = msg_send![gestures, objectAtIndex:0usize];
        let previous_target: Option<Retained<AnyObject>> = msg_send![gesture, target];
        let previous_button_target: Option<Retained<AnyObject>> = msg_send![button, target];
        let _: () = msg_send![gesture, setTarget:target_ptr];
        let _: () = msg_send![button, setTarget:target_ptr];
        let panel: Obj = msg_send![button, window];
        let number: isize = msg_send![panel, windowNumber];
        let bounds: NSRect = msg_send![button, bounds];
        let start: NSPoint = msg_send![button, convertPoint:NSPoint::new(bounds.size.width / 2., bounds.size.height / 2.), toView:ptr::null::<AnyObject>()];
        let end: NSPoint = msg_send![panel, convertPointFromScreen:NSPoint::new(destination.x, primary - destination.y)];
        let release: NSPoint = msg_send![panel, convertPointFromScreen:NSPoint::new(release_at.x, primary - release_at.y)];
        let native_app: Obj = msg_send![class!(NSApplication), sharedApplication];
        let native_event = |kind: usize,
                            point: NSPoint,
                            independent: bool|
         -> Result<Retained<AnyObject>, String> {
            let process: Obj = msg_send![class!(NSProcessInfo), processInfo];
            let uptime: f64 = msg_send![process, systemUptime];
            let event: Option<Retained<AnyObject>> = msg_send![class!(NSEvent),
                mouseEventWithType:kind, location:point, modifierFlags:if independent { 1usize << 19 } else { 0 },
                timestamp:uptime, windowNumber:number, context:ptr::null::<AnyObject>(),
                eventNumber:0isize, clickCount:1isize, pressure:if kind == 2 { 0f32 } else { 1f32 }];
            event.ok_or_else(|| "No se pudo crear el evento local de arrastre".into())
        };
        let queue = |kind: usize, point: NSPoint, independent: bool| -> Result<(), String> {
            let event = native_event(kind, point, independent)?;
            let _: () = msg_send![native_app, postEvent:&*event, atStart:false];
            Ok(())
        };
        let result = (|| -> Result<(), String> {
            let reentrant = matches!(
                delivery,
                TabPanDelivery::Reentrant
                    | TabPanDelivery::Burst
                    | TabPanDelivery::Modifiers { .. }
            );
            let (hover, release_option) = match delivery {
                TabPanDelivery::Modifiers { hover, release } => (hover, release),
                _ => (false, false),
            };
            queue(1, start, false)?;
            queue(6, NSPoint::new(start.x + 10., start.y - 8.), hover)?;
            queue(6, end, hover)?;
            if matches!(delivery, TabPanDelivery::Immediate) {
                pump(0.035);
            } else {
                APP.with(|cell| -> Result<(), String> {
                    let mut borrowed = cell.borrow_mut();
                    pump(if matches!(delivery, TabPanDelivery::Expired) {
                        0.38
                    } else {
                        0.035
                    });
                    if matches!(delivery, TabPanDelivery::Burst) {
                        // Stress reentrant delivery, not wall-clock throughput
                        // of Cocoa's synthetic-event queue. The recognizer is
                        // already moving from the real mouse sequence above.
                        let state: isize = msg_send![gesture, state];
                        if state != 2 {
                            return Err("La ráfaga requiere un pan nativo en movimiento.".into());
                        }
                        for _ in 0..256 {
                            let _: () = msg_send![target_ptr, dragTileTab:gesture];
                        }
                    }
                    if reentrant && tab_delivery::pending_count() < 2 {
                        return Err(
                            "El inicio/movimiento del pan se perdió durante reentrada AX.".into(),
                        );
                    }
                    if matches!(delivery, TabPanDelivery::Paused) {
                        borrowed.as_mut().unwrap().tiling_paused = true;
                    }
                    if matches!(delivery, TabPanDelivery::Cancelled) {
                        let _: () = msg_send![gesture, setEnabled:false];
                    }
                    Ok(())
                })?;
                pump(0.035);
            }
            APP.with(|cell| inspect_preview(&cell.borrow().as_ref().unwrap().tile_manager))?;
            queue(2, release, release_option)?;
            if reentrant || matches!(delivery, TabPanDelivery::ExpiredEnd) {
                APP.with(|cell| -> Result<(), String> {
                    let _borrowed = cell.borrow_mut();
                    pump(if matches!(delivery, TabPanDelivery::ExpiredEnd) {
                        0.38
                    } else {
                        0.025
                    });
                    if tab_delivery::pending_count() == 0 {
                        return Err("El fin del pan se perdió durante reentrada AX.".into());
                    }
                    if matches!(delivery, TabPanDelivery::Modifiers { .. }) {
                        // Advance the native recognizer to a subsequent press,
                        // without dispatching a second button click. Ended is
                        // still queued behind the borrowed App.
                        let subsequent = native_event(1, start, !release_option)?;
                        let _: () = msg_send![gesture, mouseDown:&*subsequent];
                        let live: usize = msg_send![gesture, dragModifierFlags];
                        if (live & (1 << 19) != 0) == release_option {
                            return Err(
                                "La prueba no modificó el reconocedor después de Ended.".into()
                            );
                        }
                    }
                    Ok(())
                })?;
            }
            pump(0.045);
            if tab_delivery::pending_count() != 0 {
                return Err("El arrastre dejó acciones pendientes al liberar la UI.".into());
            }
            Ok(())
        })();
        // Also release AppKit tracking on an assertion/error, never inject input
        // globally or leave a recognizer holding the next test's mouse sequence.
        let _: () = msg_send![gesture, setEnabled:false];
        let _ = queue(2, release, false);
        pump(0.02);
        let _: () = msg_send![gesture, setEnabled:true];
        let _: () = msg_send![gesture, setTarget:previous_target.as_deref().map_or(ptr::null_mut(), |target| target as *const AnyObject as Obj)];
        let _: () = msg_send![button, setTarget:previous_button_target.as_deref().map_or(ptr::null_mut(), |target| target as *const AnyObject as Obj)];
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:&*target];
        tab_delivery::clear(target_ptr);
        if let Some(mut app) = APP.with(|cell| cell.borrow_mut().take()) {
            *manager = std::mem::take(&mut app.tile_manager);
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
