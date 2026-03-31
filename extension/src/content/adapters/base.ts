/**
 * Base platform adapter — abstract class for DOM observation.
 */

export interface ParsedMessage {
  role: 'human' | 'assistant';
  content: string;
  timestamp: Date;
}

export abstract class BasePlatformAdapter {
  abstract readonly platform: string;
  abstract matches(url: string): boolean;
  abstract getMessageContainerSelector(): string;
  abstract parseMessage(element: Element): ParsedMessage | null;

  observeNewMessages(callback: (msg: ParsedMessage) => void): MutationObserver {
    const selector = this.getMessageContainerSelector();
    const parseCandidate = (candidate: Element) => {
      const msg = this.parseMessage(candidate);
      if (msg) {
        callback(msg);
      }
    };

    const inspectNode = (node: Node) => {
      if (!(node instanceof Element)) {
        return;
      }
      parseCandidate(node);
      for (const child of node.querySelectorAll('*')) {
        parseCandidate(child);
      }
    };

    const messageObserver = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        for (const node of mutation.addedNodes) {
          inspectNode(node);
        }
      }
    });

    const attachToContainer = (container: Element) => {
      messageObserver.observe(container, { childList: true, subtree: true });
    };

    const container = document.querySelector(selector);
    if (container) {
      attachToContainer(container);
      return messageObserver;
    }

    const bootstrapObserver = new MutationObserver(() => {
      const nextContainer = document.querySelector(selector);
      if (!nextContainer) {
        return;
      }

      bootstrapObserver.disconnect();
      attachToContainer(nextContainer);
    });

    const root = document.body ?? document.documentElement;
    if (root) {
      bootstrapObserver.observe(root, { childList: true, subtree: true });
    }

    return {
      observe() {
        // No-op: observation is handled by the inner observers above.
      },
      disconnect() {
        bootstrapObserver.disconnect();
        messageObserver.disconnect();
      },
      takeRecords() {
        return messageObserver.takeRecords();
      },
    };
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
