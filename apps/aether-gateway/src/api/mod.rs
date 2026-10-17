pub(crate) mod access_preview;
pub(crate) mod account;
pub(crate) use account::mount_account_routes;
pub(crate) mod ai;
pub(crate) mod avatar;
pub(crate) mod backend;
pub(crate) mod compatibility;
mod core;
pub(crate) mod diagnostics;
pub(crate) mod feedback;
pub(crate) mod leaderboard;
mod mcp;
mod ops;
pub(crate) mod response;

pub(crate) use ai::mount_ai_routes;
pub(crate) use avatar::mount_avatar_routes;
pub(crate) use backend::{
    mount_admin_routes, mount_internal_routes, mount_oauth_routes, mount_public_support_routes,
};
pub(crate) use core::mount_core_routes;
pub(crate) use feedback::mount_feedback_routes;
pub(crate) use leaderboard::mount_leaderboard_routes;
pub(crate) use mcp::mount_mcp_routes;
pub(crate) use ops::mount_operational_routes;
