//! Duration suggestion from commit timestamps. Only a suggestion; the user decides.

use serde::Serialize;

/// A pause longer than this starts a new work session.
const GAP_SECS: i64 = 90 * 60;
/// Time added per session for work before its first commit.
const RAMP_HOURS: f64 = 0.25;
/// The result is rounded up to this step.
const STEP_HOURS: f64 = 0.25;

#[derive(Debug, PartialEq, Serialize)]
pub struct Suggestion {
    pub hours: f64,
    pub sessions: usize,
    pub commits: usize,
}

/// `timestamps` are unix seconds, in any order.
pub fn suggest(timestamps: &[i64]) -> Suggestion {
    let mut times = timestamps.to_vec();
    times.sort_unstable();
    let Some((&first, rest)) = times.split_first() else {
        return Suggestion {
            hours: STEP_HOURS,
            sessions: 0,
            commits: 0,
        };
    };

    let (mut sessions, mut total_secs) = (1, 0);
    let (mut session_start, mut previous) = (first, first);
    for &time in rest {
        if time - previous > GAP_SECS {
            total_secs += previous - session_start;
            sessions += 1;
            session_start = time;
        }
        previous = time;
    }
    total_secs += previous - session_start;

    let raw = total_secs as f64 / 3600.0 + sessions as f64 * RAMP_HOURS;
    let hours = ((raw / STEP_HOURS) - 1e-9).ceil().max(1.0) * STEP_HOURS;
    Suggestion {
        hours,
        sessions,
        commits: times.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(h: i64, m: i64) -> i64 {
        h * 3600 + m * 60
    }

    #[test]
    fn single_session_spans_commits_plus_ramp() {
        let s = suggest(&[at(14, 0), at(15, 10), at(14, 30)]);
        assert_eq!(
            s,
            Suggestion {
                hours: 1.5,
                sessions: 1,
                commits: 3
            }
        );
    }

    #[test]
    fn long_pauses_split_sessions() {
        let s = suggest(&[at(9, 0), at(10, 0), at(14, 0), at(14, 30)]);
        assert_eq!(
            s,
            Suggestion {
                hours: 2.0,
                sessions: 2,
                commits: 4
            }
        );
    }

    #[test]
    fn minimum_is_one_step() {
        assert_eq!(suggest(&[]).hours, 0.25);
        assert_eq!(suggest(&[at(9, 0)]).hours, 0.25);
    }
}
