//! OAuth provider implementations for Google, GitHub, Slack, and Microsoft.

pub mod github;
pub mod google;
pub mod microsoft;
pub mod slack;

pub use github::GitHubOAuthProvider;
pub use google::GoogleOAuthProvider;
pub use microsoft::MicrosoftOAuthProvider;
pub use slack::SlackOAuthProvider;
