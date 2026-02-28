/**
 * Claude.ai adapter.
 */
import { BasePlatformAdapter } from "./base.js";

export class ClaudeAdapter extends BasePlatformAdapter {
  get platformName() { return "claude"; }

  static matches(url) {
    return /^https:\/\/claude\.ai/.test(url);
  }

  getMessageContainer() {
    return document.querySelector('[class*="conversation-content"]') ||
           document.querySelector("main");
  }

  getExistingMessages(container) {
    return Array.from(container.querySelectorAll('[class*="message"]'));
  }

  parseMessage(element) {
    const classes = element.className || "";
    let role = null;

    if (classes.includes("human") || classes.includes("user")) {
      role = "human";
    } else if (classes.includes("assistant") || classes.includes("claude")) {
      role = "assistant";
    }

    if (!role) return null;

    const content = element.textContent?.trim() || "";
    if (!content) return null;

    return { role, content };
  }
}
