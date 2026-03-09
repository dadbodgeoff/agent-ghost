//! OAuth provider implementations for Google, GitHub, Slack, and Microsoft.

pub mod configurable;
pub mod github;
pub mod google;
pub mod microsoft;
pub mod slack;

pub use configurable::ConfigurableOAuthProvider;
pub use github::GitHubOAuthProvider;
pub use google::GoogleOAuthProvider;
pub use microsoft::MicrosoftOAuthProvider;
pub use slack::SlackOAuthProvider;
