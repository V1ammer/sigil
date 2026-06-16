#![forbid(unsafe_code)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::unused_unit)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::similar_names)]
#![allow(clippy::type_complexity)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::wildcard_imports)]
#![warn(unused_imports)]
#![allow(unused_variables)]
#![warn(dead_code)]
#![allow(clippy::unit_arg)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::map_identity)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::map_clone)]
#![allow(clippy::redundant_closure)]

pub mod app;
pub mod routes;
pub mod theme;
pub mod i18n;
pub mod icons;
pub mod mock;
pub mod components;
pub mod chat;
pub mod sidebar;
pub mod settings;
pub mod screens;
pub mod state;
pub mod session;
pub mod sound;
pub mod notify;
pub mod media_transcode;
pub mod tauri_bridge;

pub use app::App;
