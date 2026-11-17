pub mod types;

use resm::codec::JsonCodec;
use resm::prelude::*;

use self::types::{JudgeDecision, JudgeVerdict, RefinementChallenge, ReviewSubmission};
use crate::agents::{
    DefenderAgent, DefenderAgentInput, JudgeAgent, JudgeAgentInput, ReviewerAgent,
    ReviewerAgentInput,
};
use crate::config::PipelineConfig;
use crate::types::{Objective, Refinement};
use crate::utils::ChatHistory;

impl<'a> std::fmt::Display for ReviewSubmission<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.round == 0 {
            write!(f, "Original research idea submission:\n{}", self.idea)
        } else {
            write!(f, "Revised research idea submission:\n{}", self.idea)?;

            if let Some(statement) = &self.statement {
                write!(
                    f,
                    "\n\nResearcher's justification and self-critique:\n{}",
                    statement
                )?;
            }
            Ok(())
        }
    }
}

impl<'a> std::fmt::Display for RefinementChallenge<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.round == 0 {
            write!(
                f,
                "The research idea to be refined:\n{}\n\nInitial critique from the reviewer:\n{}",
                self.initial_idea, self.critique
            )?;
        } else {
            write!(f, "Latest critique from the reviewer:\n{}", self.critique)?;
        }

        if let Some(judge_feedback) = &self.judge_feedback {
            write!(f, "\n\nJudge Feedback:\n{}", judge_feedback)?;
        }

        Ok(())
    }
}

#[tracing::instrument(skip_all, fields(max_rounds = config.refinement.max_rounds))]
pub async fn refine_idea(
    config: &PipelineConfig,
    initial_idea: &str,
    formulation: &str,
    objective: &Objective,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Option<String>> {
    let refinement_fp = (
        DefenderAgent.fingerprint(),
        ReviewerAgent.fingerprint(),
        JudgeAgent.fingerprint(),
    );

    let result: Option<String> = resm::cached_json!(
        store,
        "refinement_result",
        deps: &(refinement_fp, initial_idea, formulation, objective, &config.refinement),
        {
            run_refinement_loop(config, initial_idea, formulation, objective, store).await
        }
    )?;

    if let Some(result) = &result {
        store
            .save_text("refinement_result_text", result, &())
            .await?;
    }

    Ok(result)
}

async fn run_refinement_loop(
    config: &PipelineConfig,
    initial_idea: &str,
    formulation: &str,
    objective: &Objective,
    store: &(impl resm::Store + Clone),
) -> anyhow::Result<Option<String>> {
    // The defender, unlike the others, sees its earlier rounds.
    let defender = DefenderAgent::default().await;
    let mut defender_history = ChatHistory::default();

    let mut history: Vec<(
        ReviewSubmission<'_>,
        String,
        Refinement,
        Option<JudgeVerdict>,
    )> = vec![];

    let max_rounds = config.refinement.max_rounds;
    let warmup_rounds = config.refinement.warmup_rounds;

    let mut round = 1;

    let denial = loop {
        let is_warmup = round <= warmup_rounds;
        let is_finalization = round >= max_rounds;

        let submission = if let Some((_, _, refinement, _)) = history.last().cloned() {
            ReviewSubmission::from_refinement(refinement, round)
        } else {
            ReviewSubmission::new_without_statement(initial_idea, round)
        };

        let reviewer = ReviewerAgent::default().await;
        let critique =
            ChatHistory::run_once(&reviewer, ReviewerAgentInput::new(&submission)).await?;

        let challenge =
            if is_finalization && let Some((_, _, _, Some(judge_verdict))) = history.last() {
                // Only provide judge feedback on the finalization round
                RefinementChallenge::new_with_judge_feedback(
                    &critique,
                    initial_idea,
                    &judge_verdict.reasoning,
                    round,
                )
            } else {
                RefinementChallenge::new(&critique, initial_idea, round)
            };
        let refinement = defender_history
            .run(
                &defender,
                DefenderAgentInput::new(
                    &challenge,
                    &objective.core_goal,
                    &objective.key_constraints,
                    formulation,
                ),
            )
            .await?;

        if is_warmup || is_finalization {
            round += 1;
            history.push((submission, critique, refinement, None));

            if is_finalization {
                break false;
            }

            continue;
        }

        let judge = JudgeAgent::default().await;
        let judge_input = JudgeAgentInput::new(
            &refinement.refined_idea,
            &critique,
            &refinement.statement,
            &objective.key_constraints,
        );
        let judge_verdict =
            ChatHistory::run_once(&judge, judge_input)
                .await
                .unwrap_or(JudgeVerdict {
                    decision: JudgeDecision::Continue,
                    reasoning: "Judge failed to provide a valid verdict.".to_string(),
                });

        let decision = judge_verdict.decision.clone();
        history.push((submission, critique, refinement, Some(judge_verdict)));

        match decision {
            JudgeDecision::Approve => {
                // skip to last round
                round = max_rounds;
            }
            JudgeDecision::Deny => {
                // return denial immediately
                break true;
            }
            JudgeDecision::Continue => {
                // proceed to next round
                round += 1;
            }
        }
    };

    // Save audit history (only on cache miss — this runs inside the cached block)
    let codec = JsonCodec::pretty();
    store
        .save_with("refinement_history", &history, &(), &codec)
        .await
        .unwrap_or_else(|e| tracing::warn!(error = %e, "failed to save refinement history"));

    if denial {
        return Ok(None);
    }

    Ok(history
        .last()
        .map(|(_, _, refinement, _)| refinement.refined_idea.clone()))
}
