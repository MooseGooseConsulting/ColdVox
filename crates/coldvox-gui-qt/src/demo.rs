/// Demo script steps — mirrors the Tauri backend's demo.rs so both shells
/// exercise the same state sequence.
#[derive(Debug, Clone)]
pub enum DemoStep {
    Partial(&'static str),
    Final(&'static str),
}

/// Canonical demo sequence.  Returns a slice of steps that drive the overlay
/// from Listening through a series of partial updates to a final commit.
pub fn demo_script() -> &'static [DemoStep] {
    &[
        DemoStep::Partial("Streaming partials"),
        DemoStep::Partial("Streaming partials from"),
        DemoStep::Partial("Streaming partials from the"),
        DemoStep::Partial("Streaming partials from the demo"),
        DemoStep::Partial("Streaming partials from the demo driver"),
        DemoStep::Final("Streaming partials from the demo driver arrive here."),
    ]
}
