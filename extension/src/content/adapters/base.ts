/**
 * Base platform adapter — abstract class for DOM observation.
 */

export interface ParsedMessage {
  role: 'human' | 'assistant';
  content: string;
  timestamp: Date;
}

export abstract class BasePlatformAdapter {
  abstract matches(url: string): boolean;
  abstract getMessageContainerSelector(): string;
  abstract parseMessage(element: Element): ParsedMessage | null;

  observeNewMessages(callback: (msg: ParsedMessage) => void): MutationObserver {
    const selector = this.getMessageContainerSelector();
    let container = document.querySelector(selector);
    const seenElements = new WeakSet<Element>();

    const parseCandidate = (candidate: Element) => {
      if (seenElements.has(candidate)) {
        return;
      }

      const msg = this.parseMessage(candidate);
      if (msg) {
        seenElements.add(candidate);
        callback(msg);
      }
    };

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (!container) {
          const nextContainer = document.querySelector(selector);
          if (nextContainer) {
            container = nextContainer;
            observer.observe(container, { childList: true, subtree: true });
          }
        }

        for (const node of mutation.addedNodes) {
          if (node instanceof Element) {
            parseCandidate(node);
            for (const child of node.querySelectorAll('*')) {
              parseCandidate(child);
            }
          }
        }
      }
    });

    if (container) {
      observer.observe(container, { childList: true, subtree: true });
    } else {
      observer.observe(document.body, { childList: true, subtree: true });
    }

    return observer;
  }

  /** SHA-256 hash of content for privacy. */
  async hashContent(content: string): Promise<string> {
    const encoder = new TextEncoder();
    const data = encoder.encode(content);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
  }
}
