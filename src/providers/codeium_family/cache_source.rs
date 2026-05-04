mod auth_status;
mod cached_plan;
mod sqlite_store;

use super::parse_strategy::{CacheParseStrategy, ParseStrategy};
use super::spec::CodeiumFamilySpec;
use super::LOCAL_CACHE_SOURCE_LABEL;
use crate::models::RefreshData;
use crate::providers::{ProviderError, ProviderResult};
use log::{debug, warn};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use auth_status::decode_user_status_payload;
use cached_plan::read_via_cached_plan_info;

pub(in crate::providers::codeium_family) use sqlite_store::{
    cache_db_path, cache_db_path_candidates, query_auth_status_json,
};

/// 本地 quota cache source 是否可作为刷新来源。
///
/// 注意：这不是“是否存在可读取 auth DB”。Windsurf seat API 只需要从 DB 中读取 apiKey，
/// 因此 provider-level availability 会额外检查 `has_cache_db()`，避免陈旧 quota cache
/// 阻断云端 seat API 刷新。
pub fn is_available(spec: &CodeiumFamilySpec) -> bool {
    fresh_cache_db_path(spec).is_ok()
}

/// 是否存在本地 cache DB。只表达“有 DB 可尝试读取 auth / apiKey”，不承诺 quota 快照新鲜。
pub fn has_cache_db(spec: &CodeiumFamilySpec) -> bool {
    cache_db_path(spec).is_ok()
}

pub fn read_refresh_data(spec: &CodeiumFamilySpec) -> ProviderResult<RefreshData> {
    let db_path = fresh_cache_db_path(spec)?;

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

fn fresh_cache_db_path(spec: &CodeiumFamilySpec) -> ProviderResult<PathBuf> {
    select_fresh_cache_db_path(spec, cache_db_path_candidates(spec))
}

pub(super) fn select_fresh_cache_db_path(
    spec: &CodeiumFamilySpec,
    candidates: Vec<PathBuf>,
) -> ProviderResult<PathBuf> {
    if candidates.is_empty() {
        return Err(ProviderError::unavailable(
            "cannot determine config directory",
        ));
    }

    let mut stale_or_unreadable_error = None;

    for db_path in candidates {
        if !db_path.exists() {
            continue;
        }

        match ensure_cache_fresh(&db_path, spec) {
            Ok(()) => return Ok(db_path),
            Err(err) => {
                debug!(
                    target: "providers",
                    "{} local cache candidate skipped: {} ({})",
                    spec.log_label,
                    db_path.display(),
                    err
                );
                stale_or_unreadable_error = Some(err);
            }
        }
    }

    Err(stale_or_unreadable_error.unwrap_or_else(|| {
        ProviderError::unavailable(&format!(
            "{} local cache database not found",
            spec.log_label
        ))
    }))
}

/// 检查缓存 SQLite 文件最新 mtime，超过 spec.cache_max_age_secs 视为陈旧不可信。
///
/// 当 language server 长期未运行时，state.vscdb 不会被写入，里面的 quota 快照
/// 已经无法反映真实状态，必须直接拒绝以免误导用户。
///
/// 关键：VS Code/Electron 的 SQLite 走 WAL 模式，新写入会先到 `state.vscdb-wal`，
/// 主 DB 文件 mtime 在 checkpoint 之前可能远落后。因此我们取
/// `state.vscdb`、`state.vscdb-wal`、`state.vscdb-journal` 三者中**最新的 mtime**
/// 作为 cache 实际活跃时间，避免把"还在写"的 cache 误判为 stale。
pub(super) fn ensure_cache_fresh(db_path: &Path, spec: &CodeiumFamilySpec) -> ProviderResult<()> {
    if spec.cache_max_age_secs == 0 {
        return Ok(());
    }

    let (latest_mtime, source_path) = latest_cache_mtime(db_path).map_err(|err| {
        ProviderError::unavailable(&format!(
            "cannot determine {} cache mtime ({}): {}. \
             Try opening {} once to refresh local cache.",
            spec.log_label,
            db_path.display(),
            err,
            spec.display_name
        ))
    })?;

    let now = SystemTime::now();
    let age_secs = match now.duration_since(latest_mtime) {
        Ok(dur) => dur.as_secs(),
        Err(_) => {
            // mtime 在未来：时钟漂移 / 文件被恢复 / NTP 同步异常。
            // 不静默吞掉——明确 warn，并按 0 处理（视为"刚写入"）以免长期被锁死为 fresh。
            warn!(
                target: "providers",
                "{} cache mtime is in the future for {}; clock drift?",
                spec.log_label,
                source_path.display()
            );
            0
        }
    };

    if age_secs > spec.cache_max_age_secs {
        warn!(
            target: "providers",
            "{} local cache too stale: latest mtime source={}, age={}s exceeds threshold {}s; refusing to use",
            spec.log_label,
            source_path.display(),
            age_secs,
            spec.cache_max_age_secs
        );
        return Err(ProviderError::unavailable(&format!(
            "{} local cache is stale: {} last updated {}s ago (> {}s threshold). \
             Open {} once to refresh local cache.",
            spec.log_label,
            source_path.display(),
            age_secs,
            spec.cache_max_age_secs,
            spec.display_name
        )));
    }

    debug!(
        target: "providers",
        "{} local cache fresh: latest mtime source={}, age={}s within threshold {}s",
        spec.log_label,
        source_path.display(),
        age_secs,
        spec.cache_max_age_secs
    );
    Ok(())
}

/// 返回 (db / -wal / -journal) 三者中最新的 mtime 以及对应的文件路径。
///
/// 主 DB 文件必须存在（这是调用者的前置条件）；sidecar 不存在则忽略。
/// 任何能读到 mtime 的候选都会参与比较，"读不到"不报错而是跳过。
fn latest_cache_mtime(db_path: &Path) -> std::io::Result<(SystemTime, std::path::PathBuf)> {
    fn extend_extension(path: &Path, suffix: &str) -> std::path::PathBuf {
        let mut s = path.as_os_str().to_os_string();
        s.push(suffix);
        std::path::PathBuf::from(s)
    }

    let candidates = [
        db_path.to_path_buf(),
        extend_extension(db_path, "-wal"),
        extend_extension(db_path, "-journal"),
    ];

    let mut best: Option<(SystemTime, std::path::PathBuf)> = None;
    let mut last_err: Option<std::io::Error> = None;

    for path in candidates {
        match std::fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(mtime) => match &best {
                Some((cur, _)) if *cur >= mtime => {}
                _ => best = Some((mtime, path)),
            },
            Err(err) => {
                // sidecar 不存在是常态；只有所有候选都读不到时才报错
                last_err = Some(err);
            }
        }
    }

    best.ok_or_else(|| {
        last_err
            .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no candidate"))
    })
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
