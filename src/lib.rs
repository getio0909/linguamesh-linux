#![doc = "`LinguaMesh` 原生客户端的可测试应用层。"]

pub mod model;

#[cfg(feature = "gui")]
pub mod localization;

#[cfg(feature = "demo-provider")]
pub mod worker;

#[cfg(feature = "gui")]
pub mod secret_service;
