//! Duration suggestion from agent activity. Only a suggestion; the user decides.

/// The suggestion is rounded up to this step.
const STEP_HOURS: f64 = 0.25;

/// Active seconds → hours, rounded up to the next quarter hour (at least one).
pub fn round_up(seconds: i64) -> f64 {
    let raw = seconds.max(0) as f64 / 3600.0;
    ((raw / STEP_HOURS - 1e-9).ceil().max(1.0)) * STEP_HOURS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_up_to_quarter_hours() {
        assert_eq!(round_up(0), 0.25);
        assert_eq!(round_up(15 * 60), 0.25);
        assert_eq!(round_up(16 * 60), 0.5);
        assert_eq!(round_up(85 * 60), 1.5);
        assert_eq!(round_up(2 * 3600), 2.0);
    }
}
