use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use talin::chain::{ChainOutput, Ctor};

pub use crate::element_selection::types::*;
pub use crate::formulation::types::*;
pub use crate::refinement::types::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, Hash)]
#[schemars(inline)]
pub struct ListDisplayedVec<T>(Vec<T>);

impl<T> std::fmt::Display for ListDisplayedVec<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for el in &self.0 {
            writeln!(f, "- {}", el)?;
        }
        Ok(())
    }
}

impl<T> From<Vec<T>> for ListDisplayedVec<T> {
    fn from(vec: Vec<T>) -> Self {
        ListDisplayedVec(vec)
    }
}

impl<T> From<ListDisplayedVec<T>> for Vec<T> {
    fn from(list: ListDisplayedVec<T>) -> Self {
        list.0
    }
}

impl<T> IntoIterator for ListDisplayedVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T> std::ops::Deref for ListDisplayedVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for ListDisplayedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Extend<T> for ListDisplayedVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

pub type Constraints = ListDisplayedVec<String>;
pub type Elements = ListDisplayedVec<String>;

/// The top-level input to the ideation pipeline.
///
/// Describes what the pipeline should ideate on: a core goal statement and a set of key constraints
/// that bound the search space.
#[derive(Serialize, Deserialize, Debug, Clone, Ctor, ChainOutput, Hash)]
pub struct Objective {
    pub core_goal: String,
    pub key_constraints: Constraints,
}

impl std::fmt::Display for Objective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Core Goal:\n{}\n", self.core_goal)?;
        writeln!(f, "Key Constraints:\n{}", self.key_constraints)
    }
}
