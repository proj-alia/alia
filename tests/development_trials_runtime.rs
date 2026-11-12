//! Public DTO boundary only. Projection and driver assertions live inside the
//! registered production module; no implementation is compiled a second time.
use alia::domain::development_trials::{TrialBinding, TrialSide};
use serde_json::json;
use uuid::Uuid;

#[test]
fn external_trial_binding_is_closed_opaque_correlation() {
    let binding = TrialBinding {
        plan_id: Uuid::new_v4(),
        case_id: Uuid::new_v4(),
        side: TrialSide::Baseline,
        attempt_id: Uuid::new_v4(),
        evaluation_run_id: Uuid::new_v4(),
        lease_generation: 1,
    };
    let mut value = serde_json::to_value(&binding).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 6);
    assert_eq!(
        serde_json::from_value::<TrialBinding>(value.clone()).unwrap(),
        binding
    );
    value["criterion"] = json!({"expected":"must not be model input"});
    assert!(serde_json::from_value::<TrialBinding>(value).is_err());
}
