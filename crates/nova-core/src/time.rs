//! Deterministic helpers for timeline time keeping.

/// Converts a frame index to seconds at the given frame rate.
pub fn frame_to_seconds(frame: usize, fps: f32) -> f32 {
    if fps <= 0.0 {
        return 0.0;
    }
    frame as f32 / fps
}

/// Converts seconds to a frame index, clamped to `total_frames`.
pub fn seconds_to_frame(seconds: f32, fps: f32, total_frames: usize) -> usize {
    if fps <= 0.0 || total_frames == 0 {
        return 0;
    }
    let frame = (seconds.max(0.0) * fps).floor() as usize;
    frame.min(total_frames.saturating_sub(1))
}

/// Wraps a playhead that has advanced by `elapsed` seconds.
///
/// Returns the new frame index and the remaining time that did not fill a
/// whole frame, so playback does not drift.
pub fn advance_playhead(
    frame: usize,
    elapsed: f32,
    fps: f32,
    total_frames: usize,
    looping: bool,
) -> (usize, f32) {
    if fps <= 0.0 || total_frames == 0 {
        return (0, 0.0);
    }
    let frame_duration = 1.0 / fps;
    let mut remaining = elapsed + frame_duration * 0.0001;
    let mut current = frame;
    while remaining >= frame_duration {
        remaining -= frame_duration;
        current += 1;
        if current >= total_frames {
            if looping {
                current = 0;
            } else {
                current = total_frames - 1;
                remaining = 0.0;
                break;
            }
        }
    }
    (current, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_frames() {
        assert_eq!(frame_to_seconds(12, 24.0), 0.5);
        assert_eq!(seconds_to_frame(0.5, 24.0, 100), 12);
        assert_eq!(seconds_to_frame(99.0, 24.0, 100), 99);
    }

    #[test]
    fn advances_without_losing_remainder() {
        let (f, rest) = advance_playhead(0, 1.5 / 24.0, 24.0, 10, true);
        assert_eq!(f, 1);
        assert!((rest - 0.5 / 24.0).abs() < 1e-5);
    }

    #[test]
    fn loops_and_stops() {
        assert_eq!(advance_playhead(9, 1.0 / 24.0, 24.0, 10, true).0, 0);
        assert_eq!(advance_playhead(9, 1.0, 24.0, 10, false).0, 9);
    }
}
