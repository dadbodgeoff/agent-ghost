use crate::cli::error::CliError;
use crate::codex::{
    get_account_status, get_rate_limits, login_with_api_key_env, login_with_chatgpt,
    logout_account, CodexAccount, CodexLoginStart,
};

#[derive(Debug, Clone)]
pub struct CodexLoginArgs {
    pub api_key_env: Option<String>,
    pub wait: bool,
}

fn codex_error(error: crate::codex::CodexError) -> CliError {
    match error {
        crate::codex::CodexError::Auth(message) => CliError::Auth(message),
        crate::codex::CodexError::BinaryNotFound(message) => CliError::Config(message),
        crate::codex::CodexError::Spawn(message)
        | crate::codex::CodexError::Io(message)
        | crate::codex::CodexError::Protocol(message)
        | crate::codex::CodexError::Json(message) => CliError::Internal(message),
        crate::codex::CodexError::Server { message, .. } => CliError::Internal(message),
    }
}

pub async fn run_status() -> Result<(), CliError> {
    let status = get_account_status().await.map_err(codex_error)?;

    match status.account {
        Some(CodexAccount::ApiKey) => println!("Codex auth: API key"),
        Some(CodexAccount::Chatgpt { email, plan_type }) => {
            println!("Codex auth: ChatGPT");
            println!("Account: {email}");
            println!("Plan: {plan_type}");
        }
        None => {
            println!("Codex auth: not logged in");
            if status.requires_openai_auth {
                println!("OpenAI auth required: yes");
            }
        }
    }

    Ok(())
}

pub async fn run_login(args: CodexLoginArgs) -> Result<(), CliError> {
    if let Some(api_key_env) = args.api_key_env.as_deref() {
        let (_login, account) = login_with_api_key_env(api_key_env)
            .await
            .map_err(codex_error)?;
        println!("Codex API key login stored from {api_key_env}.");
        match account.account {
            Some(CodexAccount::Chatgpt { email, plan_type }) => {
                println!("Account: {email}");
                println!("Plan: {plan_type}");
            }
            Some(CodexAccount::ApiKey) | None => {}
        }
        return Ok(());
    }

    let (login, completion) = login_with_chatgpt(args.wait).await.map_err(codex_error)?;
    match login {
        CodexLoginStart::Chatgpt { auth_url, login_id } => {
            println!("Open this URL to finish Codex login:");
            println!("{auth_url}");
            println!("Login ID: {login_id}");
        }
        CodexLoginStart::ApiKey => {
            println!("Codex API key login completed.");
        }
        CodexLoginStart::ChatgptAuthTokens => {
            println!("Codex external token login completed.");
        }
    }

    if let Some(completion) = completion {
        if completion.success {
            println!("Codex login completed.");
        } else if let Some(error) = completion.error {
            return Err(CliError::Auth(error));
        } else {
            return Err(CliError::Auth(
                "Codex login did not complete successfully".into(),
            ));
        }
    } else {
        println!("Run `ghost codex status` after the browser flow completes.");
    }

    Ok(())
}

pub async fn run_logout() -> Result<(), CliError> {
    logout_account().await.map_err(codex_error)?;
    println!("Codex account logged out.");
    Ok(())
}

pub async fn run_rate_limits() -> Result<(), CliError> {
    let limits = get_rate_limits().await.map_err(codex_error)?;
    let body = serde_json::json!({
        "rate_limits": limits.rate_limits,
        "rate_limits_by_limit_id": limits.rate_limits_by_limit_id,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&body)
            .map_err(|error| CliError::Internal(format!("serialize rate limits: {error}")))?
    );
    Ok(())
}
