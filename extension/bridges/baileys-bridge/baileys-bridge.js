#!/usr/bin/env node
/**
 * GHOST Platform — Baileys WhatsApp Bridge
 *
 * JSON-RPC stdin/stdout bridge to WhatsApp Web via @whiskeysockets/baileys.
 * Spawned by ghost-channels WhatsAppAdapter on connect().
 *
 * Protocol: newline-delimited JSON on stdin/stdout.
 * Each message is a JSON-RPC 2.0 request/response.
 *
 * Requires Node.js 18+.
 */

"use strict";

const { default: makeWASocket, useMultiFileAuthState, DisconnectReason } = require("@whiskeysockets/baileys");
const { Boom } = require("@hapi/boom");
const readline = require("readline");
const path = require("path");
const fs = require("fs");

// Auth state directory — per-agent, passed via GHOST_AGENT_NAME env.
const agentName = process.env.GHOST_AGENT_NAME || "default";
const authDir = process.env.GHOST_AUTH_DIR ||
  path.join(process.env.HOME || "~", ".ghost", "agents", agentName, "whatsapp-auth");

let sock = null;
let connected = false;

// --- JSON-RPC over stdin/stdout ---

const rl = readline.createInterface({ input: process.stdin, terminal: false });

function sendResponse(id, result, error) {
  const msg = { jsonrpc: "2.0", id };
  if (error) {
    msg.error = { code: error.code || -1, message: error.message || String(error) };
  } else {
    msg.result = result;
  }
  process.stdout.write(JSON.stringify(msg) + "\n");
}

function sendNotification(method, params) {
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
}

rl.on("line", async (line) => {
  let req;
  try {
    req = JSON.parse(line);
  } catch {
    sendResponse(null, null, { code: -32700, message: "Parse error" });
    return;
  }

  const { id, method, params } = req;

  try {
    switch (method) {
      case "connect":
        await handleConnect(id);
        break;
      case "disconnect":
        await handleDisconnect(id);
        break;
      case "send":
        await handleSend(id, params);
        break;
      case "health":
        sendResponse(id, { status: connected ? "connected" : "disconnected", uptime: process.uptime() });
        break;
      default:
        sendResponse(id, null, { code: -32601, message: `Unknown method: ${method}` });
    }
  } catch (err) {
    sendResponse(id, null, { code: -32000, message: err.message });
  }
});

rl.on("close", () => {
  cleanup();
  process.exit(0);
});

// --- WhatsApp Connection ---

async function handleConnect(id) {
  if (sock && connected) {
    sendResponse(id, { status: "already_connected" });
    return;
  }

  fs.mkdirSync(authDir, { recursive: true });
  const { state, saveCreds } = await useMultiFileAuthState(authDir);

  sock = makeWASocket({
    auth: state,
    printQRInTerminal: false,
    logger: { level: "silent", child: () => ({ level: "silent" }) },
  });

  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("connection.update", (update) => {
    const { connection, lastDisconnect, qr } = update;

    if (qr) {
      // Emit QR code for the parent process to display.
      sendNotification("qr", { qr });
    }

    if (connection === "close") {
      connected = false;
      const reason = new Boom(lastDisconnect?.error)?.output?.statusCode;
      const shouldReconnect = reason !== DisconnectReason.loggedOut;
      sendNotification("connection_closed", {
        reason: reason || "unknown",
        shouldReconnect,
      });
      if (shouldReconnect) {
        // Parent process (WhatsAppAdapter) handles restart logic.
        // We just exit and let it respawn us.
        process.exit(1);
      } else {
        process.exit(0);
      }
    }

    if (connection === "open") {
      connected = true;
      sendNotification("connected", { jid: sock.user?.id });
    }
  });

  sock.ev.on("messages.upsert", ({ messages, type: updateType }) => {
    if (updateType !== "notify") return;
    for (const msg of messages) {
      if (msg.key.fromMe) continue;
      const text = msg.message?.conversation ||
        msg.message?.extendedTextMessage?.text ||
        "";
      if (!text) continue;
      sendNotification("message", {
        from: msg.key.remoteJid,
        participant: msg.key.participant || null,
        text,
        timestamp: msg.messageTimestamp,
        messageId: msg.key.id,
      });
    }
  });

  sendResponse(id, { status: "connecting" });
}

async function handleDisconnect(id) {
  await cleanup();
  sendResponse(id, { status: "disconnected" });
}

async function handleSend(id, params) {
  if (!sock || !connected) {
    sendResponse(id, null, { code: -32000, message: "Not connected" });
    return;
  }
  const { to, text } = params || {};
  if (!to || !text) {
    sendResponse(id, null, { code: -32602, message: "Missing 'to' or 'text' parameter" });
    return;
  }
  const jid = to.includes("@") ? to : `${to}@s.whatsapp.net`;
  const sent = await sock.sendMessage(jid, { text });
  sendResponse(id, { messageId: sent.key.id, status: "sent" });
}

async function cleanup() {
  if (sock) {
    try { sock.end(); } catch { /* ignore */ }
    sock = null;
    connected = false;
  }
}

// Heartbeat: respond to parent process health checks.
// The parent sends {"jsonrpc":"2.0","id":"hb","method":"health"} periodically.
// If no response within 1s, parent considers bridge unhealthy.

// Graceful shutdown on SIGTERM/SIGINT.
process.on("SIGTERM", async () => { await cleanup(); process.exit(0); });
process.on("SIGINT", async () => { await cleanup(); process.exit(0); });

// Prevent unhandled rejections from crashing silently.
process.on("unhandledRejection", (err) => {
  sendNotification("error", { message: `Unhandled rejection: ${err.message}` });
});
