//! AT-6 (WI-11): the mapping of engine failures to problems (ADR 0026): one code for each
//! `HandleError` case, and no library message in the detail unless it was scrubbed (ASM-194).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

use umbra_api_types::ProblemCode;
use umbra_engine::bus::RunState;
use umbra_engine::handle::HandleError;
use umbra_engine::{Confinement, EntityId, SessionError, SimTime, TemplateId, TickRate};
use umbra_server::ApiError;

fn rate() -> TickRate {
    TickRate::new(30).expect("rate")
}

fn cases() -> Vec<(HandleError, ProblemCode)> {
    vec![
        (HandleError::Busy, ProblemCode::EngineBusy),
        (HandleError::Closed, ProblemCode::EngineUnavailable),
        (HandleError::NoSession, ProblemCode::NoScenario),
        (
            HandleError::InvalidState {
                state: RunState::Paused,
                command: "start",
            },
            ProblemCode::InvalidState,
        ),
        (HandleError::NotStepped, ProblemCode::NotStepped),
        (HandleError::StepInProgress, ProblemCode::StepInProgress),
        (
            HandleError::StepCancelled {
                reached: SimTime::start(rate()),
            },
            ProblemCode::StepCancelled,
        ),
        (
            HandleError::Session(SessionError::TemplateNotFound(
                "t.x".parse::<TemplateId>().expect("id"),
            )),
            ProblemCode::TemplateNotFound,
        ),
        (
            HandleError::Session(SessionError::DuplicateName("a".into())),
            ProblemCode::DuplicateName,
        ),
        (
            HandleError::Session(SessionError::EntityNotFound(EntityId::new(3).expect("id"))),
            ProblemCode::EntityNotFound,
        ),
    ]
}

#[test]
fn every_handle_error_has_its_code_and_status() {
    for (error, code) in cases() {
        let problem = ApiError::from_handle_error(&error);
        assert_eq!(problem.code(), code, "{error}");
        assert_eq!(problem.status(), code.status(), "{error}");
        assert!(!problem.into_problem().detail.is_empty(), "{error}");
    }
}

#[test]
fn the_scrubbing_form_maps_to_the_same_codes() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("handle-errors");
    std::fs::create_dir_all(&dir).expect("dir");
    let confinement = Confinement::new(&dir, &[]).expect("confinement");
    for (error, code) in cases() {
        let problem = ApiError::from_handle_error_in(&error, &confinement);
        assert_eq!(problem.code(), code, "{error}");
    }
}

#[test]
fn state_and_step_details_use_wire_names_and_plain_numbers() {
    let invalid = HandleError::InvalidState {
        state: RunState::Paused,
        command: "start",
    };
    assert_eq!(
        ApiError::from_handle_error(&invalid).into_problem().detail,
        "start is not valid while the run is paused"
    );
    let cancelled = HandleError::StepCancelled {
        reached: SimTime::at_tick(rate(), 12),
    };
    assert_eq!(
        ApiError::from_handle_error(&cancelled)
            .into_problem()
            .detail,
        "the step was cancelled at tick 12"
    );
}
