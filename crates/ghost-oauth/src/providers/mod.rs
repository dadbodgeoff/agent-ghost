//! OAuth provider implementations for Google, GitHub, Slack, and Microsoft.

pub mod google;
pub mod github;
pub mod slack;
pub mod microsoft;

pub use google::GoogleOAuthProvider;
pub use github::GitHubOAuthProvider;
pub use slack::SlackOAuthProvider;
pub use microsoft::MicrosoftOAuthProvider;
