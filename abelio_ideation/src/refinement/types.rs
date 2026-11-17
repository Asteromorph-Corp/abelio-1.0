use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use talin::chain::{ChainOutput, Ctor};

#[derive(Debug, Clone, Serialize)]
pub struct RefinementChallenge<'a> {
    pub critique: Cow<'a, str>,
    pub initial_idea: Cow<'a, str>,
    pub judge_feedback: Option<Cow<'a, str>>,
    pub round: usize,
}

impl<'a> RefinementChallenge<'a> {
    pub fn new(
        critique: impl Into<Cow<'a, str>>,
        initial_idea: impl Into<Cow<'a, str>>,
        round: usize,
    ) -> Self {
        Self {
            critique: critique.into(),
            initial_idea: initial_idea.into(),
            judge_feedback: None,
            round,
        }
    }

    pub fn new_with_judge_feedback(
        critique: impl Into<Cow<'a, str>>,
        initial_idea: impl Into<Cow<'a, str>>,
        judge_feedback: impl Into<Cow<'a, str>>,
        round: usize,
    ) -> Self {
        Self {
            critique: critique.into(),
            initial_idea: initial_idea.into(),
            judge_feedback: Some(judge_feedback.into()),
            round,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewSubmission<'a> {
    pub idea: Cow<'a, str>,
    pub statement: Option<Cow<'a, str>>,
    pub round: usize,
}

impl<'a> ReviewSubmission<'a> {
    pub fn new_without_statement(idea: impl Into<Cow<'a, str>>, round: usize) -> Self {
        Self {
            idea: idea.into(),
            statement: None,
            round,
        }
    }

    pub fn from_refinement(refinement: Refinement, round: usize) -> Self {
        Self {
            idea: Cow::Owned(refinement.refined_idea),
            statement: Some(Cow::Owned(refinement.statement)),
            round,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Ctor, ChainOutput, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct Refinement {
    /// Your statement about the critique and how to refine your idea.
    pub statement: String,
    /// Your refined idea based on the critique and reasoning. "no refine available" if the idea is
    /// unsalvageable.
    pub refined_idea: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Hash)]
#[serde(rename_all = "lowercase")]
pub enum JudgeDecision {
    Approve,
    Deny,
    Continue,
}

#[derive(Debug, Clone, Serialize, Deserialize, Ctor, ChainOutput, Hash)]
#[serde(deny_unknown_fields)]
#[chain(derive_schema)]
pub struct JudgeVerdict {
    /// Your decision regarding the refinement and idea.
    pub decision: JudgeDecision,
    /// Your reasoning behind the decision.
    pub reasoning: String,
}

pub type DefenderAgentOutputCtor = RefinementCtor;
pub type JudgeAgentOutputCtor = JudgeVerdictCtor;
