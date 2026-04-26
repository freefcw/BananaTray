mod auth_status;
mod cached_plan;
mod sqlite_store;

use super::parse_strategy::{CacheParseStrategy, ParseStrategy};
use super::spec::CodeiumFamilySpec;
use super::LOCAL_CACHE_SOURCE_LABEL;
use crate::models::RefreshData;
use crate::providers::{ProviderError, ProviderResult};
use log::warn;
use rusqlite::{Connection, OpenFlags};

use auth_status::decode_user_status_payload;
use cached_plan::read_via_cached_plan_info;

pub(in crate::providers::codeium_family) use sqlite_store::{
    cache_db_path, query_auth_status_json,
};

pub fn is_available(spec: &CodeiumFamilySpec) -> bool {
    cache_db_path(spec).is_ok()
}

pub fn read_refresh_data(spec: &CodeiumFamilySpec) -> ProviderResult<RefreshData> {
    let db_path = cache_db_path(spec)?;
    let conn =
        Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|err| {
            ProviderError::unavailable(&format!(
                "cannot open {} cache DB: {} ({})",
                spec.log_label,
                db_path.display(),
                err
            ))
        })?;

    // 策略 1: 传统 protobuf 解析
    let proto_result = read_via_protobuf(&conn, spec);
    if proto_result.is_ok() {
        return proto_result;
    }
    let proto_err = proto_result.unwrap_err();

    // 策略 2: cachedPlanInfo JSON 回退（新版 Windsurf）
    if !spec.cached_plan_info_key_candidates.is_empty() {
        warn!(
            target: "providers",
            "{} protobuf decode failed: {}, trying cachedPlanInfo fallback",
            spec.log_label,
            proto_err
        );
        match read_via_cached_plan_info(&conn, spec) {
            Ok(data) => return Ok(data),
            Err(plan_err) => {
                warn!(
                    target: "providers",
                    "{} cachedPlanInfo fallback also failed: {}",
                    spec.log_label,
                    plan_err
                );
            }
        }
    }

    Err(proto_err)
}

fn read_via_protobuf(conn: &Connection, spec: &CodeiumFamilySpec) -> ProviderResult<RefreshData> {
    let auth_status_json = query_auth_status_json(conn, spec)?;
    let user_status_data = decode_user_status_payload(&auth_status_json)?;
    let strategy = CacheParseStrategy;
    let (quotas, email, plan_name) = strategy.parse(&user_status_data)?;
    Ok(RefreshData::with_account(quotas, email, plan_name)
        .with_source_label(LOCAL_CACHE_SOURCE_LABEL))
}

#[cfg(test)]
#[path = "cache_source_tests.rs"]
mod tests;
