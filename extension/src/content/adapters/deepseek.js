/**
 * DeepSeek (chat.deepseek.com) adapter.
 */
import { BasePlatformAdapter } from "./base.js";

export class DeepSeekAdapter extends BasePlatformAdapter {
  get platformName() { return "deepseek"; }

  static matches(url) {
    return /^https:\/\/chat\.deepseek\.com/.test(url);
  }

  getMessageContainer() {
    return document.querySelector('[class*="chat-message-list"]') ||
           document.querySelector('[class*="conversation"]') ||
           document.querySelector("main");
  }

  getExistingMessages(container) {
    return Array.from(container.querySelectorAll('[class*="message"]'));
  }

  parseMessage(element) {
    const classes = element.className || "";
    let role = null;

    if (classes.includes("user") || classes.includes("human")) {
      role = "human";
    } else if (classes.includes("assistant") || classes.includes("bot")) {
      role = "assistant";
    }

    if (!role) return null;

    const content = element.textContent?.trim() || "";
    if (!content) return null;

    return { role, content };
  }
}
