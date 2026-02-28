# Delivery Architecture: How Users Actually Get Monitored

## The Core Problem

The people most at risk of convergence are using:
- ChatGPT web UI (chat.openai.com)
- Claude.ai
- Character.AI (web + mobile app)
- Replika (mobile app)
- Gemini (gemini.google.com)
- Various other web-based chatbots

They are NOT making API calls. They have zero observability into their own interaction patterns. The monitoring system needs to intercept and analyze these interactions without requiring the user to change how they interact.

Three delivery approaches, in order of feasibility and user-friendliness:

---

## Approach 1: Browser Extension (Recommended Primary)

### How It Works

A browser extension (Chrome/Firefox/Edge) that:
1. Detects when the user is on a supported AI chat platform
2. Reads the conversation DOM (messages, timestamps, sender)
3. Emits ITP events to the local convergence monitor
4. All processing happens locally — nothing leaves the machine

### Technical Architecture

```
┌─────────────────────────────────────────────┐
│              Browser                         │
│  ┌────────────────────────────────────────┐  │
│  │  AI Chat Platform (e.g. ChatGPT)      │  │
│  │  ┌──────────────────────────────────┐  │  │
│  │  │         Chat DOM                  │  │  │
│  │  └──────────┬───────────────────────┘  │  │
│  └─────────────┼──────────────────────────┘  │
│                │ Content Script reads DOM     │
│  ┌─────────────▼──────────────────────────┐  │
│  │  Convergence Monitor Extension         │  │
│  │                                        │  │
│  │  Content Script (per-platform):        │  │
│  │  - Observe DOM mutations               │  │
│  │  - Extract messages + timestamps       │  │
│  │  - Detect sender (human vs agent)      │  │
│  │  - Compute message metadata            │  │
│  │                                        │  │
│  │  Background Service Worker:            │  │
│  │  - Receive events from content script  │  │
│  │  - Compute ITP signals                 │  │
│  │  - Store session data (IndexedDB)      │  │
│  │  - Run convergence detection           │  │
│  │  - Trigger notifications               │  │
│  │  - Manage intervention UI              │  │
│  │                                        │  │
│  │  Dashboard (extension popup/tab):      │  │
│  │  - Show current session signals        │  │
│  │  - Historical trends                   │  │
│  │  - Configuration                       │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
         │ (optional) Native Messaging
         ▼
┌──────────────────────────────────────────────┐
│  Local Convergence Monitor (Rust binary)      │
│  - Heavy computation offloaded here           │
│  - Hard enforcement (session termination)     │
│  - Persistent storage                         │
│  - Cannot be bypassed by disabling extension  │
└──────────────────────────────────────────────┘
```

### Content Script Strategy (Per-Platform)

Each AI platform has a different DOM structure. Content scripts need platform-specific selectors.

```javascript
// Platform adapter interface
class PlatformAdapter {
  // Identify if we're on this platform
  static matches(url) { /* return boolean */ }
  
  // CSS selectors for message containers
  getMessageContainerSelector() {}
  
  // Extract message data from a DOM element
  parseMessage(element) {
    return {
      sender: 'human' | 'agent',
      content: string,        // or hash if privacy mode
      timestamp: Date,
      contentLength: number
    }
  }
  
  // Observe new messages (MutationObserver setup)
  observeNewMessages(callback) {}
}
```

Target platforms (priority order):
1. **ChatGPT** (chat.openai.com) — largest user base, most convergence risk
2. **Claude.ai** — growing user base, long-context conversations
3. **Character.AI** (character.ai) — highest parasocial risk, younger users
4. **Gemini** (gemini.google.com)
5. **Replika** (web version)
6. **DeepSeek** (chat.deepseek.com)
7. **Grok** (grok.x.ai)

### Privacy Model

The extension operates in three privacy modes:

| Mode | What's Captured | What's Stored | Detection Capability |
|------|----------------|---------------|---------------------|
| Metadata Only | Timestamps, message lengths, sender, session boundaries | Timing data only | Basic signals (duration, latency, frequency) |
| Hashed Content | Above + SHA-256 of message content | Hashes + metadata | Above + some pattern detection |
| Full Local | Above + plaintext content | Everything, locally | All signals including vocabulary convergence |

Default: **Metadata Only**. User explicitly opts up to higher levels.

### Advantages
- Easiest to install (Chrome Web Store / Firefox Add-ons)
- No system-level configuration needed
- Works on any OS
- Users already understand browser extensions
- Can show real-time dashboard in extension popup

### Disadvantages
- User can disable the extension (mitigated by native messaging to Rust monitor)
- DOM selectors break when platforms update their UI (maintenance burden)
- Can't monitor mobile apps
- Some platforms may detect and block the extension
- ToS concerns (see Legal section below)

### The Trust Problem

The malicious extension research from 2025 is both a warning and a validation:
- Urban VPN and others were caught stealing AI chat data from 8M+ users by intercepting DOM content and overriding fetch()/XMLHttpRequest
- This proves the technical approach works
- But it also means users will be (rightly) suspicious of any extension that reads their AI chats
- **Our differentiation: fully open source, auditable, local-only, no network calls**
- The extension should be buildable from source and verifiable

