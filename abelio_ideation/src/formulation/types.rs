use serde::{Deserialize, Serialize};
use talin::chain::{ChainOutput, Ctor};

#[derive(Debug, Clone, Serialize, Deserialize, ChainOutput, Ctor, Hash)]
#[serde(deny_unknown_fields)]
pub struct Formulations {
    /// A list of formulated goals
    #[serde(rename = "embodiments")]
    pub formulations: Vec<String>,
}

// Written by hand instead of `#[chain(derive_schema)]`, which names the schema after the Rust type:
// the model sees this name as the response format name and schema title, and it keeps the paper-run
// name.
impl talin::__private::schemars::JsonSchema for Formulations {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Embodiments")
    }

    fn json_schema(
        generator: &mut talin::__private::schemars::generate::SchemaGenerator,
    ) -> talin::__private::schemars::Schema {
        FormulationsPartial::json_schema(generator)
    }
}

pub type DivergentFormulationMechanismAgentOutputCtor = FormulationsCtor;
pub type DivergentFormulationScopeAgentOutputCtor = FormulationsCtor;
pub type DivergentFormulationGapsAgentOutputCtor = FormulationsCtor;
