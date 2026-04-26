use super::super::spec::CodeiumFamilySpec;
use super::query_auth_status_json;
use crate::providers::{ProviderError, ProviderResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;

/// UserStatus protobuf 中 email 字段的 field number（来自 Codeium API schema）
const PROTO_FIELD_EMAIL: u32 = 7;

pub(super) fn decode_user_status_payload(auth_status_json: &str) -> ProviderResult<Vec<u8>> {
    let auth_status: serde_json::Value = serde_json::from_str(auth_status_json)
        .map_err(|e| ProviderError::parse_failed(&format!("invalid auth status JSON: {}", e)))?;

    let user_status_b64 = auth_status
        .get("userStatusProtoBinaryBase64")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ProviderError::parse_failed("missing userStatusProtoBinaryBase64 field"))?;

    STANDARD
        .decode(user_status_b64)
        .map_err(|e| ProviderError::parse_failed(&format!("invalid user status base64: {}", e)))
}

/// 从 auth status JSON 中提取用户 email。
///
/// 支持两种格式：
/// 1. 旧格式：JSON 顶层有 `email` 字段
/// 2. 新格式（当前 Windsurf）：JSON 只有 `userStatusProtoBinaryBase64`，
///    但 protobuf 中含非法 wire type 导致 prost::decode 整体失败。
///    用宽容扫描在遇到非法字节前提取 email field。
pub(super) fn extract_email_from_auth_status(
    conn: &Connection,
    spec: &CodeiumFamilySpec,
) -> Option<String> {
    let json_str = query_auth_status_json(conn, spec).ok()?;
    let v: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    // 旧格式：顶层 email 字段
    if let Some(email) = v
        .get("email")
        .and_then(|e| e.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
    {
        return Some(email);
    }

    // 新格式：从 protobuf 二进制做宽容扫描
    v.get("userStatusProtoBinaryBase64")
        .and_then(|b| b.as_str())
        .and_then(|b64| STANDARD.decode(b64).ok())
        .and_then(|bytes| extract_string_field_permissive(&bytes, PROTO_FIELD_EMAIL))
        .filter(|s| !s.is_empty())
}

/// 宽容扫描 protobuf 字节，提取指定 field_number 的第一个 length-delimited（wire=2）string 字段。
/// 遇到非法 wire type 时停止而不报错，确保在截断数据中也能提取已出现的字段。
pub(super) fn extract_string_field_permissive(data: &[u8], field_number: u32) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        let wire = b & 0x7;
        let field = (b >> 3) as u32;
        i += 1;

        match wire {
            2 => {
                // length-delimited：读 varint 长度
                let mut length: usize = 0;
                let mut shift = 0usize;
                loop {
                    if i >= data.len() {
                        return None;
                    }
                    let b2 = data[i];
                    i += 1;
                    length |= ((b2 & 0x7f) as usize) << shift;
                    if b2 & 0x80 == 0 {
                        break;
                    }
                    shift += 7;
                }
                if i + length > data.len() {
                    return None;
                }
                let val = &data[i..i + length];
                i += length;
                if field == field_number {
                    return std::str::from_utf8(val).ok().map(|s| s.to_string());
                }
            }
            0 => {
                // varint
                loop {
                    if i >= data.len() {
                        return None;
                    }
                    let b2 = data[i];
                    i += 1;
                    if b2 & 0x80 == 0 {
                        break;
                    }
                }
            }
            1 => {
                // 64-bit
                i += 8;
            }
            5 => {
                // 32-bit
                i += 4;
            }
            _ => {
                // 非法 wire type，停止扫描
                break;
            }
        }
    }
    None
}
