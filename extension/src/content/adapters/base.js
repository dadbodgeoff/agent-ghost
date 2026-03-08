/**
 * Base platform adapter. All platform-specific adapters extend this.
 */
export class BasePlatformAdapter {
  /** @returns {string} Platform display name */
  get platformName() { throw new Error("Not implemented"); }

  /**
   * @param {string} url
   * @returns {boolean} Whether this adapter handles the given URL
   */
  static matches(_url) { throw new Error("Not implemented"); }

  /**
   * @returns {Element|null} The DOM element containing the message list
   */
  getMessageContainer() { return null; }

  /**
   * @param {Element} container
   * @returns {Element[]} Existing message elements in the container
   */
  getExistingMessages(_container) { return []; }

  /**
   * @param {Element} element A DOM element that may be a message
   * @returns {{ role: string, content: string }|null} Parsed message or null
   */
  parseMessage(_element) { return null; }
}
