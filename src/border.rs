//! Pure clipping for a passive overlay. Drawing a border never orders its owner.
use crate::native::rect;
use objc2_foundation::NSRect;

/// A passive border must not follow the old key window across a Space slide.
#[derive(Default)]
pub struct SpacePause(Option<std::time::Instant>);
impl SpacePause {
    /// Matches the model's quiet floor, so the border returns with the first
    /// authoritative scan instead of waiting an extra window.
    pub const DELAY: std::time::Duration = std::time::Duration::from_millis(400);
    pub fn begin(&mut self, now: std::time::Instant) {
        self.0 = Some(now + Self::DELAY);
    }
    pub fn pending(&self, now: std::time::Instant) -> bool {
        self.0.is_some_and(|until| now < until)
    }
}

/// Bounded lifetime of the full-screen frame drawn when a desktop is reselected.
/// A repeated click inside the window only extends it, so the overlay does not
/// restart its fade and flicker.
#[derive(Default)]
pub struct Flash(Option<std::time::Instant>);
impl Flash {
    pub const HOLD: std::time::Duration = std::time::Duration::from_millis(460);
    pub const FADE_IN: f64 = 0.1;
    pub const FADE_OUT: f64 = 0.16;
    pub const RGB: [f64; 3] = [0.94, 0.47, 0.46];
    pub const WIDTH: f64 = 2.;
    /// Slight rounding so the frame reads as one soft figure, not four rules.
    pub const CORNER: f64 = 8.;
    pub fn begin(&mut self, now: std::time::Instant) {
        self.0 = Some(now + Self::HOLD);
    }
    pub fn pending(&self, now: std::time::Instant) -> bool {
        self.0.is_some_and(|until| now < until)
    }
}

fn intersection(a: NSRect, b: NSRect) -> Option<NSRect> {
    let x = a.origin.x.max(b.origin.x);
    let y = a.origin.y.max(b.origin.y);
    let right = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    (right > x && bottom > y).then(|| rect(x, y, right - x, bottom - y))
}

/// Disjoint pieces of the target that are not covered by a window above it.
/// Sequential subtraction handles overlapping occluders without even/odd holes.
pub fn visible_regions(target: NSRect, occluders: &[NSRect]) -> Vec<NSRect> {
    let mut regions = vec![target];
    for occluder in occluders {
        let mut next = Vec::new();
        for region in regions {
            let Some(cut) = intersection(region, *occluder) else {
                next.push(region);
                continue;
            };
            for piece in [
                rect(
                    region.origin.x,
                    region.origin.y,
                    region.size.width,
                    cut.origin.y - region.origin.y,
                ),
                rect(
                    region.origin.x,
                    cut.origin.y + cut.size.height,
                    region.size.width,
                    region.origin.y + region.size.height - cut.origin.y - cut.size.height,
                ),
                rect(
                    region.origin.x,
                    cut.origin.y,
                    cut.origin.x - region.origin.x,
                    cut.size.height,
                ),
                rect(
                    cut.origin.x + cut.size.width,
                    cut.origin.y,
                    region.origin.x + region.size.width - cut.origin.x - cut.size.width,
                    cut.size.height,
                ),
            ] {
                if piece.size.width > 0. && piece.size.height > 0. {
                    next.push(piece);
                }
            }
        }
        regions = next;
        if regions.is_empty() {
            break;
        }
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiling::contains;
    use objc2_foundation::NSPoint;
    #[test]
    fn space_pause_hides_stale_border_and_old_callbacks_cannot_resume_a_new_slide() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut pause = SpacePause::default();
        assert!(!pause.pending(now));
        pause.begin(now);
        assert!(pause.pending(now));
        assert!(pause.pending(now + Duration::from_millis(399)));
        pause.begin(now + Duration::from_millis(200));
        assert!(pause.pending(now + Duration::from_millis(500)));
        assert!(pause.pending(now + SpacePause::DELAY));
        assert!(!pause.pending(now + Duration::from_millis(601)));
    }
    #[test]
    fn flash_frame_expires_once_and_a_repeat_extends_it_without_restarting() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut flash = Flash::default();
        assert!(!flash.pending(now));
        flash.begin(now);
        assert!(flash.pending(now));
        assert!(flash.pending(now + Duration::from_millis(459)));
        assert!(!flash.pending(now + Flash::HOLD));
        // A second click inside the window only extends the same frame.
        flash.begin(now + Duration::from_millis(300));
        assert!(flash.pending(now + Flash::HOLD));
        assert!(!flash.pending(now + Duration::from_millis(760)));
        // The fade must finish inside the held window, or the panel lingers.
        assert!(Flash::HOLD.as_secs_f64() - Flash::FADE_OUT > 0.3);
    }
    #[test]
    fn overlapping_occluders_cannot_reveal_a_covered_border() {
        let target = rect(-500., -300., 800., 600.);
        let covers = [
            rect(-600., -250., 450., 400.),
            rect(-300., -400., 450., 400.),
        ];
        let visible = visible_regions(target, &covers);
        for x in 0..80 {
            for y in 0..60 {
                let point = NSPoint::new(
                    target.origin.x + x as f64 * 10. + 5.,
                    target.origin.y + y as f64 * 10. + 5.,
                );
                let expected = !covers.iter().any(|r| contains(*r, point));
                assert_eq!(
                    visible.iter().filter(|r| contains(**r, point)).count(),
                    usize::from(expected)
                );
            }
        }
    }
    #[test]
    fn empty_covered_and_nonoverlapping_windows_have_exact_masks() {
        let target = rect(10., 41., 825., 947.);
        assert_eq!(visible_regions(target, &[]), vec![target]);
        assert_eq!(
            visible_regions(target, &[rect(845., 41., 825., 947.)]),
            vec![target]
        );
        assert!(visible_regions(target, &[target]).is_empty());
        assert_eq!(
            visible_regions(target, &[rect(10., 41., 400., 947.)]),
            vec![rect(410., 41., 425., 947.)]
        );
    }
}
