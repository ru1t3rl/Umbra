//! Tick pipeline benchmarks (NFR-1 groundwork, ASM-040).
//!
//! Whole ticks of 1,000 and 10,000 owned entities with routes and a constant-velocity stub model
//! (the kernel may not depend on `cgf-motion`, ASM-005). Gate 14 only checks that this compiles;
//! there is no stored baseline.

#![forbid(unsafe_code)]
#![expect(
    clippy::expect_used,
    reason = "benchmark setup with known-valid values"
)]

use std::sync::Arc;

use cgf_core::{
    Ecef, FlatTerrain, Intent, KinematicState, Meters, MetersPerSecond, MotionModel, MotionModelId,
    Ownership, Quaternion, Seconds, TerrainProvider, TickRate, Vec3,
};
use cgf_kernel::{ClockMode, EntityInit, Kernel, KernelConfig, ManualWallClock, RouteInit};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const A: f64 = 6_378_137.0;

/// Moves the entity at a constant ECEF velocity of 1 m/s along +y (east at longitude 0); ignores intents.
struct ConstantVelocity {
    id: MotionModelId,
}

impl MotionModel for ConstantVelocity {
    fn id(&self) -> &MotionModelId {
        &self.id
    }

    fn step(
        &self,
        state: &KinematicState,
        _intents: &[Intent],
        dt: Seconds,
        _terrain: &dyn TerrainProvider,
    ) -> KinematicState {
        let mut out = *state;
        out.position = Ecef(Vec3::new(
            state.position.0.x,
            state.position.0.y + dt.0,
            state.position.0.z,
        ));
        out
    }
}

fn kernel_with_entities(count: u32) -> Kernel {
    let mut kernel = Kernel::new(
        KernelConfig {
            tick_rate: TickRate::new(30).expect("non-zero rate"),
            seed: 1,
            clock: ClockMode::AsFastAsPossible,
        },
        Box::new(FlatTerrain {
            height: Meters(0.0),
        }),
        Box::new(ManualWallClock::new()),
    );
    let model: Arc<dyn MotionModel> = Arc::new(ConstantVelocity {
        id: MotionModelId::new("bench.constant.velocity").expect("valid id"),
    });
    for i in 0..count {
        let y = f64::from(i);
        let at = |dy: f64| Ecef(Vec3::new(A, y + dy, 0.0));
        let init = EntityInit {
            state: KinematicState {
                position: at(0.0),
                velocity: Vec3::ZERO,
                orientation: Quaternion::IDENTITY,
            },
            ownership: Ownership::Owned,
            motion: Arc::clone(&model),
            route: Some(RouteInit {
                waypoints: vec![at(500.0), at(1000.0)],
                arrival_radius: Meters(5.0),
                cruise_speed: MetersPerSecond(10.0),
            }),
        };
        kernel.spawn(init).expect("spawn");
    }
    kernel
}

fn tick_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_pipeline");
    for count in [1_000_u32, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            let mut kernel = kernel_with_entities(count);
            b.iter(|| kernel.tick().expect("tick"));
        });
    }
    group.finish();
}

criterion_group!(benches, tick_pipeline);
criterion_main!(benches);
