use rand::seq::SliceRandom;

use crate::agents::{
    ElementConstraintsFilterAgent, ElementConstraintsFilterAgentInput, ElementNoveltyFilterAgent,
    ElementNoveltyFilterAgentInput, ElementRequirementFilterAgent,
    ElementRequirementFilterAgentInput,
};
use crate::types::{Constraints, Elements, Requirement};
use crate::utils::ChatHistory;

#[tracing::instrument(skip_all)]
pub async fn filter_elements(
    formulation: &str,
    constraints: &Constraints,
    requirement: &Requirement,
    elements: &Elements,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Elements> {
    // a. Novelty filter + shuffle
    let elements: Elements = resm::cached_json!(
        store,
        "a_novelty_filtered_elements",
        deps: &(ElementNoveltyFilterAgent.fingerprint(), elements, formulation),
        {
            let agent = ElementNoveltyFilterAgent::default().await;
            let input = ElementNoveltyFilterAgentInput::new(elements, formulation);
            let mut filtered = ChatHistory::run_once(&agent, input).await?.filtered;
            filtered.shuffle(&mut rand::rng());
            Ok::<_, anyhow::Error>(filtered)
        }
    )?;

    // b. Constraints filter + shuffle
    let elements: Elements = resm::cached_json!(
        store,
        "b_constraints_filtered_elements",
        deps: &(ElementConstraintsFilterAgent.fingerprint(), &elements, constraints, formulation),
        {
            let agent = ElementConstraintsFilterAgent::default().await;
            let input = ElementConstraintsFilterAgentInput::new(constraints, &elements, formulation);
            let mut filtered = ChatHistory::run_once(&agent, input).await?.filtered;
            filtered.shuffle(&mut rand::rng());
            Ok::<_, anyhow::Error>(filtered)
        }
    )?;

    // c. Requirement filter + shuffle
    let elements: Elements = resm::cached_json!(
        store,
        "c_requirements_filtered_elements",
        deps: &(ElementRequirementFilterAgent.fingerprint(), &elements, &requirement.minor, formulation),
        {
            let agent = ElementRequirementFilterAgent::default().await;
            let input =
                ElementRequirementFilterAgentInput::new(&elements, &requirement.minor, formulation);
            let mut filtered = ChatHistory::run_once(&agent, input).await?.filtered;
            filtered.shuffle(&mut rand::rng());
            Ok::<_, anyhow::Error>(filtered)
        }
    )?;

    Ok(elements)
}
