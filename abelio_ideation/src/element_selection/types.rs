use serde::{Deserialize, Serialize};
use talin::chain::{ChainOutput, Ctor};

use crate::types::{Elements, ListDisplayedVec};

#[derive(Serialize, Deserialize, Debug, Clone, ChainOutput, Ctor, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct FoundElements {
    /// A list of searched elements, without numbering or citations
    pub elements: Elements,
}

#[derive(Serialize, Deserialize, Debug, Clone, ChainOutput, Ctor, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct DistilledElements {
    /// A list of distilled elements
    pub distilled: Elements,
}

#[derive(Serialize, Deserialize, Debug, Clone, ChainOutput, Ctor, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct AlternativeEntities {
    /// A list of entities
    pub entities: Vec<String>,
}

pub type MinorRequirements = ListDisplayedVec<String>;

#[derive(Serialize, Deserialize, Debug, Clone, ChainOutput, Ctor, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct Requirement {
    /// An identified major requirement
    pub major: String,
    /// A list of minor requirements or constraints
    pub minor: MinorRequirements,
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Major Requirement:\n{}\n", self.major)?;
        writeln!(f, "Minor Requirements:\n{}", self.minor)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ChainOutput, Ctor, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct FilteredElements {
    /// A list of filtered elements
    pub filtered: Elements,
}

pub type EntityElementSearchAgentOutputCtor = FoundElementsCtor;

pub type ConceptElementSearchAgentOutputCtor = FoundElementsCtor;

pub type ElementMergerAgentOutputCtor = DistilledElementsCtor;

pub type AlternativeElementSearchAgentOutputCtor = AlternativeEntitiesCtor;

pub type RequirementSearchAgentOutputCtor = RequirementCtor;

pub type ElementNoveltyFilterAgentOutputCtor = FilteredElementsCtor;
pub type ElementConstraintsFilterAgentOutputCtor = FilteredElementsCtor;
pub type ElementRequirementFilterAgentOutputCtor = FilteredElementsCtor;
