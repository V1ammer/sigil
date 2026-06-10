//! Settings page sub-components.

pub mod about;
pub mod account;
pub mod admin_invites;
pub mod admin_users;
pub mod appearance;
pub mod devices;
pub mod notifications;
pub mod privacy;
pub mod voice;

pub use about::AboutSettings;
pub use account::AccountSettings;
pub use admin_invites::AdminInvitesSettings;
pub use admin_users::AdminUsersSettings;
pub use appearance::AppearanceSettings;
pub use devices::DevicesSettings;
pub use notifications::NotificationsSettings;
pub use privacy::PrivacySettings;
pub use voice::VoiceSettings;
