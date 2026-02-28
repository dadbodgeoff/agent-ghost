/**
 * ChatGPT (chat.openai.com / chatgpt.com) adapter.
 *
 * DOM structure is subject to change — selectors may need updating
 * when OpenAI updates their frontend.
 */
import { BasePlatformAdapter } from "./base.js";

export class ChatGPTAdapter extends BasePlatformAdapter {
  get platformName() { return "chatgpt"; }

  static matches(url) {
    return /^https:\/\/(chat\.openai\.com|chatgpt\.com)/.test(url);
  }

  getMessageContainer() {
    // ChatGPT renders messages in a scrollable container
    return document.querySelector('[class*="react-scroll-to-bottom"]') ||
           document.querySelector("main .flex.flex-col");
  }

  getExistingMessages(container) {
    return Array.from(container.querySelectorAll('[data-message-author-role]'));
  }

  parseMessage(element) {
    const role = element.getAttribute?.("data-message-author-role");
    if (!role) {
      // Check children for message elements
      const child = element.querySelector?.("[data-message-author-role]");
      if (child) return this.parseMessage(child);
      return null;
    }

    const contentEl = element.querySelector(".markdown, .whitespace-pre-wrap");
    const content = contentEl?.textContent?.trim() || "";
    if (!content) return null;

    return {
      role: role === "user" ? "human" : "assistant",
      content,
    };
  }
}
