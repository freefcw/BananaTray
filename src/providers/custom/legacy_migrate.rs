//! 旧版自定义 Provider YAML（顶层 source/parser）→ schema_version 2。
//!
//! 规则与 `scripts/migrate_custom_provider_yaml.py` 对齐：fail-closed、
//! 尽量保留原文块，不猜测未知结构。运行时仍只解释 v2 `plan.steps`。

use std::collections::{BTreeSet, HashMap, HashSet};

use regex::Regex;
use std::sync::LazyLock;

static TOP_LEVEL_KEYS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "id",
        "schema_version",
        "base_url",
        "metadata",
        "availability",
        "source",
        "parser",
        "preprocess",
        "plan",
    ])
});

const MOVABLE_KEYS: [&str; 4] = ["availability", "source", "parser", "preprocess"];

type YamlBlock = Vec<String>;
type NamedBlock = (Option<String>, YamlBlock);
type MappingField = (usize, String, String);

static TOP_LEVEL_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^(?:"(?P<double>[^"]+)"|'(?P<single>[^']+)'|(?P<plain>[A-Za-z_][A-Za-z0-9_-]*))\s*:"#,
    )
    .expect("top-level key pattern")
});

static MAPPING_ENTRY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^(?P<indent> *)(?:"(?P<double>[^"]+)"|'(?P<single>[^']+)'|(?P<plain>[A-Za-z_][A-Za-z0-9_-]*))\s*:(?P<value>.*)$"#,
    )
    .expect("mapping entry pattern")
});

static SCHEMA_VERSION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^schema_version\s*:\s*(?P<version>\d+)\s*(?:#.*)?$")
        .expect("schema version pattern")
});

static SIMPLE_SCALAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^\s*(?:'(?P<single>[A-Za-z_][A-Za-z0-9_-]*)'|"(?P<double>[A-Za-z_][A-Za-z0-9_-]*)"|(?P<plain>[A-Za-z_][A-Za-z0-9_-]*))\s*(?:#.*)?$"#,
    )
    .expect("simple scalar pattern")
});

static LEGACY_HTTP_TYPE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^(?P<indent>\s*)type\s*:\s*(?:'(?P<single>http_get|http_post)'|"(?P<double>http_get|http_post)"|(?P<plain>http_get|http_post))(?:(?P<comment_space>[ \t]+)(?P<comment>#.*))?$"#,
    )
    .expect("legacy http type pattern")
});

static LEGACY_HTTP_TYPE_TOKEN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bhttp_(?:get|post)\b").expect("legacy http type token"));

#[derive(Debug)]
pub(super) struct MigrationError(String);

impl MigrationError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for MigrationError {}

