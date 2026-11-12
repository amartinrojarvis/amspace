//! Pure presentation and geometry rules; independent of AppKit.
pub fn indicator(active: usize, count: usize) -> String {
    if count == 0 || active == 0 || active > count {
        return "—".into();
    }
    if count > 9 {
        return format!("{active} / {count}");
    }
    (1..=count)
        .map(|i| {
            if i == active {
                i.to_string()
            } else {
                "·".into()
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

pub fn thickness(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(1., 6.)
    } else {
        2.
    }
}
/// Border color and its contrast-safe backing, computed only on color changes.
#[derive(Clone, Copy, PartialEq)]
pub struct TabAccent {
    pub rgb: [f64; 3],
    backing: [f64; 3],
}

/// The shipped default: the same indigo the border falls back to.
pub const DEFAULT_ACCENT: [f64; 3] = [0.42, 0.48, 0.95];
/// Neutral well: both opaque black and white icon details clear 4.5:1.
pub const TAB_ICON_BACKING: [f64; 3] = [0.46; 3];

/// Relative-luminance ceiling that still clears 4.5:1 against white text:
/// `(1.0 + 0.05) / 4.5 - 0.05`.
const ACTIVE_LUMINANCE: f64 = 0.183_33;

fn channel(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        fallback
    }
}

fn linear(channel: f64) -> f64 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(rgb: [f64; 3]) -> f64 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

impl TabAccent {
    /// Clamp finite channels; repair non-finite preferences with the default.
    pub fn new(rgb: [f64; 3]) -> Self {
        let rgb = [
            channel(rgb[0], DEFAULT_ACCENT[0]),
            channel(rgb[1], DEFAULT_ACCENT[1]),
            channel(rgb[2], DEFAULT_ACCENT[2]),
        ];
        let mut backing = rgb;
        if luminance(rgb) > ACTIVE_LUMINANCE {
            // Scale all channels together. Keep the satisfying side of the
            // monotone search so rounding cannot reduce text contrast.
            let mut low = 0.;
            let mut high = 1.;
            for _ in 0..12 {
                let mid = (low + high) / 2.;
                if luminance(rgb.map(|channel| channel * mid)) > ACTIVE_LUMINANCE {
                    high = mid;
                } else {
                    low = mid;
                }
            }
            backing = rgb.map(|channel| channel * low);
        }
        Self { rgb, backing }
    }

    pub fn backing(self) -> [f64; 3] {
        self.backing
    }
}

impl Default for TabAccent {
    fn default() -> Self {
        Self::new(DEFAULT_ACCENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_positions_and_compaction() {
        assert_eq!(indicator(1, 3), "1  ·  ·");
        assert_eq!(indicator(2, 3), "·  2  ·");
        assert_eq!(indicator(3, 3), "·  ·  3");
        assert_eq!(indicator(1, 1), "1");
        assert_eq!(indicator(12, 14), "12 / 14");
        for (a, n) in [(0, 0), (0, 3), (4, 3)] {
            assert_eq!(indicator(a, n), "—");
        }
    }
    #[test]
    fn corrupt_preferences_cannot_break_geometry() {
        assert_eq!(thickness(f64::NAN), 2.);
        assert_eq!(thickness(f64::INFINITY), 2.);
        assert_eq!(thickness(-8.), 1.);
        assert_eq!(thickness(100.), 6.);
        assert_eq!(thickness(2.5), 2.5);
    }
    #[test]
    fn active_backing_keeps_white_legible_over_every_supported_accent() {
        // The shipped default, the three extra Settings presets and both extremes.
        let palette = [
            DEFAULT_ACCENT,
            [0.25, 0.72, 0.62],
            [0.94, 0.65, 0.27],
            [0.88, 0.42, 0.60],
            [0., 0., 0.],
            [1., 1., 1.],
        ];
        for rgb in palette {
            let accent = TabAccent::new(rgb);
            let backing = accent.backing();
            assert!(
                contrast_with_white(backing) >= 4.5,
                "accent {rgb:?} backing {backing:?} is too light"
            );
            // One scale factor over all channels: the R:G:B ratios — the hue — survive.
            let peak = rgb[0].max(rgb[1]).max(rgb[2]);
            let kept = backing[0].max(backing[1]).max(backing[2]);
            for index in 0..3 {
                let expected = if peak > 0. {
                    rgb[index] * (kept / peak)
                } else {
                    0.
                };
                assert!(
                    (backing[index] - expected).abs() < 1e-3,
                    "hue drifted on {rgb:?}"
                );
            }
        }
        // Black stays black, white lands on the ceiling, nonfinite is repaired.
        assert_eq!(TabAccent::new([0., 0., 0.]).backing(), [0., 0., 0.]);
        assert!(
            (luminance(TabAccent::new([1., 1., 1.]).backing()) - ACTIVE_LUMINANCE).abs() < 1e-3
        );
        assert_eq!(
            TabAccent::new([f64::NAN, f64::INFINITY, 0.5]).rgb,
            [DEFAULT_ACCENT[0], DEFAULT_ACCENT[1], 0.5]
        );
        assert_eq!(TabAccent::default().rgb, DEFAULT_ACCENT);
    }

    #[test]
    fn custom_accents_and_monochrome_icons_keep_contrast() {
        for r in [0., 0.1, 0.25, 0.5, 0.75, 1.] {
            for g in [0., 0.1, 0.25, 0.5, 0.75, 1.] {
                for b in [0., 0.1, 0.25, 0.5, 0.75, 1.] {
                    assert!(contrast_with_white(TabAccent::new([r, g, b]).backing()) >= 4.5);
                }
            }
        }
        assert!(contrast_with_white(TAB_ICON_BACKING) >= 4.5);
        assert!((luminance(TAB_ICON_BACKING) + 0.05) / 0.05 >= 4.5);
    }

    fn contrast_with_white(rgb: [f64; 3]) -> f64 {
        (1. + 0.05) / (luminance(rgb) + 0.05)
    }
}
