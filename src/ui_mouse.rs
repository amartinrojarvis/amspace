//! Main-thread mouse boundaries survive nested AX/AppKit delivery, without motion polling.
use super::*;

const CAPACITY: usize = 8;
const MAX_AGE: Duration = Duration::from_millis(350);

#[derive(Clone, Copy)]
struct Boundary {
    down: bool,
    point: NSPoint,
    queued: Instant,
}

#[derive(Default)]
struct Boundaries {
    entries: [Option<Boundary>; CAPACITY],
    head: usize,
    len: usize,
    cancel: bool,
}
impl Boundaries {
    fn push(&mut self, event: Boundary) {
        if self.len == CAPACITY {
            // Never keep a release whose matching press was lost to overflow.
            *self = Self {
                cancel: true,
                ..Self::default()
            };
            if !event.down {
                return;
            }
        }
        self.entries[(self.head + self.len) % CAPACITY] = Some(event);
        self.len += 1;
    }
    fn pop(&mut self, now: Instant) -> Option<Boundary> {
        if self.len == 0 {
            return None;
        }
        let event = self.entries[self.head].take().unwrap();
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        if now.saturating_duration_since(event.queued) >= MAX_AGE {
            *self = Self {
                cancel: true,
                ..Self::default()
            };
            None
        } else {
            Some(event)
        }
    }
}
thread_local! {
    static PENDING: RefCell<Boundaries> = RefCell::new(Boundaries::default());
}

pub(super) fn clear(target: Obj) {
    PENDING.with(|pending| *pending.borrow_mut() = Boundaries::default());
    unsafe {
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:target, selector:sel!(tileMouseEvent:), object:ptr::null::<AnyObject>()];
    }
}

pub(super) fn enqueue(target: Obj, down: bool, point: NSPoint) {
    PENDING.with(|pending| {
        pending.borrow_mut().push(Boundary {
            down,
            point,
            queued: Instant::now(),
        })
    });
    autoreleasepool(|_| drain(target, sel!(tileMouseEvent:), ptr::null_mut()));
}

pub(super) extern "C-unwind" fn drain(target: Obj, _: Sel, _: Obj) {
    loop {
        let mut found_app = false;
        let delivered = try_with_app(|app| {
            found_app = true;
            if app.layout_picker.is_some()
                || app.tiling_paused
                || app.tab_menu.is_some()
                || app.transfer.is_some()
                || app.switching.is_some()
                || app.sleeping
                || crate::tile_manager::pref(Retained::as_ptr(&app.defaults) as Obj, "tile.enabled")
                    == 0
            {
                clear(target);
                app.tile_manager.cancel_drag();
                return;
            }
            let (event, cancel) = PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                let event = pending.pop(Instant::now());
                (event, std::mem::take(&mut pending.cancel))
            });
            if cancel {
                app.tile_manager.cancel_drag();
            }
            if let Some(event) = event {
                app.tile_manager.mouse_event(
                    Retained::as_ptr(&app.defaults) as Obj,
                    event.down,
                    event.point,
                );
                if let Some(error) = app.tile_manager.interaction_error.take() {
                    app.show_notice(&error);
                }
                app.update_tile_timer();
            }
        });
        if !delivered {
            // One replaceable, bounded retry; no timer or work while idle.
            schedule_app_callback(target, sel!(tileMouseEvent:), 0.01);
            return;
        }
        if !found_app
            || PENDING.with(|pending| {
                let pending = pending.borrow();
                pending.len == 0 && !pending.cancel
            })
        {
            clear(target);
            return;
        }
    }
}

pub(super) fn pending_count() -> usize {
    PENDING.with(|pending| pending.borrow().len)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(down: bool, x: f64, queued: Instant) -> Boundary {
        Boundary {
            down,
            point: NSPoint::new(x, 42.),
            queued,
        }
    }
    #[test]
    fn native_press_and_release_keep_order_and_the_actual_release_point() {
        let now = Instant::now();
        let mut queue = Boundaries::default();
        queue.push(event(true, 100., now));
        queue.push(event(false, 800., now + Duration::from_millis(20)));
        let press = queue.pop(now + Duration::from_millis(30)).unwrap();
        let release = queue.pop(now + Duration::from_millis(30)).unwrap();
        assert!(press.down);
        assert!(!release.down);
        assert_eq!(release.point, NSPoint::new(800., 42.));
        assert!(queue.pop(now).is_none());
        assert!(!queue.cancel);
    }
    #[test]
    fn overflow_cancels_instead_of_releasing_an_unrelated_gesture() {
        let now = Instant::now();
        let mut queue = Boundaries::default();
        for index in 0..CAPACITY {
            queue.push(event(index % 2 == 0, index as f64, now));
        }
        queue.push(event(false, 999., now));
        assert!(queue.cancel);
        assert!(queue.pop(now).is_none());
        queue.push(event(true, 120., now));
        queue.push(event(false, 140., now));
        assert!(queue.pop(now).unwrap().down);
        assert_eq!(queue.pop(now).unwrap().point.x, 140.);
    }
    #[test]
    fn expired_press_cannot_be_followed_by_a_fresh_but_unmatched_release() {
        let now = Instant::now();
        let mut queue = Boundaries::default();
        queue.push(event(true, 100., now));
        queue.push(event(false, 800., now + MAX_AGE));
        assert!(queue.pop(now + MAX_AGE).is_none());
        assert!(queue.cancel);
        assert!(queue.pop(now + MAX_AGE).is_none());
    }
}
