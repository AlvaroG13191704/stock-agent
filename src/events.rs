use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }

    pub fn add_assign(&mut self, other: &Self) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
    }
}

#[derive(Debug, Clone)]
pub enum RunEvent {
    Started {
        run_id: Uuid,
        conversation_id: Uuid,
    },
    Trace {
        run_id: Uuid,
        message: String,
    },
    Usage {
        run_id: Uuid,
        usage: TokenUsage,
    },
    Stage {
        run_id: Uuid,
        agent: String,
        current: usize,
        total: usize,
    },
    Completed {
        run_id: Uuid,
        conversation_id: Uuid,
    },
    Failed {
        run_id: Uuid,
        conversation_id: Uuid,
        message: String,
        retryable: bool,
    },
}
