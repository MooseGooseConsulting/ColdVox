/// Steps that the demo driver emits to exercise the overlay contract.
///
/// The demo driver is the only consumer of these variants. Real STT wiring
/// will bypass the demo path entirely once it lands.
#[derive(Debug, Clone)]
pub enum DemoStep {
    /// Emit a partial transcript update (text visible while speech is ongoing).
    Partial(String),
    /// Promote partial text to final and transition to Ready.
    Final(String),
    /// Pause playback for the given number of milliseconds.
    Wait(u64),
}

/// Canonical demo script used by the overlay shell demo driver.
///
/// The sequence exercises the full Idle → Listening → (partials) → Ready
/// state machine so that the seam can be verified end-to-end without a live
/// audio or STT runtime.
pub fn demo_script() -> Vec<DemoStep> {
    vec![
        DemoStep::Wait(300),
        DemoStep::Partial("Streaming partials".to_string()),
        DemoStep::Wait(400),
        DemoStep::Partial("Streaming partials from".to_string()),
        DemoStep::Wait(350),
        DemoStep::Partial("Streaming partials from the".to_string()),
        DemoStep::Wait(300),
        DemoStep::Partial("Streaming partials from the demo".to_string()),
        DemoStep::Wait(350),
        DemoStep::Partial("Streaming partials from the demo driver".to_string()),
        DemoStep::Wait(400),
        DemoStep::Final("Streaming partials from the demo driver arrive here.".to_string()),
    ]
}
