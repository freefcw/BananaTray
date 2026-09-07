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
            ClinePass => "cline-pass" => cline_pass::ClinePassProvider,
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
            Grok => "grok" => grok::GrokProvider,
        }
    };
}

pub(crate) use builtin_provider_manifest;

#[cfg(test)]
mod tests {
    use crate::application::display_source_label;
    use crate::models::test_helpers::setup_test_locale;
    use crate::models::ProviderKind;
    use crate::providers::ProviderManager;

    /// 加内置 Provider 时，`source_label` 要在 selector 的文案表里补一条，
    /// 漏了不会编译失败——设置页副标题会静默显示成「自定义」。Grok 就这样漏过一次。
    #[test]
    fn every_builtin_source_label_has_its_own_translation() {
        let _locale_guard = setup_test_locale();
        let manager = ProviderManager::new();
        let custom_fallback = display_source_label("no-such-source-label");
        let cli_fallback = display_source_label("no-such cli");

        for kind in ProviderKind::all() {
            let raw = manager.metadata_for(*kind).source_label;
            let label = display_source_label(&raw);
            assert!(
                label != custom_fallback && label != cli_fallback,
                "{kind:?} 的 source_label {raw:?} 未收录，会回落到通用文案 {label:?}"
            );
        }
    }
}
