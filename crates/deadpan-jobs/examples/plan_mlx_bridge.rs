//! Emits a plan for the deliberately narrow development MLX probe envelope.
use std::error::Error;

use deadpan_core::{FrameDuration, FrameRate};
use deadpan_jobs::{
    AxisLimits, BridgeCapability, BridgeGenerationPlan, DimensionLimits, FrameCountFormula,
    NativeDimensions,
};

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 3 {
        return Err("expected interior_frames fps_numerator fps_denominator".into());
    }
    let frames = FrameDuration::new(arguments[0].parse()?)?;
    let rate = FrameRate::new(arguments[1].parse()?, arguments[2].parse()?)?;
    let capability = BridgeCapability::new(
        true,
        FrameRate::new(24, 1)?,
        FrameCountFormula::new(8, 1, 9, 97)?,
        DimensionLimits::new(
            AxisLimits::new(768, 768, 64)?,
            AxisLimits::new(320, 320, 64)?,
        ),
    );
    let plan =
        BridgeGenerationPlan::new(frames, rate, &capability, NativeDimensions::new(768, 320)?)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
