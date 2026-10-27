//! Capture gates (plan §5.2, §4.3, §7.4). Pure: OS facts in, decision out.

/// What the user is doing, platform-neutral (Windows: `SHQueryUserNotificationState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserActivity {
    Normal,
    Fullscreen,
    Presentation,
    /// Full-screen app or presentation settings.
    Busy,
    /// Lock screen, screen saver or switched-away user.
    NotPresent,
    /// The OS could not say. Treated like `Normal`: the state query only fails in exotic sessions.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateInputs {
    pub paused: bool,
    pub locked: bool,
    pub suspended: bool,
    pub idle_ms: u64,
    pub activity: UserActivity,
    pub on_battery: bool,
    pub battery_saver: bool,
}

/// Why capture is off. Content-free; shown in the notch as a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    Paused,
    Suspended,
    Locked,
    AwayFromDesktop,
    Fullscreen,
    Presentation,
    Idle,
}

impl Blocker {
    pub fn as_str(self) -> &'static str {
        match self {
            Blocker::Paused => "paused",
            Blocker::Suspended => "suspended",
            Blocker::Locked => "locked",
            Blocker::AwayFromDesktop => "away",
            Blocker::Fullscreen => "fullscreen",
            Blocker::Presentation => "presentation",
            Blocker::Idle => "idle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    /// `None` = capture allowed.
    pub blocked: Option<Blocker>,
    /// Multiply every capture interval by this (battery ×3, plan §5.3).
    pub cadence_multiplier: u32,
    /// Rung 3 (VLM) allowed.
    pub vlm_allowed: bool,
    /// Any GPU inference allowed (none while a full-screen app or presentation runs).
    pub gpu_allowed: bool,
    /// P2 background jobs (rollups): idle or locked, on AC (§4.3, §7.4).
    pub background_jobs_allowed: bool,
    pub hide_notch: bool,
}

/// No input for this long counts as idle (Appendix A `idle_after_s = 120`).
pub const IDLE_AFTER_MS: u64 = 120_000;
pub const BATTERY_MULTIPLIER: u32 = 3;

pub fn evaluate(i: &GateInputs) -> Gate {
    let fullscreenish = matches!(i.activity, UserActivity::Fullscreen | UserActivity::Busy);
    let presenting = i.activity == UserActivity::Presentation;
    let blocked = if i.paused {
        Some(Blocker::Paused)
    } else if i.suspended {
        Some(Blocker::Suspended)
    } else if i.locked {
        Some(Blocker::Locked)
    } else if i.activity == UserActivity::NotPresent {
        Some(Blocker::AwayFromDesktop)
    } else if presenting {
        Some(Blocker::Presentation)
    } else if fullscreenish {
        Some(Blocker::Fullscreen)
    } else if i.idle_ms >= IDLE_AFTER_MS {
        Some(Blocker::Idle)
    } else {
        None
    };
    let low_power = i.on_battery || i.battery_saver;
    let resting = i.locked || i.idle_ms >= IDLE_AFTER_MS || i.activity == UserActivity::NotPresent;
    Gate {
        blocked,
        cadence_multiplier: if low_power { BATTERY_MULTIPLIER } else { 1 },
        vlm_allowed: blocked.is_none() && !low_power,
        gpu_allowed: !fullscreenish && !presenting && !i.suspended,
        background_jobs_allowed: resting && !i.on_battery && !i.suspended && !i.paused,
        hide_notch: fullscreenish || presenting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> GateInputs {
        GateInputs {
            paused: false,
            locked: false,
            suspended: false,
            idle_ms: 1_000,
            activity: UserActivity::Normal,
            on_battery: false,
            battery_saver: false,
        }
    }

    #[test]
    fn normal_use_captures_at_full_cadence() {
        let g = evaluate(&base());
        assert_eq!(g.blocked, None);
        assert_eq!(g.cadence_multiplier, 1);
        assert!(g.vlm_allowed && g.gpu_allowed && !g.background_jobs_allowed && !g.hide_notch);
    }

    #[test]
    fn each_blocker_and_precedence() {
        let cases: [(GateInputs, Blocker); 7] = [
            (
                GateInputs {
                    paused: true,
                    locked: true,
                    ..base()
                },
                Blocker::Paused,
            ),
            (
                GateInputs {
                    suspended: true,
                    locked: true,
                    ..base()
                },
                Blocker::Suspended,
            ),
            (
                GateInputs {
                    locked: true,
                    idle_ms: 999_999,
                    ..base()
                },
                Blocker::Locked,
            ),
            (
                GateInputs {
                    activity: UserActivity::NotPresent,
                    ..base()
                },
                Blocker::AwayFromDesktop,
            ),
            (
                GateInputs {
                    activity: UserActivity::Presentation,
                    ..base()
                },
                Blocker::Presentation,
            ),
            (
                GateInputs {
                    activity: UserActivity::Fullscreen,
                    ..base()
                },
                Blocker::Fullscreen,
            ),
            (
                GateInputs {
                    idle_ms: IDLE_AFTER_MS,
                    ..base()
                },
                Blocker::Idle,
            ),
        ];
        for (i, want) in cases {
            assert_eq!(evaluate(&i).blocked, Some(want), "{i:?}");
        }
        assert_eq!(
            evaluate(&GateInputs {
                activity: UserActivity::Busy,
                ..base()
            })
            .blocked,
            Some(Blocker::Fullscreen)
        );
        assert_eq!(
            evaluate(&GateInputs {
                idle_ms: IDLE_AFTER_MS - 1,
                ..base()
            })
            .blocked,
            None
        );
    }

    #[test]
    fn fullscreen_hides_notch_and_stops_gpu() {
        let g = evaluate(&GateInputs {
            activity: UserActivity::Fullscreen,
            ..base()
        });
        assert!(g.hide_notch && !g.gpu_allowed && !g.vlm_allowed);
    }

    #[test]
    fn battery_slows_capture_and_defers_background_work() {
        let g = evaluate(&GateInputs {
            on_battery: true,
            idle_ms: IDLE_AFTER_MS,
            ..base()
        });
        assert_eq!(g.cadence_multiplier, 3);
        assert!(!g.vlm_allowed && !g.background_jobs_allowed);
        let g = evaluate(&GateInputs {
            battery_saver: true,
            ..base()
        });
        assert_eq!(g.cadence_multiplier, 3);
    }

    #[test]
    fn rollups_run_when_idle_or_locked_on_ac() {
        assert!(
            evaluate(&GateInputs {
                idle_ms: IDLE_AFTER_MS,
                ..base()
            })
            .background_jobs_allowed
        );
        assert!(evaluate(&GateInputs { locked: true, ..base() }).background_jobs_allowed);
        assert!(
            !evaluate(&GateInputs {
                locked: true,
                paused: true,
                ..base()
            })
            .background_jobs_allowed
        );
    }
}
