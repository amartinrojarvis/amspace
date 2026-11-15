//! Tab actions retain their original control across nested AX/AppKit delivery.
use super::*;

const CAPACITY: usize = 8;
const MAX_AGE: Duration = Duration::from_millis(350);

#[derive(Clone, Copy)]
pub(super) enum Action {
    Select,
    Close,
    Drag { state: isize, location: NSPoint },
}
struct Delivery {
    button: Retained<AnyObject>,
    action: Action,
    queued: Instant,
}
struct Deliveries {
    entries: [Option<Delivery>; CAPACITY],
    head: usize,
    len: usize,
    cancel_drag: bool,
}
impl Deliveries {
    const fn new() -> Self {
        Self {
            entries: [const { None }; CAPACITY],
            head: 0,
            len: 0,
            cancel_drag: false,
        }
    }
    fn pop(&mut self) -> Option<Delivery> {
        if self.len == 0 {
            return None;
        }
        let delivery = self.entries[self.head].take();
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        delivery
    }
    fn cancel(&mut self) {
        *self = Self {
            cancel_drag: true,
            ..Self::new()
        };
    }
    fn push(&mut self, delivery: Delivery) {
        // A motion burst needs only its latest point, never another slot.
        // Began/Ended remain distinct and ordered, including across AX reentry.
        if matches!(delivery.action, Action::Drag { state: 2, .. }) && self.len != 0 {
            let tail = (self.head + self.len - 1) % CAPACITY;
            if let Some(previous) = &mut self.entries[tail]
                && matches!(previous.action, Action::Drag { state: 2, .. })
                && Retained::as_ptr(&previous.button) == Retained::as_ptr(&delivery.button)
            {
                *previous = delivery;
                return;
            }
        }
        if self.len == CAPACITY {
            if matches!(delivery.action, Action::Drag { .. }) {
                // Do not deliver an Ended after losing its matching boundary.
                self.cancel();
            }
            return;
        }
        self.entries[(self.head + self.len) % CAPACITY] = Some(delivery);
        self.len += 1;
    }
    fn expire(&mut self) {
        while self.len != 0
            && self.entries[self.head]
                .as_ref()
                .is_some_and(|delivery| delivery.queued.elapsed() >= MAX_AGE)
        {
            if matches!(self.pop().unwrap().action, Action::Drag { .. }) {
                self.cancel();
                break;
            }
        }
    }
}
thread_local! {
    static PENDING: RefCell<Deliveries> = const { RefCell::new(Deliveries::new()) };
}

pub(super) fn clear(target: Obj) {
    PENDING.with(|pending| *pending.borrow_mut() = Deliveries::new());
    unsafe {
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:target, selector:sel!(tileTabClick:), object:ptr::null::<AnyObject>()];
    }
}

pub(super) fn cancel_drag(target: Obj) {
    PENDING.with(|pending| pending.borrow_mut().cancel());
    drain(target, sel!(tileTabClick:), ptr::null_mut());
}

pub(super) fn enqueue(target: Obj, sender: Obj, action: Action) {
    let Some(button) = (unsafe { Retained::retain(sender) }) else {
        return;
    };
    PENDING.with(|pending| {
        pending.borrow_mut().push(Delivery {
            button,
            action,
            queued: Instant::now(),
        });
    });
    drain(target, sel!(tileTabClick:), ptr::null_mut());
}

pub(super) extern "C-unwind" fn drain(target: Obj, _: Sel, _: Obj) {
    for _ in 0..CAPACITY {
        let mut found_app = false;
        let delivered = try_with_app(|app| {
            found_app = true;
            let (cancel, delivery) = PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                pending.expire();
                (std::mem::take(&mut pending.cancel_drag), pending.pop())
            });
            if cancel {
                app.tile_manager.cancel_tab_drag();
            }
            let Some(delivery) = delivery else {
                return;
            };
            unsafe {
                let panel: Obj = msg_send![&*delivery.button, window];
                if panel.is_null()
                    || !msg_send![panel, isVisible]
                    || !msg_send![panel, isOnActiveSpace]
                {
                    if matches!(delivery.action, Action::Drag { .. }) {
                        app.tile_manager.cancel_tab_drag();
                    }
                    return;
                }
            }
            apply_tab_action(
                app,
                Retained::as_ptr(&delivery.button) as Obj,
                delivery.action,
            );
        });
        if delivered && !found_app {
            clear(target);
            return;
        }
        if !delivered {
            PENDING.with(|pending| pending.borrow_mut().expire());
            break;
        }
        if pending_count() == 0 {
            clear(target);
            return;
        }
    }
    if pending_count() == 0 {
        clear(target);
    } else {
        schedule_app_callback(target, sel!(tileTabClick:), 0.01);
    }
}

pub(super) fn pending_count() -> usize {
    PENDING.with(|pending| {
        let pending = pending.borrow();
        pending.len + usize::from(pending.cancel_drag)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pan(button: &Retained<AnyObject>, state: isize, x: f64) -> Delivery {
        Delivery {
            button: button.clone(),
            action: Action::Drag {
                state,
                location: NSPoint::new(x, 20.),
            },
            queued: Instant::now(),
        }
    }

    #[test]
    fn motion_burst_preserves_original_press_and_final_release_points() {
        let button: Retained<AnyObject> = unsafe { msg_send![class!(NSObject), new] };
        let mut pending = Deliveries::new();
        pending.push(pan(&button, 1, 10.));
        for x in 0..10_000 {
            pending.push(pan(&button, 2, x as f64));
        }
        pending.push(pan(&button, 3, 12_000.));
        for (phase, x) in [(1, 10.), (2, 9_999.), (3, 12_000.)] {
            let event = pending.pop().unwrap();
            assert!(Retained::as_ptr(&event.button) == Retained::as_ptr(&button));
            assert!(
                matches!(event.action, Action::Drag { state, location } if state == phase && location.x == x)
            );
        }
        assert!(pending.pop().is_none());
        assert!(!pending.cancel_drag);
    }

    #[test]
    fn an_expired_pan_discards_its_release_and_requests_cancellation() {
        let button: Retained<AnyObject> = unsafe { msg_send![class!(NSObject), new] };
        let mut pending = Deliveries::new();
        let mut began = pan(&button, 1, 10.);
        began.queued -= MAX_AGE;
        pending.push(began);
        pending.push(pan(&button, 2, 50.));
        pending.push(pan(&button, 3, 80.));
        pending.expire();
        assert!(pending.cancel_drag);
        assert!(pending.pop().is_none());
    }

    #[test]
    fn overflow_cancels_instead_of_delivering_an_unmatched_release() {
        let button: Retained<AnyObject> = unsafe { msg_send![class!(NSObject), new] };
        let mut pending = Deliveries::new();
        for _ in 0..CAPACITY {
            pending.push(pan(&button, 1, 10.));
        }
        pending.push(pan(&button, 3, 80.));
        assert!(pending.cancel_drag);
        assert!(pending.pop().is_none());
        pending.push(pan(&button, 1, 100.));
        assert!(matches!(
            pending.pop().unwrap().action,
            Action::Drag { state: 1, .. }
        ));
    }
}
