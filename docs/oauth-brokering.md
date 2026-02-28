# OAuth Brokering

GHOST agents can access third-party APIs (GitHub, Google, Slack, Microsoft) via
the OAuth broker. The agent never sees raw tokens — it uses opaque reference IDs
(`OAuthRefId`) and the broker injects credentials at execution time.

## Setup

### 1. Register an OAuth App

Register an OAuth application with your provider. Set the callback URL to:

```
http://127.0.0.1:18789/api/oauth/callback
```

### 2. Configure in ghost.yml

```yaml
oauth:
  providers:
    github:
      client_id: "your-client-id"
      client_secret_key: "GITHUB_CLIENT_SECRET"  # Key in SecretProvider
      auth_url: "https://github.com/login/oauth/authorize"
      token_url: "https://github.com/login/oauth/access_token"
      scopes:
        repo: ["repo"]
        user: ["user:email"]
    google:
      client_id: "your-client-id"
      client_secret_key: "GOOGLE_CLIENT_SECRET"
      auth_url: "https://accounts.google.com/o/oauth2/v2/auth"
      token_url: "https://oauth2.googleapis.com/token"
      revoke_url: "https://oauth2.googleapis.com/revoke"
      scopes:
        email: ["gmail.readonly"]
        calendar: ["calendar"]
```

### 3. Store the Client Secret

```bash
# Using env provider:
export GITHUB_CLIENT_SECRET="your-secret"

# Using keychain provider:
ghost secrets set GITHUB_CLIENT_SECRET
```

## Connect / Disconnect Flow

### Connect

1. Agent (or user via dashboard) initiates: `POST /api/oauth/connect`
2. Gateway generates PKCE challenge, returns authorization URL
3. User authorizes in browser, provider redirects to callback
4. Gateway exchanges code for tokens, stores encrypted, returns `OAuthRefId`

### Disconnect

1. `POST /api/oauth/disconnect` with `ref_id`
2. Gateway revokes token at provider (if supported), deletes local storage

## Agent Tool Usage

Agents use the `oauth_api_call` tool with the opaque `ref_id`:

```json
{
  "tool": "oauth_api_call",
  "arguments": {
    "ref_id": "550e8400-e29b-41d4-a716-446655440000",
    "method": "GET",
    "url": "https://api.github.com/user/repos"
  }
}
```

The broker injects the Bearer token and executes the request. The agent
receives the response body but never the token itself.

## Security Model

- Tokens are `SecretString` (zeroized on drop)
- Stored encrypted on disk via `ghost-secrets`
- Agent only sees `OAuthRefId` (UUID), never raw tokens
- PKCE (S256) used for all authorization code flows
- Kill switch `QUARANTINE` revokes all OAuth connections for the agent
- Kill switch `KILL_ALL` revokes all connections platform-wide
- Token auto-refresh is transparent to the agent
