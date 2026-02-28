/**
 * Grok (grok.x.ai) adapter.
 */
import { BasePlatformAdapter } from "./base.js";

export class GrokAdapter extends BasePlatformAdapter {
  get platformName() { return "grok"; }

  static matches(url) {
    return /^https:\/\/grok\.x\.ai/.test(url);
  }

  getMessageContainer() {
    return document.querySelector('[class*="message-list"]') ||
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
    } else if (classes.includes("assistant") || classes.includes("grok")) {
      role = "assistant";
    }

    if (!role) return null;

    const content = element.textContent?.trim() || "";
    if (!content) return null;

    return { role, content };
  }
}