---

## Approach 2: Local HTTPS Proxy (Power User / Maximum Coverage)

### How It Works

A local proxy (built on mitmproxy or custom Rust implementation) that:
1. Sits between the browser and the internet
2. Intercepts HTTPS traffic to AI chat domains only
3. Parses request/response payloads to extract conversation data
4. Emits ITP events to the convergence monitor
5. Passes all traffic through unmodified (read-only interception)

### Technical Architecture

```
┌──────────────┐     ┌──────────────────────┐     ┌──────────────┐
│   Browser     │────▶│  Local Proxy          │────▶│  AI Platform  │
│              │◀────│  (localhost:8080)      │◀────│  Servers      │
│  Configured  │     │                        │     │              │
│  to use      │     │  - TLS termination     │     │              │
│  proxy       │     │  - Domain filtering    │     │              │
│              │     │  - Payload parsing     │     │              │
│              │     │  - ITP event emission  │     │              │
│              │     │  - Pass-through mode   │     │              │
└──────────────┘     └──────────┬─────────────┘     └──────────────┘
                                │
                                ▼
                     ┌──────────────────────┐
                     │  Convergence Monitor  │
                     │  (Rust binary)        │
                     └──────────────────────┘
```

### Implementation Options

#### Option A: mitmproxy + Python Addon

mitmproxy is the gold standard for HTTPS interception. It supports:
- HTTP/1, HTTP/2, HTTP/3, WebSockets
- Python addon scripts for custom processing
- Transparent proxy mode
- Domain filtering (only intercept AI chat domains)

```python
# mitmproxy addon sketch
from mitmproxy import http

AI_DOMAINS = [
    "chat.openai.com",
    "chatgpt.com", 
    "claude.ai",
    "character.ai",
    "gemini.google.com",
    "chat.deepseek.com",
]

class ConvergenceInterceptor:
    def request(self, flow: http.HTTPFlow):
        # Only process AI chat domains
        if not any(d in flow.request.host for d in AI_DOMAINS):
            return
        
        # Extract and emit ITP event for human message
        if is_chat_message(flow.request):
            emit_itp_event("human", parse_request(flow.request))
    
    def response(self, flow: http.HTTPFlow):
        if not any(d in flow.request.host for d in AI_DOMAINS):
            return
        
        # Extract and emit ITP event for agent response
        if is_chat_response(flow.response):
            emit_itp_event("agent", parse_response(flow.response))
```

#### Option B: Custom Rust Proxy

A lightweight Rust proxy that only handles the specific interception needed:
- Less overhead than full mitmproxy
- Tighter integration with the Rust convergence monitor
- Harder to build and maintain
- Could use `hyper` + `rustls` for the proxy layer

**Recommendation:** Start with mitmproxy + Python addon for prototyping. Move to Rust proxy for production if performance or integration demands it.

### Setup Requirements

The proxy approach requires:
1. Install a local CA certificate (for HTTPS interception)
2. Configure browser/system to use the proxy
3. Trust the local CA

This is a significant setup barrier but provides the most complete interception.

### Domain-Specific Payload Parsing

Each AI platform uses different API formats. The proxy needs parsers for each:

```
ChatGPT:
  POST https://chatgpt.com/backend-api/conversation
  Request: JSON with "messages" array
  Response: Server-Sent Events (SSE) stream

Claude.ai:
  POST https://claude.ai/api/organizations/{org}/chat_conversations/{id}/completion
  Request: JSON with "prompt"
  Response: SSE stream

Character.AI:
  WebSocket connection
  JSON messages with character/user turns

Gemini:
  POST https://gemini.google.com/_/BardChatUi/data/assistant.lamda.BardFrontendService/StreamGenerate
  Request: Protobuf-encoded
  Response: Streaming JSON
```

### Advantages
- Catches ALL traffic, not just DOM-visible content
- Works with any browser
- Can intercept WebSocket connections (Character.AI)
- Can intercept streaming responses (SSE)
- Harder to bypass than a browser extension
- Can also monitor desktop apps that use HTTP

