mod compact_snapshot;
mod store;
mod tools;

pub use compact_snapshot::extract_snapshot;
pub use store::ContextStore;
pub use tools::{
    CtxExecuteTool, CtxRetrieveTool, CtxSearchTool, CtxStatsTool, auto_index_output, init_store,
    intent_filter, record_event,
};

pub mod event {
    pub const TOOL_CALL: &str = "tool_call";
    pub const TOOL_RESULT: &str = "tool_result";
    pub const COMMAND_FAIL: &str = "command_fail";
    #[allow(dead_code)]
    pub const FILE_WRITE: &str = "file_write";
    #[allow(dead_code)]
    pub const FILE_EDIT: &str = "file_edit";
    pub const FILE_READ: &str = "file_read";
    pub const COMPACTION: &str = "compaction";
    #[allow(dead_code)]
    pub const PERMISSION_DENY: &str = "permission_deny";

    pub const PRIORITY_CRITICAL: i32 = 1;
    pub const PRIORITY_HIGH: i32 = 2;
    pub const PRIORITY_LOW: i32 = 3;
}
