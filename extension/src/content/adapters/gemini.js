/**
 * Google Gemini (gemini.google.com) adapter.
 */
import { BasePlatformAdapter } from "./base.js";

export class GeminiAdapter extends BasePlatformAdapter {
  get platformName() { return "gemini"; }

  static matches(url) {
    return /^https:\/\/gemini\.google\.com/.test(url);
  }

  getMessageContainer() {
    return document.querySelector("chat-window") ||
           document.querySelector('[class*="conversation"]') ||
           document.querySelector("main");
  }

  getExistingMessages(container) {
    return Array.from(container.querySelectorAll("message-content, [class*='message']"));
  }

  parseMessage(element) {
    const tag = element.tagName?.toLowerCase() || "";
    const classes = element.className || "";
    let role = null;

    if (classes.includes("user") || classes.includes("query")) {
      role = "human";
    } else if (classes.includes("model") || classes.includes("response")) {
      role = "assistant";
    }

    if (!role) return null;

    const content = element.textContent?.trim() || "";
    if (!content) return null;

    return { role, content };
  }
}
