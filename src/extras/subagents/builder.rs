use crate::agent::tools;
use crate::extras::subagents::prompt;
use crate::provider::{AnyAgent, AnyModel, OpenAiAgent, OpenAiModel};
use rig::agent::{Agent, AgentBuilder};
use rig::completion::CompletionModel;

fn build_explore_agent_inner<M: CompletionModel + 'static>(
    model: M,
    max_turns: usize,
    max_text_file_size: u64,
    max_read_lines: u64,
    max_grep_results: u64,
    max_find_results: u64,
    max_list_dir_entries: Option<u64>,
    additional_params: Option<serde_json::Value>,
    session_id: String,
) -> Agent<M> {
    let preamble = prompt::explore_prompt();

    let mut tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![
        Box::new(tools::ReadTool::new(
            None,
            None,
            Some(max_text_file_size),
            max_read_lines,
            session_id.clone(),
        )),
        Box::new(tools::GrepTool::new(None, None, max_grep_results)),
        Box::new(tools::FindFilesTool::new(None, None, max_find_results)),
        Box::new(tools::ListDirTool::new(None, None, max_list_dir_entries)),
        #[cfg(feature = "memory")]
        Box::new(crate::extras::memory::MemoryRead::new(None, None)),
        #[cfg(feature = "memory")]
        Box::new(crate::extras::memory::MemorySearch::new(None, None)),
    ];

    #[cfg(feature = "context")]
    tools.extend([
        Box::new(crate::extras::context::CtxSearchTool::new(
            None,
            None,
            session_id.clone(),
        )) as Box<dyn rig::tool::ToolDyn>,
        Box::new(crate::extras::context::CtxExecuteTool::new(
            None,
            None,
            session_id.clone(),
        )) as Box<dyn rig::tool::ToolDyn>,
    ]);

    let mut builder = AgentBuilder::new(model)
        .preamble(&preamble)
        .default_max_turns(max_turns)
        .tools(tools);

    if let Some(params) = additional_params {
        builder = builder.additional_params(params);
    }

    builder.build()
}

pub(crate) async fn build_explore_agent(
    model: AnyModel,
    max_turns: usize,
    cfg: &crate::config::Config,
    session_id: String,
) -> AnyAgent {
    let max_text_file_size = cfg.max_text_file_size.unwrap_or(10 * 1024 * 1024);
    let max_read_lines = cfg.resolve_subagent_max_read_lines();
    let max_grep_results = cfg.resolve_subagent_max_grep_results();
    let max_find_results = cfg.resolve_subagent_max_find_results();
    let max_list_dir_entries = cfg.resolve_subagent_max_list_dir_entries();
    match model {
        AnyModel::OpenRouter(m, extra) => AnyAgent::OpenRouter(build_explore_agent_inner(
            m,
            max_turns,
            max_text_file_size,
            max_read_lines,
            max_grep_results,
            max_find_results,
            max_list_dir_entries,
            extra,
            session_id,
        )),
        AnyModel::OpenAI(m) => AnyAgent::OpenAI(match m {
            OpenAiModel::Responses(m) => OpenAiAgent::Responses(build_explore_agent_inner(
                m,
                max_turns,
                max_text_file_size,
                max_read_lines,
                max_grep_results,
                max_find_results,
                max_list_dir_entries,
                None,
                session_id,
            )),
            OpenAiModel::Completions(m) => OpenAiAgent::Completions(build_explore_agent_inner(
                m,
                max_turns,
                max_text_file_size,
                max_read_lines,
                max_grep_results,
                max_find_results,
                max_list_dir_entries,
                None,
                session_id,
            )),
        }),
        AnyModel::Anthropic(m) => AnyAgent::Anthropic(build_explore_agent_inner(
            m,
            max_turns,
            max_text_file_size,
            max_read_lines,
            max_grep_results,
            max_find_results,
            max_list_dir_entries,
            None,
            session_id,
        )),
        AnyModel::Gemini(m) => AnyAgent::Gemini(build_explore_agent_inner(
            m,
            max_turns,
            max_text_file_size,
            max_read_lines,
            max_grep_results,
            max_find_results,
            max_list_dir_entries,
            None,
            session_id,
        )),
        AnyModel::Ollama(m) => AnyAgent::Ollama(build_explore_agent_inner(
            m,
            max_turns,
            max_text_file_size,
            max_read_lines,
            max_grep_results,
            max_find_results,
            max_list_dir_entries,
            None,
            session_id,
        )),
        AnyModel::DeepSeek(m) => AnyAgent::DeepSeek(build_explore_agent_inner(
            m,
            max_turns,
            max_text_file_size,
            max_read_lines,
            max_grep_results,
            max_find_results,
            max_list_dir_entries,
            None,
            session_id,
        )),
    }
}
