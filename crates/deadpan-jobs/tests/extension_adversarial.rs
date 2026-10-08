//! Gate G: mutate the exact extension plan/map crossing the worker boundary.
use deadpan_chaos::{Target, Verdict, fuzz, reject};
use deadpan_core::{ExtensionDirection, FrameDuration, FrameRate};
use deadpan_jobs::{
    AxisLimits, DimensionLimits, ExtensionCapability, ExtensionGenerationPlan, FrameCountFormula,
    NativeDimensions,
};

#[global_allocator]
static ALLOCATOR: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;

#[test]
fn adversarial_extension_plans_preserve_generated_only_samples() {
    // Same K9/E8/N12 conversion exercised by the independent real-media test.
    let capability = ExtensionCapability::new(
        FrameRate::new(24, 1).unwrap(),
        9,
        FrameCountFormula::new(8, 0, 8, 88).unwrap(),
        DimensionLimits::new(
            AxisLimits::new(768, 768, 64).unwrap(),
            AxisLimits::new(320, 320, 64).unwrap(),
        ),
        FrameDuration::new(180).unwrap(),
    )
    .unwrap();
    let seeds = [ExtensionDirection::FromLeft, ExtensionDirection::FromRight]
        .into_iter()
        .map(|direction| {
            let plan = ExtensionGenerationPlan::new(
                direction,
                FrameDuration::new(12).unwrap(),
                FrameRate::new(24, 1).unwrap(),
                &capability,
                NativeDimensions::new(768, 320).unwrap(),
            )
            .unwrap();
            serde_json::to_vec(&plan).unwrap()
        })
        .collect();
    fuzz(
        Target::json("jobs-extension-plan").iterations(1000),
        seeds,
        |input| {
            let plan = match serde_json::from_slice::<ExtensionGenerationPlan>(input) {
                Ok(plan) => plan,
                Err(error) => return reject(error),
            };
            let encoded = serde_json::to_vec(&plan).map_err(|error| error.to_string())?;
            let again: ExtensionGenerationPlan =
                serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
            if plan != again {
                return Err("extension plan changed during round trip".into());
            }
            let count =
                u64::try_from(plan.project_frames().frames()).map_err(|error| error.to_string())?;
            let interval = plan.sampling_map().generated_interval();
            // Never allocate/iterate from an untrusted count. Intrinsic validity
            // must permit bounded sampling even when a host capability rejects it.
            for index in [0, count / 2, count - 1] {
                let sample = plan.sample(index).map_err(|error| error.to_string())?;
                if !interval.contains(&i64::from(sample.lower_index()))
                    || !interval.contains(&i64::from(sample.upper_index()))
                {
                    return Err("extension interpolation read retained context".into());
                }
                if sample.upper_weight().compare_integer(0).is_lt()
                    || sample.upper_weight().compare_integer(1).is_ge()
                {
                    return Err("invalid extension interpolation weight".into());
                }
            }
            if plan.sample(count).is_ok() {
                return Err("extension accepted an out-of-range sample".into());
            }
            Ok(Verdict::Accepted)
        },
    )
    .assert_clean();
}
