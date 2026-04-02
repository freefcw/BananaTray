pub(super) fn get_api_key() -> Option<String> {
    std::env::var("MINIMAX_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

pub(super) fn api_url() -> &'static str {
    let region = std::env::var("MINIMAX_REGION").unwrap_or_default();
    match region.to_lowercase().as_str() {
        "international" => "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
        _ => "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url_default() {
        std::env::remove_var("MINIMAX_REGION");
        assert_eq!(
            api_url(),
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains"
        );
    }
}
