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
}