/// 将旧 YAML 转为 schema_version 2。第二个返回值表示内容是否变化。
pub(super) fn migrate_text(text: &str) -> Result<(String, bool), MigrationError> {
    let newline = preferred_newline(text);
    let lines: Vec<String> = split_keep_ends(text);
    let blocks = split_blocks(&lines)?;
    validate_top_level_keys(&blocks)?;

    let mut by_key: HashMap<&str, &[String]> = HashMap::new();
    for (key, block) in &blocks {
        if let Some(key) = key.as_deref() {
            by_key.insert(key, block.as_slice());
        }
    }
    let movable_keys: BTreeSet<&str> = MOVABLE_KEYS
        .into_iter()
        .filter(|key| by_key.contains_key(key))
        .collect();

    let version = if let Some(block) = by_key.get("schema_version") {
        Some(schema_version(block)?)
    } else {
        None
    };
    if matches!(version, Some(v) if v != 1 && v != 2) {
        return Err(MigrationError::new(format!(
            "unsupported legacy schema_version: {}",
            version.unwrap()
        )));
    }

    let missing_required: Vec<&str> = ["id", "metadata"]
        .into_iter()
        .filter(|key| !by_key.contains_key(key))
        .collect();
    if !missing_required.is_empty() {
        return Err(MigrationError::new(format!(
            "provider is missing required top-level key(s): {}",
            missing_required.join(", ")
        )));
    }

    if by_key.contains_key("plan") {
        if !movable_keys.is_empty() {
            let fields = movable_keys.into_iter().collect::<Vec<_>>().join(", ");
            return Err(MigrationError::new(format!(
                "plan already exists alongside legacy field(s): {fields}"
            )));
        }
        if version != Some(2) {
            return Err(MigrationError::new(
                "existing plan requires exactly schema_version: 2",
            ));
        }
        validate_plan_block(by_key["plan"])?;
        return Ok((text.to_string(), false));
    }

    if !by_key.contains_key("source") {
        if !movable_keys.is_empty() {
            let fields = movable_keys.into_iter().collect::<Vec<_>>().join(", ");
            return Err(MigrationError::new(format!(
                "legacy field(s) require a top-level source: {fields}"
            )));
        }
        return Err(MigrationError::new(
            "custom provider requires a top-level source or plan",
        ));
    }

    let (_, source_kind) = source_type(by_key["source"])?;
    if source_kind != "placeholder" && !by_key.contains_key("parser") {
        return Err(MigrationError::new(
            "non-placeholder legacy source requires a top-level parser",
        ));
    }

    let mut output_blocks: Vec<NamedBlock> = Vec::new();
    for (key, block) in &blocks {
        if key
            .as_deref()
            .is_some_and(|key| MOVABLE_KEYS.contains(&key))
        {
            continue;
        }
        let mut block = block.clone();
        if key.as_deref() == Some("schema_version") {
            block = replace_schema_version(&block, newline);
        }
        output_blocks.push((key.clone(), ensure_block_terminated(block, newline)));
        if key.as_deref() == Some("id") && version.is_none() {
            output_blocks.push((
                Some("schema_version".to_string()),
                vec![format!("schema_version: 2{newline}")],
            ));
        }
    }

    let mut step_lines = vec![
        format!("plan:{newline}"),
        format!("  mode: first_success{newline}"),
        format!("  steps:{newline}"),
        format!("    - name: \"default\"{newline}"),
        format!("      required: true{newline}"),
    ];
    for key in ["availability", "source", "preprocess", "parser"] {
        let Some(block) = by_key.get(key) else {
            continue;
        };
        let block = if key == "source" {
            migrate_source_block(block, newline)?
        } else {
            block.to_vec()
        };
        step_lines.extend(indent_block(&ensure_block_terminated(block, newline), 6));
    }

    output_blocks.push((Some("plan".to_string()), step_lines));
    let migrated: String = output_blocks
        .into_iter()
        .flat_map(|(_, block)| block)
        .collect();
    let changed = migrated != text;
    Ok((migrated, changed))
}

fn split_keep_ends(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(index) = rest.find(['\n', '\r']) {
            let ending_len = if rest[index..].starts_with("\r\n") {
                2
            } else {
                1
            };
            let (line, next) = rest.split_at(index + ending_len);
            lines.push(line.to_string());
            rest = next;
        } else {
            lines.push(rest.to_string());
            break;
        }
    }
    lines
}

fn top_level_key(line: &str) -> Result<Option<String>, MigrationError> {
    if line.is_empty() || line.starts_with([' ', '\t']) {
        return Ok(None);
    }
    let stripped = line_content(line).trim();
    if stripped.is_empty() || stripped.starts_with('#') || matches!(stripped, "---" | "...") {
        return Ok(None);
    }
    let Some(captures) = TOP_LEVEL_KEY_PATTERN.captures(line) else {
        return Err(MigrationError::new(format!(
            "unsupported top-level YAML syntax: {stripped:?}"
        )));
    };
    Ok(named_key(&captures))
}