### Disadvantages
- Complex setup (CA certificate installation, proxy configuration)
- Breaks some certificate pinning (some apps won't work through proxy)
- Higher technical barrier for non-developer users
- Can't easily monitor mobile apps (would need device-level proxy)
- mitmproxy is Python — performance overhead for high-throughput
- TLS interception has security implications (local CA could be compromised)

---

## Approach 3: Platform API Export + Analysis (Lowest Friction)

### How It Works

Instead of real-time interception, use platform-provided data export features:
1. User exports their conversation history from the platform (most platforms support this)
2. The convergence monitor ingests the export file
3. Retrospective analysis of all signals
4. Sets up baseline and alerts for future sessions

### Supported Exports

| Platform | Export Method | Format |
|----------|-------------|--------|
| ChatGPT | Settings → Data Controls → Export Data | JSON (full history) |
| Claude.ai | No official export (yet) | — |
| Character.AI | Settings → Privacy → Request Data | JSON |
| Gemini | Google Takeout | JSON |

### Advantages
- Zero setup complexity
- No interception, no proxy, no extension
- Uses official platform features
- No ToS concerns
- Works retroactively on existing conversations

### Disadvantages
- Not real-time — can't intervene during a session
- Export frequency is limited (ChatGPT: once per request, takes hours)
- Not all platforms support export
- Missing real-time signals (response latency, within-session patterns)
- By the time you export and analyze, the convergence event may have already happened

---

## Recommended Strategy: Layered Approach

```
┌─────────────────────────────────────────────────────┐
│  Layer 1: Browser Extension (Primary)                │
│  - Real-time monitoring for web-based chat platforms │
│  - Lowest barrier to entry                           │
│  - Dashboard + notifications                         │
│  - Connects to Layer 3 via Native Messaging          │
├─────────────────────────────────────────────────────┤
│  Layer 2: Local Proxy (Power Users)                  │
│  - Catches traffic extension might miss              │
│  - WebSocket interception                            │
│  - Desktop app monitoring                            │
│  - Feeds into Layer 3                                │
├─────────────────────────────────────────────────────┤
│  Layer 3: Convergence Monitor (Rust Core)            │
│  - Receives ITP events from Layer 1 and/or Layer 2  │
│  - Runs detection algorithms                         │
│  - Manages intervention escalation                   │
│  - Enforces hard boundaries                          │
│  - Persistent storage and trend analysis             │
├─────────────────────────────────────────────────────┤
│  Layer 4: Data Export Analysis (Supplementary)       │
│  - Retrospective analysis of exported conversations  │
│  - Baseline establishment from historical data       │
│  - Works even without real-time monitoring            │
└─────────────────────────────────────────────────────┘
```

Users start with Layer 1 (extension). Power users add Layer 2 (proxy). Layer 3 runs regardless. Layer 4 is available for anyone who wants retrospective analysis.

---

## Mobile App Monitoring

Mobile is the hardest surface. Character.AI and Replika are primarily mobile apps.

### Options
- **Android:** Local VPN app that routes traffic through on-device proxy (similar to NetGuard/AdGuard approach). No root required.
- **iOS:** Much harder. Network extension API is limited. Would need a companion app that acts as a local VPN.
- **Both:** Could build a companion app that the user manually logs sessions in (lowest tech, highest friction, but works)

### Recommendation
Mobile is Phase 2. Start with browser extension for web platforms. Mobile users can use the web versions of these platforms through the monitored browser.

---

## Legal & ToS Considerations

### Browser Extension
- Reading the DOM of a page you're visiting is standard browser extension behavior
- The user is reading their own conversation data
- No data leaves the device
- **Risk:** Platforms could argue this violates ToS if they prohibit "automated access" or "scraping"
- **Mitigation:** The extension reads the DOM passively (like a screen reader), doesn't automate interactions
- **Precedent:** Numerous chat export extensions exist on Chrome Web Store

### Local Proxy
- Intercepting your own HTTPS traffic is legal (it's your traffic, your machine)
- **Risk:** Platforms with certificate pinning may detect proxy and refuse to connect
- **Risk:** Some jurisdictions may have laws about intercepting encrypted communications, even your own
- **Mitigation:** The proxy is read-only, doesn't modify traffic, user explicitly installs and configures it

### Data Export
- Using official export features is explicitly supported by the platforms
- No legal risk

### Overall
- **Key principle:** The user is monitoring their own behavior with their own data on their own machine
- This is analogous to screen time monitoring apps (Apple Screen Time, Android Digital Wellbeing)
- Those apps monitor app usage patterns without platform permission
- Our extension monitors interaction patterns within specific apps

> [NEEDS LEGAL REVIEW]
> Specific ToS analysis for each platform
> Jurisdiction-specific considerations
> Whether "safety tool" framing provides any legal protection
> Whether open-source + non-commercial status matters

---

## Build Priority

1. **Browser extension (Chrome)** — MVP delivery vehicle, covers ChatGPT + Claude + Gemini
2. **Rust convergence monitor** — core detection engine, receives events from extension
3. **Extension dashboard** — visualization of signals and trends
4. **Firefox extension** — port from Chrome
5. **Data export analyzer** — retrospective analysis tool
6. **Local proxy (mitmproxy addon)** — power user option
7. **Mobile companion** — Phase 2

---

## Open Questions

- How do you handle platforms that use SSE (Server-Sent Events) for streaming? The extension sees the final DOM, not the stream. Is that sufficient?
- Should the extension inject any UI into the chat page itself (e.g., a small indicator showing current convergence score)?
- How do you handle incognito/private browsing? Extension may not have permission.
- What happens when a platform redesigns their UI? How fast can adapters be updated?
- Should there be a "panic button" in the extension that immediately terminates the session?
- How do you handle multi-tab scenarios (user has multiple AI chats open)?
