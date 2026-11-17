use talin::agent::{AgentRun, AgentSession, AnyAgentSession, SessionRun, StrategyFor};
use talin::chain::{InputCtor, OutputCtor, PartialWithInput};
use talin::llm::Llm;
use talin::schemas::{AssistantMessage, BodyMessage};
use talin::strategy::{DropThoughtStrategy, Strategy};
use talin::transcript::EntryView;

/// The user messages and final answers of earlier turns, which later turns run on top of.
///
/// Tool calls, tool results, and reasoning are not kept, so a later turn, even one run by another
/// agent, sees only the conversation itself.
///
/// Every agent call in the pipeline runs through [`ChatHistory::run`], which also drops thoughts
/// from the requests within a turn, for every agent and model type, as in the build used for the
/// paper runs.
#[derive(Debug, Clone, Default)]
pub struct ChatHistory(Vec<BodyMessage>);

impl ChatHistory {
    /// Runs one turn of `agent` after the recorded messages, then records the turn.
    pub async fn run<'i, A, I, O>(
        &mut self,
        agent: &A,
        input: I::Target<'i>,
    ) -> anyhow::Result<O::Target<'i>>
    where
        A: AgentRun<I, O>,
        A::Session: Conversation,
        I: InputCtor,
        O: OutputCtor,
        DropThoughtStrategy: StrategyFor<A::Session, I, O>,
        O::Partial: PartialWithInput<I::Target<'i>, O::Target<'i>>,
    {
        let mut session = agent.session();
        session.seed(&self.0)?;

        let output = session
            .run_turn_with_strategy(DropThoughtStrategy, input)
            .await?;

        self.0 = session.conversation()?;
        Ok(output.content)
    }

    /// Runs one turn of `agent` on an empty history.
    pub async fn run_once<'i, A, I, O>(
        agent: &A,
        input: I::Target<'i>,
    ) -> anyhow::Result<O::Target<'i>>
    where
        A: AgentRun<I, O>,
        A::Session: Conversation,
        I: InputCtor,
        O: OutputCtor,
        DropThoughtStrategy: StrategyFor<A::Session, I, O>,
        O::Partial: PartialWithInput<I::Target<'i>, O::Target<'i>>,
    {
        Self::default().run(agent, input).await
    }
}

/// A session whose transcript [`ChatHistory`] seeds before a turn and reads back after it.
pub trait Conversation {
    /// Appends `messages` to the transcript.
    fn seed(&mut self, messages: &[BodyMessage]) -> anyhow::Result<()>;

    /// The user messages and final answers in the transcript, without their thoughts.
    fn conversation(&self) -> anyhow::Result<Vec<BodyMessage>>;
}

impl<L, I, O, S> Conversation for AgentSession<L, I, O, S>
where
    L: Llm,
    I: InputCtor,
    O: OutputCtor,
    S: Strategy<L, O> + Clone,
{
    fn seed(&mut self, messages: &[BodyMessage]) -> anyhow::Result<()> {
        for message in messages {
            self.transcript.push_message(message.clone())?;
        }
        Ok(())
    }

    fn conversation(&self) -> anyhow::Result<Vec<BodyMessage>> {
        let messages = self
            .transcript
            .full_view()
            .into_iter()
            .filter_map(|entry| match entry {
                EntryView::Message(_, BodyMessage::User(message)) => {
                    Some(BodyMessage::User(message.clone()))
                }
                EntryView::Message(_, BodyMessage::Assistant(message)) => {
                    Some(BodyMessage::Assistant(AssistantMessage {
                        thought: None,
                        content: message.content.clone(),
                    }))
                }
                _ => None,
            })
            .collect();
        Ok(messages)
    }
}

/// Forwards to the [`AgentSession`] inside each [`AnyAgentSession`] variant.
macro_rules! on_session {
    ($session:expr, $inner:ident => $body:expr) => {
        match $session {
            AnyAgentSession::GenericChat($inner) => $body,
            AnyAgentSession::OpenAIChat($inner) => $body,
            AnyAgentSession::OpenAIResponses($inner) => $body,
            AnyAgentSession::ClaudeMessages($inner) => $body,
            AnyAgentSession::GeminiGenerateContent($inner) => $body,
            AnyAgentSession::GeminiInteractions($inner) => $body,
            AnyAgentSession::XaiResponses($inner) => $body,
            AnyAgentSession::SglangGenerate($inner) => $body,
            other => anyhow::bail!(
                "ChatHistory does not support the {} backend",
                other.backend()
            ),
        }
    };
}

