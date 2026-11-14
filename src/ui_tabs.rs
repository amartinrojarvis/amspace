//! Tab clicks retain their original control across nested AX/AppKit delivery.
use super::*;

const CAPACITY: usize = 8;
const MAX_AGE: Duration = Duration::from_millis(350);

#[derive(Clone, Copy)]
pub(super) enum Action {
    Select,
    Close,
}
struct Click {
    button: Retained<AnyObject>,
    action: Action,
    queued: Instant,
}
struct Clicks {
    entries: [Option<Click>; CAPACITY],
    head: usize,
    len: usize,
}
impl Clicks {
    const fn new() -> Self {
        Self {
            entries: [const { None }; CAPACITY],
            head: 0,
            len: 0,
        }
    }
    fn pop(&mut self) -> Option<Click> {
        if self.len == 0 {
            return None;
        }
        let click = self.entries[self.head].take();
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        click
    }
}
thread_local! {
    static PENDING: RefCell<Clicks> = const { RefCell::new(Clicks::new()) };
}

pub(super) fn clear(target: Obj) {
    PENDING.with(|pending| *pending.borrow_mut() = Clicks::new());
    unsafe {
        let _: () = msg_send![class!(NSObject), cancelPreviousPerformRequestsWithTarget:target, selector:sel!(tileTabClick:), object:ptr::null::<AnyObject>()];
    }
}

pub(super) fn enqueue(target: Obj, sender: Obj, action: Action) {
    let Some(button) = (unsafe { Retained::retain(sender) }) else {
        return;
    };
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        // A bounded queue never turns a burst into delayed destructive actions.
        if pending.len == CAPACITY {
            return;
        }
        let index = (pending.head + pending.len) % CAPACITY;
        pending.entries[index] = Some(Click {
            button,
            action,
            queued: Instant::now(),
        });
        pending.len += 1;
    });
    drain(target, sel!(tileTabClick:), ptr::null_mut());
}

pub(super) extern "C-unwind" fn drain(target: Obj, _: Sel, _: Obj) {
    // Bound work even if a synchronous AX call dispatches more clicks.
    for _ in 0..CAPACITY {
        let mut found_app = false;
        let delivered = try_with_app(|app| {
            found_app = true;
            let Some(click) = PENDING.with(|pending| pending.borrow_mut().pop()) else {
                return;
            };
            if click.queued.elapsed() >= MAX_AGE {
                return;
            }
            unsafe {
                let panel: Obj = msg_send![&*click.button, window];
                if panel.is_null() {
                    return;
                }
                let visible: bool = msg_send![panel, isVisible];
                let active: bool = msg_send![panel, isOnActiveSpace];
                if !visible || !active {
                    return;
                }
            }
            apply_tab_click(app, Retained::as_ptr(&click.button) as Obj, click.action);
        });
        if delivered && !found_app {
            clear(target);
            return;
        }
        if !delivered {
            PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                while pending.entries[pending.head]
                    .as_ref()
                    .is_some_and(|click| click.queued.elapsed() >= MAX_AGE)
                {
                    pending.pop();
                }
            });
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
    PENDING.with(|pending| pending.borrow().len)
}