fn split_blocks(lines: &[String]) -> Result<Vec<NamedBlock>, MigrationError> {
    let mut blocks = Vec::new();
    let mut current_key = None;
    let mut current = Vec::new();

    for line in lines {
        if let Some(key) = top_level_key(line)? {
            if !current.is_empty() {
                blocks.push((current_key, current));
            }
            current_key = Some(key);
            current = vec![line.clone()];
        } else {
            current.push(line.clone());
        }
    }
    if !current.is_empty() {
        blocks.push((current_key, current));
    }
    Ok(blocks)
}

fn validate_top_level_keys(blocks: &[NamedBlock]) -> Result<(), MigrationError> {
    let keys: Vec<&str> = blocks
        .iter()
        .filter_map(|(key, _)| key.as_deref())
        .collect();
    let mut seen = HashSet::new();
    let mut duplicates = BTreeSet::new();
    for key in &keys {
        if !seen.insert(*key) {
            duplicates.insert(*key);
        }
    }
    if !duplicates.is_empty() {
        return Err(MigrationError::new(format!(
            "duplicate top-level key(s): {}",
            duplicates.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }

    let unknown: BTreeSet<&str> = keys
        .into_iter()
        .filter(|key| !TOP_LEVEL_KEYS.contains(key))
        .collect();
    if !unknown.is_empty() {
        return Err(MigrationError::new(format!(
            "unknown top-level key(s): {}",
            unknown.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    Ok(())
}

fn schema_version(block: &[String]) -> Result<u32, MigrationError> {
    let first_line = line_content(&block[0]);
    let Some(captures) = SCHEMA_VERSION_PATTERN.captures(first_line) else {
        return Err(MigrationError::new(
            "schema_version must be an unquoted integer",
        ));
    };
    captures
        .name("version")
        .and_then(|value| value.as_str().parse().ok())
        .ok_or_else(|| MigrationError::new("schema_version must be an unquoted integer"))
}

fn line_ending(line: &str) -> &str {
    if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else if line.ends_with('\r') {
        "\r"
    } else {
        ""
    }
}

fn preferred_newline(text: &str) -> &str {
    for line in split_keep_ends(text) {
        let ending = line_ending(&line);
        if !ending.is_empty() {
            return match ending {
                "\r\n" => "\r\n",
                "\r" => "\r",
                _ => "\n",
            };
        }
    }
    "\n"
}

fn ensure_block_terminated(mut block: Vec<String>, newline: &str) -> Vec<String> {
    let needs_newline = block
        .last()
        .is_some_and(|last| !last.ends_with('\n') && !last.ends_with('\r'));
    if needs_newline {
        let last = format!("{}{newline}", block.last().unwrap());
        *block.last_mut().unwrap() = last;
    }
    block
}

fn replace_schema_version(block: &[String], default_newline: &str) -> Vec<String> {
    let newline = {
        let ending = line_ending(&block[0]);
        if ending.is_empty() {
            default_newline
        } else {
            ending
        }
    };
    let mut migrated = vec![format!("schema_version: 2{newline}")];
    migrated.extend(block.iter().skip(1).cloned());
    migrated
}

fn indent_block(block: &[String], spaces: usize) -> Vec<String> {
    let prefix = " ".repeat(spaces);
    block
        .iter()
        .map(|line| {
            if line_content(line).trim().is_empty() {
                line.clone()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect()
}

fn line_content(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn named_key(captures: &regex::Captures<'_>) -> Option<String> {
    ["double", "single", "plain"]
        .into_iter()
        .find_map(|name| captures.name(name).map(|value| value.as_str().to_string()))
}

fn mapping_entry(line: &str) -> Option<MappingField> {
    let captures = MAPPING_ENTRY_PATTERN.captures(line_content(line))?;
    let indent = captures
        .name("indent")
        .map_or(0, |value| value.as_str().len());
    let key = named_key(&captures)?;
    let value = captures
        .name("value")
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    Some((indent, key, value))
}

fn indentation(line: &str, context: &str) -> Result<usize, MigrationError> {
    let content = line_content(line);
    let prefix_len = content.len() - content.trim_start_matches([' ', '\t']).len();
    let prefix = &content[..prefix_len];
    if prefix.contains('\t') {
        return Err(MigrationError::new(format!(
            "{context} uses unsupported tab indentation"
        )));
    }
    Ok(prefix.len())
}

fn direct_mapping_entries(
    block: &[String],
    block_name: &str,
) -> Result<(usize, Vec<MappingField>), MigrationError> {
    let header =
        mapping_entry(&block[0]).filter(|(indent, key, _)| *indent == 0 && key == block_name);
    let Some((_, _, header_value)) = header else {
        return Err(MigrationError::new(format!(
            "cannot determine top-level {block_name} mapping"
        )));
    };
    if !header_value.trim().is_empty() && !header_value.trim_start().starts_with('#') {
        return Err(MigrationError::new(format!(
            "top-level {block_name} must use block mapping syntax"
        )));
    }

    let mut structural_lines = Vec::new();
    for (offset, line) in block.iter().enumerate().skip(1) {
        let content = line_content(line);
        let stripped = content.trim();
        if stripped.is_empty() || stripped.starts_with('#') {
            continue;
        }
        structural_lines.push((offset, indentation(line, block_name)?, content.to_string()));
    }
    if structural_lines.is_empty() {
        return Err(MigrationError::new(format!(
            "top-level {block_name} mapping is empty"
        )));
    }

    let direct_indent = structural_lines
        .iter()
        .map(|(_, indent, _)| *indent)
        .min()
        .unwrap_or(0);
    if direct_indent == 0 {
        return Err(MigrationError::new(format!(
            "cannot determine direct {block_name} fields"
        )));
    }

    let mut entries = Vec::new();
    for (index, indent, _) in &structural_lines {
        if *indent != direct_indent {
            continue;
        }
        let Some((_, key, value)) = mapping_entry(&block[*index]) else {
            return Err(MigrationError::new(format!(
                "cannot determine direct {block_name} fields"
            )));
        };
        entries.push((*index, key, value));
    }

    let mut seen = HashSet::new();
    let mut duplicates = BTreeSet::new();
    for (_, key, _) in &entries {
        if !seen.insert(key.clone()) {
            duplicates.insert(key.clone());
        }
    }
    if !duplicates.is_empty() {
        let fields = duplicates
            .into_iter()
            .map(|key| format!("{block_name}.{key}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(MigrationError::new(format!(
            "duplicate direct field(s): {fields}"
        )));
    }
    Ok((direct_indent, entries))
}

fn source_type(block: &[String]) -> Result<(usize, String), MigrationError> {
    let (_, entries) = direct_mapping_entries(block, "source")?;
    let candidates: Vec<(usize, String)> = entries
        .into_iter()
        .filter(|(_, key, _)| key == "type")
        .map(|(index, _, value)| (index, value))
        .collect();
    if candidates.len() != 1 {
        return Err(MigrationError::new(
            "source.type must be one unambiguous direct field",
        ));
    }
    let (index, value) = candidates.into_iter().next().unwrap();
    let Some(captures) = SIMPLE_SCALAR_PATTERN.captures(&value) else {
        return Err(MigrationError::new("source.type must be a simple scalar"));
    };
    Ok((index, named_key(&captures).unwrap_or_default()))
}

fn migrate_source_block(
    block: &[String],
    default_newline: &str,
) -> Result<Vec<String>, MigrationError> {
    let (type_index, source_kind) = source_type(block)?;
    if source_kind != "http_get" && source_kind != "http_post" {
        return Ok(block.to_vec());
    }

    let (_, entries) = direct_mapping_entries(block, "source")?;
    if entries.iter().any(|(_, key, _)| key == "method") {
        return Err(MigrationError::new(
            "legacy source.type cannot be combined with source.method",
        ));
    }

    let line = &block[type_index];
    let newline = line_ending(line);
    let content = if newline.is_empty() {
        line.as_str()
    } else {
        &line[..line.len() - newline.len()]
    };
    let Some(captures) = LEGACY_HTTP_TYPE_PATTERN.captures(content) else {
        let stripped = content.trim();
        if LEGACY_HTTP_TYPE_TOKEN_PATTERN.is_match(stripped) {
            return Err(MigrationError::new(format!(
                "unsupported legacy source.type syntax: {stripped:?}"
            )));
        }
        return Err(MigrationError::new("cannot determine legacy source.type"));
    };

    let line_indent = captures.name("indent").map_or("", |value| value.as_str());
    let source_kind = named_key(&captures).unwrap_or(source_kind);
    let method = if source_kind == "http_get" {
        "get"
    } else {
        "post"
    };
    let comment_suffix = captures
        .name("comment")
        .map(|value| format!(" {}", value.as_str()))
        .unwrap_or_default();
    let output_newline = if newline.is_empty() {
        default_newline
    } else {
        newline
    };
    let replacement = [
        format!("{line_indent}type: http{comment_suffix}{output_newline}"),
        format!("{line_indent}method: {method}{output_newline}"),
    ];
    let mut migrated = block[..type_index].to_vec();
    migrated.extend(replacement);
    migrated.extend(block[type_index + 1..].iter().cloned());
    Ok(migrated)
}

fn validate_plan_block(block: &[String]) -> Result<(), MigrationError> {
    let (direct_indent, entries) = direct_mapping_entries(block, "plan")?;
    let steps: Vec<(usize, String)> = entries
        .into_iter()
        .filter(|(_, key, _)| key == "steps")
        .map(|(index, _, value)| (index, value))
        .collect();
    if steps.len() != 1 {
        return Err(MigrationError::new(
            "plan.steps must be one non-empty block sequence",
        ));
    }
    let (steps_index, value) = steps.into_iter().next().unwrap();
    if !value.trim().is_empty() && !value.trim_start().starts_with('#') {
        return Err(MigrationError::new(
            "plan.steps must be a non-empty block sequence",
        ));
    }

    let mut step_lines = Vec::new();
    for line in &block[steps_index + 1..] {
        let content = line_content(line);
        let stripped = content.trim();
        if stripped.is_empty() || stripped.starts_with('#') {
            continue;
        }
        let line_indent = indentation(line, "plan.steps")?;
        if line_indent <= direct_indent {
            break;
        }
        step_lines.push((line_indent, stripped.to_string()));
    }
    if step_lines.is_empty() {
        return Err(MigrationError::new(
            "plan.steps must be a non-empty block sequence",
        ));
    }

    let item_indent = step_lines
        .iter()
        .map(|(indent, _)| *indent)
        .min()
        .unwrap_or(0);
    let direct_items: Vec<&str> = step_lines
        .iter()
        .filter(|(indent, _)| *indent == item_indent)
        .map(|(_, text)| text.as_str())
        .collect();
    if direct_items.is_empty()
        || direct_items
            .iter()
            .any(|text| *text != "-" && !text.starts_with("- "))
    {
        return Err(MigrationError::new(
            "plan.steps must be a non-empty block sequence",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_YAML: &str = r#"id: "legacy:http"
schema_version: 1
base_url: "https://example.com"
metadata:
  display_name: "Legacy"
  brand_name: "Legacy"
source:
  type: http_get
  url: "/api/usage"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\d+)/(\d+)'
"#;

    const NEWAPI_LEGACY_YAML: &str = r#"# 自动生成的 NewAPI 中转站配置
# 由 BananaTray 快速添加向导创建

id: "anyrouter-top:newapi"

base_url: "https://anyrouter.top"

metadata:
  display_name: "AnyRouter"
  brand_name: "NewAPI Relay"
  dashboard_url: "/"
  account_hint: "NewAPI account"
  source_label: "newapi api"

availability:
  type: always

source:
  type: http_get
  url: "/api/user/self"
  auth:
    type: cookie
    value: "session=test-token"
  headers:
    - name: "New-Api-User"
      value: "3301"

parser:
  format: json
  account_email: "data.display_name"
  quotas:
    - label: "Balance"
      remaining: "data.quota"
      used: "data.used_quota"
      quota_type: credit
      divisor: 500000
"#;

    const V2_YAML: &str = r#"id: "current"
schema_version: 2
metadata:
  display_name: "Current"
  brand_name: "Current"
plan:
  mode: first_success
  steps:
    - name: "default"
      required: true
      source:
        type: placeholder
        reason: "Not configured"
"#;

    #[test]
    fn migrate_text_rewrites_schema_version_and_http_get() {
        let (migrated, changed) = migrate_text(LEGACY_YAML).unwrap();

        assert!(changed);
        assert_eq!(migrated.matches("schema_version:").count(), 1);
        assert_eq!(migrated.matches("plan:").count(), 1);
        assert!(migrated.contains("schema_version: 2"));
        assert!(migrated.contains("      source:\n        type: http\n        method: get"));
    }

    #[test]
    fn migrate_text_rewrites_generated_newapi_legacy_yaml() {
        let (migrated, changed) = migrate_text(NEWAPI_LEGACY_YAML).unwrap();

        assert!(changed);
        assert!(migrated.contains("schema_version: 2"));
        assert!(migrated.contains("plan:"));
        assert!(migrated.contains("      availability:\n        type: always"));
        assert!(migrated.contains("        type: http\n        method: get"));
        assert!(migrated.contains("        value: \"session=test-token\""));
        assert!(!migrated.contains("\navailability:"));
        assert!(!migrated.contains("http_get"));
    }

    #[test]
    fn migrate_text_rejects_plan_combined_with_legacy_fields() {
        let text = format!("{LEGACY_YAML}plan:\n  steps: []\n");
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("plan"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_unknown_top_level_field() {
        let text = LEGACY_YAML.replacen("source:\n", "custom_timeout: 42\nsource:\n", 1);
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("custom_timeout"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_duplicate_schema_version() {
        let text = LEGACY_YAML.replacen(
            "schema_version: 1\n",
            "schema_version: 1\nschema_version: 2\n",
            1,
        );
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_future_schema_version() {
        let text = LEGACY_YAML.replacen("schema_version: 1", "schema_version: 3", 1);
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("schema_version: 3"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_incomplete_v1_without_source() {
        let text = r#"id: "incomplete"
schema_version: 1
metadata:
  display_name: "Incomplete"
"#;
        let err = migrate_text(text).unwrap_err();
        assert!(err.to_string().contains("source"), "got: {err}");
    }

    #[test]
    fn migrate_text_migrates_quoted_legacy_http_type() {
        let text = LEGACY_YAML.replacen("type: http_get", r#"type: "http_get""#, 1);
        let (migrated, changed) = migrate_text(&text).unwrap();
        assert!(changed);
        assert!(migrated.contains("        type: http\n        method: get"));
        assert!(!migrated.contains("http_get"));
    }

    #[test]
    fn migrate_text_migrates_commented_legacy_http_type() {
        let text = LEGACY_YAML.replacen("type: http_get", "type: http_get # legacy endpoint", 1);
        let (migrated, changed) = migrate_text(&text).unwrap();
        assert!(changed);
        assert!(migrated.contains("        type: http # legacy endpoint\n        method: get"));
        assert!(!migrated.contains("http_get"));
    }

    #[test]
    fn migrate_text_migrates_legacy_http_post_type() {
        let text = LEGACY_YAML.replacen("type: http_get", "type: http_post", 1);
        let (migrated, changed) = migrate_text(&text).unwrap();
        assert!(changed);
        assert!(migrated.contains("        type: http\n        method: post"));
        assert!(!migrated.contains("http_post"));
    }

    #[test]
    fn migrate_text_rejects_legacy_http_type_with_existing_method() {
        let text =
            LEGACY_YAML.replacen("  type: http_get\n", "  type: http_get\n  method: get\n", 1);
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("source.method"), "got: {err}");
    }

    #[test]
    fn migrate_text_preserves_http_get_text_inside_source_block_scalar() {
        let text = LEGACY_YAML.replacen(
            "  url: \"/api/usage\"\n",
            "  url: \"/api/usage\"\n  body: |\n    type: http_get\n",
            1,
        );
        let (migrated, changed) = migrate_text(&text).unwrap();
        assert!(changed);
        assert!(migrated.contains("        type: http\n        method: get"));
        assert!(migrated.contains("        body: |\n          type: http_get"));
        assert_eq!(migrated.matches("method: get").count(), 1);
    }

    #[test]
    fn migrate_text_rejects_multiple_direct_source_types() {
        let text = LEGACY_YAML.replacen(
            "  type: http_get\n",
            "  type: http_get\n  type: http_post\n",
            1,
        );
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("source.type"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_inline_source() {
        let text = LEGACY_YAML.replacen(
            "source:\n  type: http_get\n  url: \"/api/usage\"\n",
            "source: { type: http_get, url: \"/api/usage\" }\n",
            1,
        );
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("source"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_non_placeholder_without_parser() {
        let text = LEGACY_YAML.split("parser:\n").next().unwrap();
        let err = migrate_text(text).unwrap_err();
        assert!(err.to_string().contains("parser"), "got: {err}");
    }

    #[test]
    fn migrate_text_allows_placeholder_without_parser() {
        let text = r#"id: "placeholder"
schema_version: 1
metadata:
  display_name: "Placeholder"
source:
  type: placeholder
  reason: "Not configured"
"#;
        let (migrated, changed) = migrate_text(text).unwrap();
        assert!(changed);
        assert!(migrated.contains("schema_version: 2"));
        assert!(migrated.contains("        type: placeholder"));
    }

    #[test]
    fn migrate_text_separates_generated_plan_when_file_has_no_trailing_newline() {
        let text = concat!(
            "id: \"placeholder\"\n",
            "source:\n",
            "  type: placeholder\n",
            "  reason: \"Not configured\"\n",
            "metadata:\n",
            "  display_name: \"Placeholder\"",
        );
        let (migrated, changed) = migrate_text(text).unwrap();
        assert!(changed);
        assert!(migrated.contains("  display_name: \"Placeholder\"\nplan:\n"));
    }

    #[test]
    fn migrate_text_preserves_crlf() {
        let text = LEGACY_YAML.replace('\n', "\r\n");
        let (migrated, changed) = migrate_text(&text).unwrap();
        assert!(changed);
        assert!(!migrated.replace("\r\n", "").contains('\n'));
        assert!(migrated.contains("schema_version: 2\r\n"));
        assert!(migrated.contains("plan:\r\n"));
    }

    #[test]
    fn migrate_text_leaves_valid_v2_unchanged() {
        let (migrated, changed) = migrate_text(V2_YAML).unwrap();
        assert!(!changed);
        assert_eq!(migrated, V2_YAML);
    }

    #[test]
    fn migrate_text_rejects_v2_plan_without_id() {
        let text = V2_YAML.replacen("id: \"current\"\n", "", 1);
        let err = migrate_text(&text).unwrap_err();
        assert!(err.to_string().contains("id"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_v2_plan_without_steps() {
        let text = r#"id: "current"
schema_version: 2
metadata:
  display_name: "Current"
plan:
  mode: first_success
"#;
        let err = migrate_text(text).unwrap_err();
        assert!(err.to_string().contains("steps"), "got: {err}");
    }

    #[test]
    fn migrate_text_rejects_v2_plan_with_empty_inline_steps() {
        let text = r#"id: "current"
schema_version: 2
metadata:
  display_name: "Current"
plan:
  mode: first_success
  steps: []
"#;
        let err = migrate_text(text).unwrap_err();
        assert!(err.to_string().contains("steps"), "got: {err}");
    }
}
