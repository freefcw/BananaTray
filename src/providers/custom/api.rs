//! Custom provider lifecycle API.
//!
//! Runtime 和 Settings UI 只应依赖这里的稳定入口：
//! - filename / YAML 生成
//! - 默认脚本模板
//! - edit-data 读取
//! - 保存 / 删除 custom provider 文件
//!
//! NewAPI / 脚本 provider 生命周期分别实现在 `newapi_lifecycle.rs` /
//! `script_provider_lifecycle.rs`；低层文件事务实现在 `file_ops.rs`。
//! schema / locator / generator 继续作为内部实现细节存在。

pub(crate) use crate::providers::custom::newapi_lifecycle::{
    delete_yaml as delete_newapi_yaml, generate_filename, read_config as read_newapi_config,
    save_yaml as save_newapi_yaml,
};
pub(crate) use crate::providers::custom::script_provider_lifecycle::{
    default_template as default_script_template, delete_files as delete_script_provider_files,
    generate_script_filename, generate_yaml_filename as generate_script_yaml_filename,
    read_config as read_script_provider_config, save as save_script_provider,
};
