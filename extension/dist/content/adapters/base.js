/**
 * Base platform adapter — abstract class for DOM observation.
 */
export class BasePlatformAdapter {
    observeNewMessages(callback) {
        const selector = this.getMessageContainerSelector();
        let container = document.querySelector(selector);
        const emittedElements = new WeakSet();
        const emitParsedMessage = (element) => {
            if (emittedElements.has(element)) {
                return;
            }
            const msg = this.parseMessage(element);
            if (!msg) {
                return;
            }
            emittedElements.add(element);
            callback(msg);
        };
        const observer = new MutationObserver((mutations) => {
            if (!container || !container.isConnected) {
                container = document.querySelector(selector);
                if (container) {
                    observer.observe(container, { childList: true, subtree: true });
                }
            }
            for (const mutation of mutations) {
                for (const node of mutation.addedNodes) {
                    if (node instanceof Element) {
                        emitParsedMessage(node);
                        for (const descendant of node.querySelectorAll('*')) {
                            emitParsedMessage(descendant);
                        }
                    }
                }
            }
        });
        if (container) {
            observer.observe(container, { childList: true, subtree: true });
        }
        else if (document.body) {
            observer.observe(document.body, { childList: true, subtree: true });
        }
        return observer;
    }
    /** SHA-256 hash of content for privacy. */
    async hashContent(content) {
        const encoder = new TextEncoder();
        const data = encoder.encode(content);
        const hashBuffer = await crypto.subtle.digest('SHA-256', data);
        const hashArray = Array.from(new Uint8Array(hashBuffer));
        return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
    }
}
//# sourceMappingURL=base.js.map
