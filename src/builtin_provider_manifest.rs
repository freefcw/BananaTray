/// 内置 Provider 的单一编译期清单。
///
/// 每一项依次为：`ProviderKind` 变体、设置序列化 ID、provider 模块名、provider 类型名。
/// 新增内置 Provider 只需在此添加一行，并实现对应模块。
macro_rules! builtin_provider_manifest {
    ($macro:ident) => {
        $macro! {
            Claude => "claude" => claude::ClaudeProvider,
            Gemini => "gemini" => gemini::GeminiProvider,
            Copilot => "copilot" => copilot::CopilotProvider,
            Codex => "codex" => codex::CodexProvider,
            Kimi => "kimi" => kimi::KimiProvider,
            Amp => "amp" => amp::AmpProvider,
            Cursor => "cursor" => cursor::CursorProvider,
            OpenCode => "opencode" => opencode::OpenCodeProvider,
            MiniMax => "minimax" => minimax::MiniMaxProvider,
            VertexAi => "vertexai" => vertex_ai::VertexAiProvider,
            Kilo => "kilo" => kilo::KiloProvider,
            Kiro => "kiro" => kiro::KiroProvider,
            Antigravity => "antigravity" => antigravity::AntigravityProvider,
            Windsurf => "windsurf" => windsurf::WindsurfProvider,
        }
    };
}

pub(crate) use builtin_provider_manifest;
