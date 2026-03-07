/**
 * Base platform adapter — abstract class for DOM observation.
 */
export class BasePlatformAdapter {
    observeNewMessages(callback) {
        const selector = this.getMessageContainerSelector();
        const container = document.querySelector(selector);
        const observer = new MutationObserver((mutations) => {
            for (const mutation of mutations) {
                for (const node of mutation.addedNodes) {
                    if (node instanceof Element) {
                        const msg = this.parseMessage(node);
                        if (msg) {
                            callback(msg);
                        }
                    }
                }
            }
        });
        if (container) {
            observer.observe(container, { childList: true, subtree: true });
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