impl<I, O> Conversation for AnyAgentSession<I, O>
where
    I: InputCtor,
    O: OutputCtor,
{
    fn seed(&mut self, messages: &[BodyMessage]) -> anyhow::Result<()> {
        on_session!(self, session => session.seed(messages))
    }

    fn conversation(&self) -> anyhow::Result<Vec<BodyMessage>> {
        on_session!(self, session => session.conversation())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use talin::llm::{GenericChat, LlmPreset, OpenAIChat};

    use super::ChatHistory;
    use crate::agents::GoalSimplifierAgent;

    /// The `(role, text)` pairs of a chat completions request.
    fn messages(request: &Value) -> Vec<(String, String)> {
        request["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|m| {
                let text = match &m["content"] {
                    Value::String(text) => text.clone(),
                    Value::Array(parts) => {
                        parts.iter().filter_map(|p| p["text"].as_str()).collect()
                    }
                    other => panic!("unexpected content {other}"),
                };
                (m["role"].as_str().expect("role").to_owned(), text)
            })
            .collect()
    }

    #[tokio::test]
    async fn later_turn_sees_earlier_user_messages_and_answers() {
        let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut server = mockito::Server::new_async().await;
        let recorded = Arc::clone(&requests);
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_header("content-type", "application/json")
            .with_body_from_request(move |req| {
                let body = serde_json::from_slice(req.body().unwrap()).unwrap();
                let mut recorded = recorded.lock().unwrap();
                recorded.push(body);
                let answer = format!("answer {}", recorded.len());
                serde_json::to_vec(&json!({
                    "id": "chatcmpl-0",
                    "object": "chat.completion",
                    "created": 0,
                    "model": "fake",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": answer },
                        "finish_reason": "stop"
                    }],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                }))
                .unwrap()
            })
            .expect(2)
            .create_async()
            .await;

        let model = OpenAIChat::builder()
            .with_api_base(server.url())
            .with_api_key("fake-key")
            .with_model("fake")
            .build();
        let agent = GoalSimplifierAgent::new(LlmPreset::new(model, ())).await;

        let mut history = ChatHistory::default();
        let first = history.run(&agent, "goal one".into()).await.unwrap();
        let second = history.run(&agent, "goal two".into()).await.unwrap();
        assert_eq!((first.as_str(), second.as_str()), ("answer 1", "answer 2"));

        let requests = requests.lock().unwrap();
        let first = messages(&requests[0]);
        let second = messages(&requests[1]);
        assert_eq!(
            first
                .iter()
                .map(|(role, _)| role.as_str())
                .collect::<Vec<_>>(),
            ["system", "user"]
        );
        assert_eq!(
            second,
            [
                first[0].clone(),
                first[1].clone(),
                ("assistant".to_owned(), "answer 1".to_owned()),
                (
                    "user".to_owned(),
                    first[1].1.replace("goal one", "goal two")
                ),
            ]
        );
    }

    #[tokio::test]
    async fn drops_thoughts_within_a_turn() {
        const THOUGHT: &str = "I should look the goal up before simplifying it.";

        let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
        let mut server = mockito::Server::new_async().await;
        let recorded = Arc::clone(&requests);
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_header("content-type", "application/json")
            .with_body_from_request(move |req| {
                let body = serde_json::from_slice(req.body().unwrap()).unwrap();
                let mut recorded = recorded.lock().unwrap();
                recorded.push(body);
                // Each turn calls a tool first, then answers.
                let content = if recorded.len() % 2 == 1 {
                    format!(
                        "{THOUGHT}\n```json\n{{\"action\": \"lookup\", \"action_input\": {{}}}}\n```"
                    )
                } else {
                    "simplified".to_owned()
                };
                serde_json::to_vec(&json!({
                    "id": "chatcmpl-0",
                    "object": "chat.completion",
                    "created": 0,
                    "model": "fake",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": content },
                        "finish_reason": "stop"
                    }],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                }))
                .unwrap()
            })
            .expect(4)
            .create_async()
            .await;

        let model = GenericChat::builder()
            .with_api_base(server.url())
            .with_api_key("fake-key")
            .with_model("fake")
            .build();
        let agent = GoalSimplifierAgent::new(LlmPreset::new(model, ())).await;

        // The agent's own strategy keeps thoughts, so this turn's second request shows the thought
        // would otherwise reach the model.
        let kept = agent.run("goal".into()).await.unwrap();
        let dropped = ChatHistory::run_once(&agent, "goal".into()).await.unwrap();
        assert_eq!(
            (kept.as_str(), dropped.as_str()),
            ("simplified", "simplified")
        );

        let requests = requests.lock().unwrap();
        assert!(requests[1].to_string().contains(THOUGHT));
        assert!(!requests[3].to_string().contains(THOUGHT));
    }
}
