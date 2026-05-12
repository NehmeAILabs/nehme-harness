use compact_str::CompactString;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub type AskSender = mpsc::Sender<AskRequest>;
pub type AskReceiver = mpsc::Receiver<AskRequest>;

#[derive(Debug)]
pub struct AskRequest {
    pub tool: CompactString,
    pub input: String,
    pub reply: oneshot::Sender<UserDecision>,
}

#[derive(Debug, Clone)]
pub enum UserDecision {
    AllowOnce,
    AllowAlways(String),
    Deny,
}
