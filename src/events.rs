use uuid::Uuid;

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
    Completed {
        run_id: Uuid,
        conversation_id: Uuid,
    },
    Failed {
        run_id: Uuid,
        conversation_id: Uuid,
        message: String,
    },
}
