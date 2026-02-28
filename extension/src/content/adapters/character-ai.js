/**
 * Character.AI adapter.
 */
import { BasePlatformAdapter } from "./base.js";

export class CharacterAIAdapter extends BasePlatformAdapter {
  get platformName() { return "character_ai"; }

  static matches(url) {
    return /^https:\/\/character\.ai/.test(url);
  }

  getMessageContainer() {
    return document.querySelector('[class*="chat-messages"]') ||
           document.querySelector('[class*="msg-list"]') ||
           document.querySelector("main");
  }

  getExistingMessages(container) {
    return Array.from(container.querySelectorAll('[class*="msg"]'));
  }

  parseMessage(element) {
    const classes = element.className || "";
    let role = null;

    if (classes.includes("human") || classes.includes("user") || element.querySelector('[class*="human"]')) {
      role = "human";
    } else if (classes.includes("char") || classes.includes("bot") || element.querySelector('[class*="char"]')) {
      role = "assistant";
    }

    if (!role) return null;

    const content = element.textContent?.trim() || "";
    if (!content || content.length < 2) return null;

    return { role, content };
  }
}
