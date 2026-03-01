//! Phase 5 tests for ghost-channels (Task 5.7).

use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════
// Task 5.7 — Channel Adapter Trait
// ═══════════════════════════════════════════════════════════════════════

mod adapter_trait {
    use ghost_channels::adapter::ChannelAdapter;
    use ghost_channels::adapters::cli::CliAdapter;
    use ghost_channels::adapters::websocket::WebSocketAdapter;
    use ghost_channels::adapters::telegram::TelegramAdapter;
    use ghost_channels::adapters::discord::DiscordAdapter;
    use ghost_channels::adapters::slack::SlackAdapter;
    use ghost_channels::adapters::whatsapp::WhatsAppAdapter;

    /// ChannelAdapter trait is object-safe (can be used as Box<dyn>).
    #[test]
    fn trait_is_object_safe() {
        let adapters: Vec<Box<dyn ChannelAdapter>> = vec![
            Box::new(CliAdapter::new()),
            Box::new(WebSocketAdapter::loopback()),
            Box::new(TelegramAdapter::new("test-token")),
            Box::new(DiscordAdapter::new("test-token")),
            Box::new(SlackAdapter::new("bot-token", "app-token")),
            Box::new(WhatsAppAdapter::new_sidecar()),
        ];
        assert_eq!(adapters.len(), 6);
    }

    #[test]
    fn channel_types() {
        assert_eq!(CliAdapter::new().channel_type(), "cli");
        assert_eq!(WebSocketAdapter::loopback().channel_type(), "websocket");
        assert_eq!(TelegramAdapter::new("t").channel_type(), "telegram");
        assert_eq!(DiscordAdapter::new("t").channel_type(), "discord");
        assert_eq!(SlackAdapter::new("b", "a").channel_type(), "slack");
        assert_eq!(WhatsAppAdapter::new_sidecar().channel_type(), "whatsapp");
    }

    #[test]
    fn streaming_support() {
        assert!(!CliAdapter::new().supports_streaming());
        assert!(WebSocketAdapter::loopback().supports_streaming());
        assert!(TelegramAdapter::new("t").supports_streaming());
    }

    #[test]
    fn editing_support() {
        assert!(!CliAdapter::new().supports_editing());
        assert!(WebSocketAdapter::loopback().supports_editing());
        assert!(TelegramAdapter::new("t").supports_editing());
        assert!(DiscordAdapter::new("t").supports_editing());
        assert!(SlackAdapter::new("b", "a").supports_editing());
        assert!(!WhatsAppAdapter::new_sidecar().supports_editing());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.7 — CLI Adapter
// ═══════════════════════════════════════════════════════════════════════

mod cli_adapter {
    use ghost_channels::adapter::ChannelAdapter;
    use ghost_channels::adapters::cli::CliAdapter;
    use ghost_channels::types::OutboundMessage;

    #[tokio::test]
    async fn connect_disconnect() {
        let mut adapter = CliAdapter::new();
        assert!(adapter.connect().await.is_ok());
        assert!(adapter.disconnect().await.is_ok());
    }

    #[tokio::test]
    async fn send_message() {
        let adapter = CliAdapter::new();
        let msg = OutboundMessage {
            content: "test output".into(),
            reply_to: None,
            attachments: Vec::new(),
        };
        assert!(adapter.send(msg).await.is_ok());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.7 — WebSocket Adapter
// ═══════════════════════════════════════════════════════════════════════

mod websocket_adapter {
    use ghost_channels::adapter::ChannelAdapter;
    use ghost_channels::adapters::websocket::WebSocketAdapter;

    #[tokio::test]
    async fn loopback_connect() {
        let mut adapter = WebSocketAdapter::loopback();
        assert!(adapter.connect().await.is_ok());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.7 — WhatsApp Adapter
// ═══════════════════════════════════════════════════════════════════════

mod whatsapp_adapter {
    use ghost_channels::adapters::whatsapp::WhatsAppAdapter;

    #[test]
    fn sidecar_restart_within_limit() {
        let mut adapter = WhatsAppAdapter::new_sidecar();
        assert!(adapter.restart_sidecar()); // 1st
        assert!(adapter.restart_sidecar()); // 2nd
        assert!(adapter.restart_sidecar()); // 3rd
    }

    #[test]
    fn sidecar_restart_exceeds_limit() {
        let mut adapter = WhatsAppAdapter::new_sidecar();
        adapter.restart_sidecar(); // 1
        adapter.restart_sidecar(); // 2
        adapter.restart_sidecar(); // 3
        assert!(!adapter.restart_sidecar()); // 4th — degraded
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.7 — Message Types
// ═══════════════════════════════════════════════════════════════════════

mod message_types {
    use ghost_channels::types::InboundMessage;

    #[test]
    fn inbound_message_normalizes() {
        let msg = InboundMessage::new("cli", "user", "hello");
        assert_eq!(msg.channel, "cli");
        assert_eq!(msg.sender, "user");
        assert_eq!(msg.content, "hello");
        assert!(msg.attachments.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Task 5.7 — Streaming Formatter
// ═══════════════════════════════════════════════════════════════════════

mod streaming {
    use std::time::Duration;
    use ghost_channels::streaming::StreamingFormatter;

    #[test]
    fn buffers_chunks() {
        let mut fmt = StreamingFormatter::new(Duration::from_millis(100));
        fmt.push_chunk("hello ");
        fmt.push_chunk("world");
        assert_eq!(fmt.peek(), "hello world");
    }

    #[test]
    fn flush_returns_content() {
        let mut fmt = StreamingFormatter::new(Duration::from_millis(0));
        fmt.push_chunk("data");
        let content = fmt.flush();
        assert_eq!(content, "data");
        assert!(fmt.is_empty());
    }

    #[test]
    fn throttle_respected() {
        let fmt = StreamingFormatter::new(Duration::from_secs(10));
        // Just created — should not flush yet (throttle is 10s)
        assert!(!fmt.should_flush());
    }

    #[test]
    fn immediate_throttle_flushes() {
        let fmt = StreamingFormatter::new(Duration::from_millis(0));
        // Zero throttle — should always be ready
        std::thread::sleep(Duration::from_millis(1));
        assert!(fmt.should_flush());
    }
}
